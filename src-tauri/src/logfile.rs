//! Read-only parser for FFXIV binary chat logs, plus filtering and export.
//!
//! File layout: 8-byte header (first / last record numbers, LE u32), a cumulative
//! end-offset table, then the record bodies.  Data after the last offset is stale
//! and ignored.  Record: u32 unix time, u32 code (kind = low 7 bits), 0x1F,
//! sender, 0x1F, message.

use crate::channels::name_of;
use crate::sestring::{decode_text, packed_uint};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Entry {
    pub timestamp: u32,
    /// Days since 1970-01-01 in JST.
    pub day: i32,
    pub channel: u8,
    pub code: u32,
    pub sender: String,
    pub message: String,
    /// Lower-cased "sender\nmessage" for searching.
    pub hay: String,
    pub character: Arc<str>,
    pub source: Arc<str>,
    pub record: u32,
    pub offset: usize,
    /// The whole record, header included.
    pub raw: Vec<u8>,
    pub warnings: String,
}

pub struct FileReport {
    pub trailing_bytes_ignored: usize,
}

// ---- dates (JST, no time-zone database needed) -------------------------------

const JST_SECS: i64 = 9 * 3600;

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn day_of(timestamp: u32) -> i32 {
    ((i64::from(timestamp) + JST_SECS).div_euclid(86_400)) as i32
}

pub fn date_string(day: i32) -> String {
    let (y, m, d) = civil_from_days(i64::from(day));
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn iso_jst(timestamp: u32) -> String {
    let secs = i64::from(timestamp) + JST_SECS;
    let rem = secs.rem_euclid(86_400);
    format!("{}T{:02}:{:02}:{:02}+09:00", date_string(secs.div_euclid(86_400) as i32), rem / 3600, rem % 3600 / 60, rem % 60)
}

/// "YYYY-MM-DD" -> day number; empty -> None.
pub fn parse_date(text: &str) -> Result<Option<i32>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let bad = || format!("日付の形式が不正です: {text}");
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err(bad());
    }
    let (y, m, d) = (
        parts[0].parse::<i64>().map_err(|_| bad())?,
        parts[1].parse::<u32>().map_err(|_| bad())?,
        parts[2].parse::<u32>().map_err(|_| bad())?,
    );
    let day = days_from_civil(y, m, d);
    if !(1..=12).contains(&m) || date_string(day as i32) != text {
        return Err(bad());
    }
    Ok(Some(day as i32))
}

// ---- parsing -----------------------------------------------------------------

/// The separator can also occur inside a binary link payload.
pub fn split_fields(data: &[u8]) -> Result<(&[u8], &[u8]), String> {
    let mut pos = 0;
    while pos < data.len() {
        match data[pos] {
            0x1F => return Ok((&data[..pos], &data[pos + 1..])),
            2 => {
                let (size, start) = packed_uint(data, pos + 2)?;
                let end = start + size as usize;
                if end >= data.len() || data[end] != 3 {
                    return Err("発言者の制御データが不正です".into());
                }
                pos = end + 1;
            }
            _ => pos += 1,
        }
    }
    Err("本文区切りがありません".into())
}

fn le32(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Canonical path without the Windows `\\?\` prefix.
fn plain_path(path: &Path) -> String {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy().into_owned();
    text.strip_prefix(r"\\?\").map_or(text.clone(), str::to_owned)
}

/// The records of a game log file: (offset in the file, record bytes), plus the numbers of
/// the first and the one-past-last record and the stale bytes after the last offset.
struct GameLog<'a> {
    first: u32,
    last: u32,
    records: Vec<(usize, &'a [u8])>,
    trailing: usize,
}

fn split_game_log(data: &[u8]) -> Result<GameLog<'_>, String> {
    if data.len() < 8 {
        return Err("8バイトのヘッダーがありません".into());
    }
    let (first, last) = (le32(data, 0), le32(data, 4));
    if last < first {
        return Err("レコード数またはヘッダー長が不正です".into());
    }
    let count = (last - first) as usize;
    let base = 8 + count * 4;
    if count > 1_000_000 || base > data.len() {
        return Err("レコード数またはヘッダー長が不正です".into());
    }
    let mut previous = 0usize;
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let end = le32(data, 8 + index * 4) as usize;
        if end < previous || base + end > data.len() {
            return Err(format!("行{}: オフセットが範囲外です", index + 1));
        }
        records.push((base + previous, &data[base + previous..base + end]));
        previous = end;
    }
    Ok(GameLog { first, last, records, trailing: data.len() - base - previous })
}

/// The folder name two levels above a log file is the character it belongs to.
fn character_of(path: &Path) -> Arc<str> {
    path.parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
        .into()
}

fn entry_from(raw: &[u8], character: &Arc<str>, source: &Arc<str>, record: u32, offset: usize) -> Result<Entry, String> {
    if raw.len() < 10 || raw[8] != 0x1F {
        return Err("レコードの構造が不正です".into());
    }
    let (timestamp, code) = (le32(raw, 0), le32(raw, 4));
    let (who, text) = split_fields(&raw[9..])?;
    let (sender, mut warnings) = decode_text(who);
    let (message, more) = decode_text(text);
    warnings.extend(more);
    warnings.sort_unstable();
    warnings.dedup();
    let hay = format!("{sender}\n{message}").to_lowercase();
    Ok(Entry {
        timestamp,
        day: day_of(timestamp),
        channel: (code & 0x7F) as u8,
        code,
        sender,
        message,
        hay,
        character: character.clone(),
        source: source.clone(),
        record,
        offset,
        raw: raw.to_vec(),
        warnings: warnings.join("; "),
    })
}

pub fn parse_file(path: &Path) -> Result<(Vec<Entry>, FileReport), String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let source: Arc<str> = plain_path(path).into();
    let character = character_of(path);
    let log = split_game_log(&data)?;
    let mut entries = Vec::with_capacity(log.records.len());
    for (index, (offset, raw)) in log.records.iter().enumerate() {
        entries.push(entry_from(raw, &character, &source, log.first + index as u32, *offset).map_err(|e| format!("行{}: {e}", index + 1))?);
    }
    Ok((entries, FileReport { trailing_bytes_ignored: log.trailing }))
}

// ---- the archive's own format ----------------------------------------------------
//
// Every session (one game log file) is kept as an append-only stream: a 24-byte header
// (magic, unix time of the first record, number of the next record to expect, record
// count, byte length of the data) followed by records, each framed with a u32 length.
// Importing a newer version of the same log only appends the records not stored yet.

const ARCHIVE_MAGIC: &[u8; 4] = b"ALG1";
const ARCHIVE_HEADER: usize = 24;

#[derive(Clone, Copy)]
struct ArchiveHeader {
    stamp: u32,
    next: u32,
    count: u32,
    len: u64,
}

impl ArchiveHeader {
    fn read(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < ARCHIVE_HEADER || &bytes[..4] != ARCHIVE_MAGIC {
            return None;
        }
        let len = u64::from_le_bytes(bytes[16..24].try_into().ok()?);
        Some(Self { stamp: le32(bytes, 4), next: le32(bytes, 8), count: le32(bytes, 12), len })
    }

    fn to_bytes(self) -> [u8; ARCHIVE_HEADER] {
        let mut out = [0u8; ARCHIVE_HEADER];
        out[..4].copy_from_slice(ARCHIVE_MAGIC);
        out[4..8].copy_from_slice(&self.stamp.to_le_bytes());
        out[8..12].copy_from_slice(&self.next.to_le_bytes());
        out[12..16].copy_from_slice(&self.count.to_le_bytes());
        out[16..24].copy_from_slice(&self.len.to_le_bytes());
        out
    }
}

pub fn parse_archive(path: &Path) -> Result<(Vec<Entry>, FileReport), String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let header = ArchiveHeader::read(&data).ok_or("アーカイブのヘッダーが不正です")?;
    let end = ARCHIVE_HEADER + header.len as usize;
    if end > data.len() {
        return Err("アーカイブが途中で切れています".into());
    }
    let source: Arc<str> = plain_path(path).into();
    let character = character_of(path);
    let mut entries = Vec::with_capacity(header.count as usize);
    let mut pos = ARCHIVE_HEADER;
    for index in 0..header.count {
        if pos + 4 > end {
            return Err(format!("行{}: アーカイブが壊れています", index + 1));
        }
        let size = le32(&data, pos) as usize;
        if pos + 4 + size > end {
            return Err(format!("行{}: アーカイブが壊れています", index + 1));
        }
        entries.push(entry_from(&data[pos + 4..pos + 4 + size], &character, &source, index, pos + 4).map_err(|e| format!("行{}: {e}", index + 1))?);
        pos += 4 + size;
    }
    Ok((entries, FileReport { trailing_bytes_ignored: data.len() - end }))
}

// ---- finding files ---------------------------------------------------------------

/// Game logs under `root` (`.log`).
pub fn find_logs(root: &Path) -> Result<Vec<PathBuf>, String> {
    find_files(root, None, &["log"])
}

/// Folders that never hold the game's logs; a scan of a big parent folder (the game's
/// document folder can contain this very project) must not descend into them.
const SKIPPED_DIRS: [&str; 4] = ["node_modules", "target", ".git", "demo-logs"];

/// Files with one of `exts` below `root`, never looking inside `skip` (the archive, which
/// can sit below the folder being scanned and would otherwise be imported into itself).
fn find_files(root: &Path, skip: Option<&Path>, exts: &[&str]) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, skip: Option<&Path>, exts: &[&str], root_is_demo: bool, out: &mut Vec<PathBuf>) {
        let Ok(read) = fs::read_dir(dir) else { return };
        for item in read.flatten() {
            let path = item.path();
            let Ok(kind) = item.file_type() else { continue };
            if kind.is_dir() {
                let name = item.file_name();
                let name = name.to_string_lossy();
                let skipped_name = SKIPPED_DIRS.iter().any(|d| name.eq_ignore_ascii_case(d) && !(root_is_demo && *d == "demo-logs"));
                let is_skip = skip.is_some_and(|s| fs::canonicalize(&path).is_ok_and(|p| p == s));
                if !skipped_name && !is_skip {
                    walk(&path, skip, exts, root_is_demo, out);
                }
            } else if kind.is_file() && path.extension().is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x))) {
                out.push(path);
            }
        }
    }
    if root.is_file() {
        return Ok(vec![root.to_path_buf()]);
    }
    if !root.is_dir() {
        return Err(format!("パスが見つかりません: {}", root.display()));
    }
    let mut out = Vec::new();
    let skip = skip.and_then(|s| fs::canonicalize(s).ok());
    let root_is_demo = root.components().any(|c| c.as_os_str().eq_ignore_ascii_case("demo-logs"));
    walk(root, skip.as_deref(), exts, root_is_demo, &mut out);
    out.sort();
    Ok(out)
}

pub struct Loaded {
    pub entries: Vec<Entry>,
    pub reports: Vec<FileReport>,
    pub errors: Vec<String>,
}

/// Reads everything under `root`: archive streams (`.alog`) and plain game logs (`.log`).
pub fn load_logs(root: &Path, progress: impl Fn(usize, usize)) -> Result<Loaded, String> {
    let paths = find_files(root, None, &["alog", "log"])?;
    let mut loaded = Loaded { entries: Vec::new(), reports: Vec::new(), errors: Vec::new() };
    for (i, path) in paths.iter().enumerate() {
        let is_archive = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("alog"));
        // A malformed file may not be able to take the whole load down with it.
        let parsed = std::panic::catch_unwind(|| if is_archive { parse_archive(path) } else { parse_file(path) })
            .unwrap_or_else(|_| Err("解析中に内部エラーが発生しました".to_owned()));
        match parsed {
            Ok((rows, report)) => {
                loaded.entries.extend(rows);
                loaded.reports.push(report);
            }
            Err(e) => loaded.errors.push(format!("{}: {e}", path.display())),
        }
        progress(i + 1, paths.len());
    }
    loaded.entries.sort_by(|a, b| {
        (a.timestamp, &a.source, a.record).cmp(&(b.timestamp, &b.source, b.record))
    });
    Ok(loaded)
}

// ---- local archive -------------------------------------------------------------

/// Keeps file names inside the archive: letters, digits, `_`, `-` and `.` only.
fn safe_name(text: &str) -> String {
    let cleaned: String = text.chars().map(|c| if c.is_ascii_alphanumeric() || "_-.".contains(c) { c } else { '_' }).collect();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '.') { "unknown".into() } else { cleaned }
}

#[derive(Default, Debug)]
pub struct ArchiveReport {
    /// Sessions that gained records.
    pub copied: usize,
    /// Sessions that were already up to date.
    pub unchanged: usize,
    /// Records appended in total.
    pub records: usize,
    pub errors: Vec<String>,
}

/// Successful source snapshots only; failed or changing writes are retried on the next tick.
#[derive(Default)]
pub struct SourceCache(HashMap<PathBuf, (u64, std::time::SystemTime)>);

/// Appends the records of `log` that the stream at `dest` does not hold yet and returns how
/// many that was.  A stream that does not exist is created.
fn import_session(dest: &Path, log: &GameLog<'_>, stamp: u32) -> Result<usize, String> {
    use std::io::{Read, Seek, SeekFrom};
    let existing = match fs::File::open(dest) {
        Ok(mut file) => {
            let mut head = [0u8; ARCHIVE_HEADER];
            file.read_exact(&mut head).map_err(|e| e.to_string())?;
            Some(ArchiveHeader::read(&head).ok_or("アーカイブのヘッダーが不正です")?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    let next = existing.map_or(0, |h| h.next);
    let mut chunk = Vec::new();
    let mut added = 0u32;
    for (index, (_, raw)) in log.records.iter().enumerate() {
        if log.first + index as u32 >= next {
            chunk.extend((raw.len() as u32).to_le_bytes());
            chunk.extend(*raw);
            added += 1;
        }
    }
    if added == 0 {
        return Ok(0);
    }
    match existing {
        Some(old) => {
            let mut file = fs::OpenOptions::new().read(true).write(true).open(dest).map_err(|e| e.to_string())?;
            // Anything after the recorded length is leftovers of an interrupted write.
            file.seek(SeekFrom::Start(ARCHIVE_HEADER as u64 + old.len)).map_err(|e| e.to_string())?;
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            let header = ArchiveHeader { stamp: old.stamp, next: old.next.max(log.last), count: old.count + added, len: old.len + chunk.len() as u64 };
            file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
            file.write_all(&header.to_bytes()).map_err(|e| e.to_string())?;
            file.set_len(ARCHIVE_HEADER as u64 + header.len).map_err(|e| e.to_string())?;
        }
        None => {
            fs::create_dir_all(dest.parent().ok_or("保存先が不正です")?).map_err(|e| e.to_string())?;
            let header = ArchiveHeader { stamp, next: log.last, count: added, len: chunk.len() as u64 };
            let temp = dest.with_extension("tmp");
            let mut bytes = header.to_bytes().to_vec();
            bytes.extend(chunk);
            fs::write(&temp, bytes).map_err(|e| e.to_string())?;
            fs::rename(&temp, dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(added as usize)
}

/// Unix time of a log's first record, or None for an empty or unusable log.
fn first_stamp(log: &GameLog<'_>) -> Option<u32> {
    log.records.first().filter(|(_, raw)| raw.len() >= 4).map(|(_, raw)| le32(raw, 0))
}

/// Earlier versions kept whole copies of the game logs (`.log`) in the archive; fold each
/// into the stream of the same session and remove the copy.
fn migrate_whole_copies(archive: &Path, report: &mut ArchiveReport) {
    let Ok(copies) = find_files(archive, None, &["log"]) else { return };
    for path in copies {
        let result = (|| -> Result<(), String> {
            let data = fs::read(&path).map_err(|e| e.to_string())?;
            let log = split_game_log(&data)?;
            if let Some(stamp) = first_stamp(&log) {
                report.records += import_session(&path.with_extension("alog"), &log, stamp)?;
            }
            fs::remove_file(&path).map_err(|e| e.to_string())
        })();
        if let Err(e) = result {
            report.errors.push(format!("{}: {e}", path.display()));
        }
    }
}

/// The game rotates and overwrites its logs, so what it shows us is kept in `archive`
/// (laid out as `<character>/log/<name>_<first time>.alog`).  A session is identified by
/// its character, file name and first record time; only records the archive does not hold
/// yet are appended, so nothing the game later deletes or truncates is lost.  Originals are
/// only read.
#[cfg(test)]
pub fn archive_logs(source: &Path, archive: &Path, progress: impl Fn(usize, usize)) -> Result<ArchiveReport, String> {
    archive_logs_cached(source, archive, &mut SourceCache::default(), progress)
}

pub fn archive_logs_cached(source: &Path, archive: &Path, cache: &mut SourceCache, progress: impl Fn(usize, usize)) -> Result<ArchiveReport, String> {
    let mut report = ArchiveReport::default();
    migrate_whole_copies(archive, &mut report);
    let paths = find_files(source, Some(archive), &["log"])?;
    // Pointing the app at the archive itself needs nothing imported.
    if let (Ok(src), Ok(dst)) = (fs::canonicalize(source), fs::canonicalize(archive)) {
        if src.starts_with(&dst) {
            return Ok(report);
        }
    }
    for (i, path) in paths.iter().enumerate() {
        progress(i + 1, paths.len());
        let snapshot = || fs::metadata(path).and_then(|m| Ok((m.len(), m.modified()?)));
        let before = snapshot().ok();
        if before.is_some() && cache.0.get(path) == before.as_ref() {
            report.unchanged += 1;
            continue;
        }
        let import = || -> Result<usize, String> {
            let data = fs::read(path).map_err(|e| e.to_string())?;
            let log = split_game_log(&data)?;
            for (_, raw) in &log.records {
                if raw.len() < 10 || raw[8] != 0x1f || split_fields(&raw[9..]).is_err() {
                    return Err("ログの行が不完全です。次回もう一度読み込みます".into());
                }
            }
            if snapshot().ok() != before {
                return Err("ログの書き込み中です。次回もう一度読み込みます".into());
            }
            let Some(stamp) = first_stamp(&log) else { return Ok(0) };
            let character = character_of(path);
            let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let dest = archive.join(safe_name(&character)).join("log").join(format!("{}_{stamp:08x}.alog", safe_name(&stem)));
            import_session(&dest, &log, stamp)
        };
        match import() {
            Ok(n) => {
                if let Some(stamp) = before { cache.0.insert(path.clone(), stamp); }
                if n == 0 { report.unchanged += 1; } else { report.copied += 1; }
                report.records += n;
            }
            Err(e) => report.errors.push(format!("{}: {e}", path.display())),
        }
    }
    cache.0.retain(|path, _| paths.binary_search(path).is_ok());
    Ok(report)
}

/// The character (archive folder name) that most of the logs below `path` belong to.
pub fn main_character(path: &Path) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for log in find_logs(path).ok()? {
        *counts.entry(safe_name(&character_of(&log))).or_default() += 1;
    }
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(name, _)| name)
}

/// Deletes the archived logs of one character: the folder `<archive>/<id>` and nothing else.
/// The id must be a plain folder name that is a direct child of the archive.
pub fn delete_character(archive: &Path, id: &str) -> Result<(), String> {
    if id.is_empty() || safe_name(id) != id {
        return Err("キャラクターIDが不正です".into());
    }
    let dir = archive.join(id);
    let meta = fs::symlink_metadata(&dir).map_err(|_| "このキャラクターのログはアーカイブにありません".to_owned())?;
    if !meta.is_dir() {
        return Err("このキャラクターのログはアーカイブにありません".into());
    }
    let inside = match (fs::canonicalize(&dir), fs::canonicalize(archive)) {
        (Ok(d), Ok(a)) => d.parent() == Some(a.as_path()),
        _ => false,
    };
    if !inside {
        return Err("アーカイブの外は削除できません".into());
    }
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

// ---- filtering ---------------------------------------------------------------

#[derive(Default)]
pub struct Filter {
    pub needle: String,
    pub channels: Option<HashSet<u8>>,
    pub character: String,
    pub since: Option<i32>,
    pub until: Option<i32>,
}

pub fn select(entries: &[Entry], f: &Filter) -> Vec<u32> {
    let needle = f.needle.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            (needle.is_empty() || e.hay.contains(&needle))
                && f.channels.as_ref().is_none_or(|set| set.contains(&e.channel))
                && (f.character.is_empty() || *e.character == *f.character)
                && f.since.is_none_or(|d| e.day >= d)
                && f.until.is_none_or(|d| e.day <= d)
        })
        .map(|(i, _)| i as u32)
        .collect()
}

// ---- export ------------------------------------------------------------------

#[derive(Serialize)]
struct Row<'a> {
    timestamp: u32,
    datetime_jst: String,
    channel: u8,
    channel_name: String,
    code: String,
    sender: &'a str,
    message: &'a str,
    character: &'a str,
    source: &'a str,
    record: u32,
    offset: usize,
    raw_hex: String,
    warnings: &'a str,
}

fn row(e: &Entry) -> Row<'_> {
    Row {
        timestamp: e.timestamp,
        datetime_jst: iso_jst(e.timestamp),
        channel: e.channel,
        channel_name: name_of(e.channel),
        code: format!("{:08X}", e.code),
        sender: &e.sender,
        message: &e.message,
        character: &e.character,
        source: &e.source,
        record: e.record,
        offset: e.offset,
        raw_hex: e.raw.iter().map(|b| format!("{b:02x}")).collect(),
        warnings: &e.warnings,
    }
}

fn csv_cell(value: &str) -> String {
    // Prevent spreadsheet formula execution from user chat messages.
    let guarded = if value.trim_start().starts_with(['=', '+', '-', '@']) { format!("'{value}") } else { value.to_owned() };
    if guarded.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

/// Writes the selected entries.  Never overwrites an original binary log: only
/// `.csv` / `.json` targets are accepted.
pub fn export(path: &Path, entries: &[Entry], indices: &[u32]) -> Result<(), String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext != "csv" && ext != "json" {
        return Err("出力先の拡張子は .csv または .json にしてください".into());
    }
    let file = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    let io = |e: std::io::Error| e.to_string();
    if ext == "json" {
        out.write_all(b"[\n").map_err(io)?;
        for (n, &i) in indices.iter().enumerate() {
            if n > 0 {
                out.write_all(b",\n").map_err(io)?;
            }
            serde_json::to_writer(&mut out, &row(&entries[i as usize])).map_err(|e| e.to_string())?;
        }
        out.write_all(b"\n]\n").map_err(io)?;
    } else {
        out.write_all("\u{feff}".as_bytes()).map_err(io)?;
        out.write_all(
            b"timestamp,datetime_jst,channel,channel_name,code,sender,message,character,source,record,offset,raw_hex,warnings\r\n",
        )
        .map_err(io)?;
        for &i in indices {
            let r = row(&entries[i as usize]);
            let cells = [
                r.timestamp.to_string(),
                r.datetime_jst.clone(),
                r.channel.to_string(),
                r.channel_name.clone(),
                r.code.clone(),
                r.sender.to_owned(),
                r.message.to_owned(),
                r.character.to_owned(),
                r.source.to_owned(),
                r.record.to_string(),
                r.offset.to_string(),
                r.raw_hex.clone(),
                r.warnings.to_owned(),
            ];
            let line: Vec<String> = cells.iter().map(|c| csv_cell(c)).collect();
            out.write_all(line.join(",").as_bytes()).map_err(io)?;
            out.write_all(b"\r\n").map_err(io)?;
        }
    }
    out.flush().map_err(io)
}

// ---- character names ---------------------------------------------------------

const CHAT_KINDS: [u8; 14] = [10, 11, 14, 15, 30, 32, 16, 17, 18, 19, 20, 21, 22, 23];

pub fn valid_folder_id(id: &str) -> bool {
    id.strip_prefix("FFXIV_CHR")
        .is_some_and(|hex| (1..=32).contains(&hex.len()) && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The logs never record the player's own name, so this is a guess: the
/// same-world sender seen in the most files of a character, kept only when it
/// clearly beats the runner-up.
pub fn guess_names(entries: &[Entry]) -> HashMap<String, String> {
    let mut seen: HashMap<&str, HashMap<String, HashSet<&str>>> = HashMap::new();
    for e in entries {
        if !CHAT_KINDS.contains(&e.channel) {
            continue;
        }
        let name: String = e.sender.chars().filter(|c| !('\u{e000}'..='\u{f8ff}').contains(c)).collect();
        let name = name.trim();
        if !name.is_empty() {
            seen.entry(&e.character).or_default().entry(name.to_owned()).or_default().insert(&e.source);
        }
    }
    let mut out = HashMap::new();
    for (who, names) in seen {
        let mut ranked: Vec<(usize, &String)> = names.iter().map(|(n, f)| (f.len(), n)).collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        if ranked[0].0 >= 3 && (ranked.len() == 1 || ranked[0].0 >= ranked[1].0 * 2) {
            out.insert(who.to_owned(), ranked[0].1.clone());
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn make_file(dir: &Path, tail: &[u8]) -> PathBuf {
        let mut record = Vec::new();
        record.extend(1_700_000_000u32.to_le_bytes());
        record.extend(0x10Eu32.to_le_bytes());
        record.extend(b"\x1fAlice\x1f");
        record.extend("日本語: hello".as_bytes());
        let mut file = Vec::new();
        for n in [1000u32, 1001, record.len() as u32] {
            file.extend(n.to_le_bytes());
        }
        file.extend(&record);
        file.extend(tail);
        let folder = dir.join("FFXIV_CHR0001").join("log");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join("example.log");
        fs::write(&path, file).unwrap();
        path
    }

    pub(crate) fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aethelog-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn boundaries_and_stale_tail() {
        let dir = temp("tail");
        let (rows, report) = parse_file(&make_file(&dir, b"old record")).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message, "日本語: hello");
        assert_eq!(rows[0].sender, "Alice");
        assert_eq!(rows[0].channel, 14);
        assert_eq!(rows[0].record, 1000);
        assert_eq!(rows[0].offset, 12);
        assert_eq!(&*rows[0].character, "FFXIV_CHR0001");
        assert_eq!(iso_jst(rows[0].timestamp), "2023-11-15T07:13:20+09:00");
        assert_eq!(report.trailing_bytes_ignored, 10);
    }

    #[test]
    fn corrupt_offset_rejected() {
        let dir = temp("corrupt");
        let path = make_file(&dir, b"");
        let mut raw = fs::read(&path).unwrap();
        raw[8..12].copy_from_slice(&99_999u32.to_le_bytes());
        fs::write(&path, raw).unwrap();
        assert!(parse_file(&path).is_err());
    }

    #[test]
    fn split_fields_skips_separator_inside_link() {
        let sender = b"\x02\x27\x03\x1f\x01\x03Alice";
        let mut data = sender.to_vec();
        data.extend(b"\x1fhello");
        assert_eq!(split_fields(&data).unwrap(), (&sender[..], &b"hello"[..]));
    }

    #[test]
    fn filter_and_exports() {
        let dir = temp("export");
        let (mut rows, _) = parse_file(&make_file(&dir, b"")).unwrap();
        let day = rows[0].day;
        let f = Filter { needle: "HELLO".into(), since: Some(day), until: Some(day), ..Filter::default() };
        assert_eq!(select(&rows, &f), vec![0]);
        assert!(select(&rows, &Filter { since: Some(day + 1), ..Filter::default() }).is_empty());
        assert_eq!(parse_date("2023-11-15").unwrap(), Some(day));
        assert!(parse_date("2023-02-30").is_err());
        rows[0].message = "=1+1".into();
        let (csv, json) = (dir.join("out.csv"), dir.join("out.json"));
        export(&csv, &rows, &[0]).unwrap();
        export(&json, &rows, &[0]).unwrap();
        assert!(fs::read_to_string(&csv).unwrap().contains("'=1+1"));
        assert!(fs::read_to_string(&json).unwrap().contains("\"message\":\"=1+1\""));
        assert!(export(&dir.join("FFXIV_CHR0001").join("log").join("example.log"), &rows, &[0]).is_err());
    }

    #[test]
    fn folder_id_is_validated() {
        assert!(valid_folder_id("FFXIV_CHR0040000000000001"));
        assert!(!valid_folder_id("../x"));
        assert!(!valid_folder_id("FFXIV_CHR"));
    }

    /// Parity check against real logs: `AETHELOG_LOGS=<dir> cargo test real_logs -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_logs_summary() {
        let root = PathBuf::from(std::env::var("AETHELOG_LOGS").expect("AETHELOG_LOGS"));
        let started = std::time::Instant::now();
        let loaded = load_logs(&root, |_, _| {}).unwrap();
        let chars: usize = loaded.entries.iter().map(|e| e.message.chars().count() + e.sender.chars().count()).sum();
        let warn = loaded.entries.iter().filter(|e| !e.warnings.is_empty()).count();
        println!(
            "files={} entries={} errors={} chars={} warnings={} trailing={} first={} last={} {:?}",
            loaded.reports.len(),
            loaded.entries.len(),
            loaded.errors.len(),
            chars,
            warn,
            loaded.reports.iter().filter(|r| r.trailing_bytes_ignored > 0).count(),
            iso_jst(loaded.entries[0].timestamp),
            iso_jst(loaded.entries.last().unwrap().timestamp),
            started.elapsed()
        );
        for (k, v) in guess_names(&loaded.entries) {
            println!("{k} -> {v}");
        }
    }

    /// A game log with the given records (time `1_700_000_000 + n`, kind 14, sender "Alice").
    pub(crate) fn game_log(dir: &Path, name: &str, first: u32, messages: &[&str]) -> PathBuf {
        let mut bodies = Vec::new();
        for (n, text) in messages.iter().enumerate() {
            let mut record = Vec::new();
            record.extend((1_700_000_000u32 + first + n as u32).to_le_bytes());
            record.extend(0x10Eu32.to_le_bytes());
            record.extend(b"\x1fAlice\x1f");
            record.extend(text.as_bytes());
            bodies.push(record);
        }
        let mut file = Vec::new();
        file.extend(first.to_le_bytes());
        file.extend((first + messages.len() as u32).to_le_bytes());
        let mut end = 0u32;
        for body in &bodies {
            end += body.len() as u32;
            file.extend(end.to_le_bytes());
        }
        for body in &bodies {
            file.extend(body);
        }
        let folder = dir.join("FFXIV_CHR0001").join("log");
        fs::create_dir_all(&folder).unwrap();
        let path = folder.join(name);
        fs::write(&path, file).unwrap();
        path
    }

    fn messages(archive: &Path) -> Vec<String> {
        load_logs(archive, |_, _| {}).unwrap().entries.iter().map(|e| e.message.clone()).collect()
    }

    #[test]
    fn archive_imports_only_new_records() {
        let dir = temp("archive");
        let source = dir.join("game");
        let archive = dir.join("archive");
        let log = game_log(&source, "00000000.log", 0, &["one", "two"]);
        let report = archive_logs(&source, &archive, |_, _| {}).unwrap();
        assert_eq!((report.copied, report.unchanged, report.records), (1, 0, 2));
        // Same content again: nothing to do.
        let report = archive_logs(&source, &archive, |_, _| {}).unwrap();
        assert_eq!((report.copied, report.unchanged, report.records), (0, 1, 0));
        // The game appends a line: only that record is imported.
        game_log(&source, "00000000.log", 0, &["one", "two", "three"]);
        let report = archive_logs(&source, &archive, |_, _| {}).unwrap();
        assert_eq!((report.copied, report.records), (1, 1));
        assert_eq!(messages(&archive), ["one", "two", "three"]);
        // The game deletes the original; the archived records are still readable.
        fs::remove_file(&log).unwrap();
        assert_eq!(messages(&archive), ["one", "two", "three"]);
        assert_eq!(&*load_logs(&archive, |_, _| {}).unwrap().entries[0].character, "FFXIV_CHR0001");
        // A shorter version of the same session never removes or duplicates anything.
        game_log(&source, "00000000.log", 0, &["one"]);
        let report = archive_logs(&source, &archive, |_, _| {}).unwrap();
        assert_eq!(report.records, 0);
        assert_eq!(messages(&archive), ["one", "two", "three"]);
        // Viewing the archive itself imports nothing.
        assert_eq!(archive_logs(&archive, &archive, |_, _| {}).unwrap().records, 0);
        assert_eq!(messages(&archive).len(), 3);
    }

    #[test]
    fn repeated_live_copies_keep_one_file_and_retry_incomplete_writes() {
        let dir = temp("live-copy");
        let source = dir.join("game");
        let archive = dir.join("archive");
        let mut cache = SourceCache::default();
        for n in 1..=100 {
            let lines: Vec<String> = (0..n).map(|i| format!("line {i}")).collect();
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let path = game_log(&source, "00000000.log", 0, &refs);
            let original = fs::read(&path).unwrap();
            let report = archive_logs_cached(&source, &archive, &mut cache, |_, _| {}).unwrap();
            assert_eq!(report.records, 1);
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(archive_logs_cached(&source, &archive, &mut cache, |_, _| {}).unwrap().records, 0);
        }
        let path = game_log(&source, "00000000.log", 0, &["line 0"]);
        fs::write(&path, [0u8; 3]).unwrap();
        assert!(!archive_logs_cached(&source, &archive, &mut cache, |_, _| {}).unwrap().errors.is_empty());
        assert!(!cache.0.contains_key(&path) || cache.0[&path].0 != 3);
        let lines: Vec<String> = (0..101).map(|i| format!("line {i}")).collect();
        game_log(&source, "00000000.log", 0, &lines.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(archive_logs_cached(&source, &archive, &mut cache, |_, _| {}).unwrap().records, 1);
        assert_eq!(find_files(&archive, None, &["alog", "tmp", "log"]).unwrap().len(), 1);
        assert_eq!(messages(&archive).len(), 101);
    }

    #[test]
    fn archive_keeps_a_session_after_the_game_trims_its_front() {
        let dir = temp("trim");
        let source = dir.join("game");
        let archive = dir.join("archive");
        game_log(&source, "00000000.log", 0, &["a", "b", "c"]);
        archive_logs(&source, &archive, |_, _| {}).unwrap();
        // Records 0 and 1 are gone from the original; a new one is added.  The session's
        // first record time changed, so it is a new stream, but nothing is lost.
        game_log(&source, "00000000.log", 2, &["c", "d"]);
        archive_logs(&source, &archive, |_, _| {}).unwrap();
        let all = messages(&archive);
        assert!(all.contains(&"a".to_string()) && all.contains(&"d".to_string()));
    }

    #[test]
    fn whole_copies_from_older_versions_are_folded_in() {
        let dir = temp("legacy");
        let source = dir.join("game");
        let archive = dir.join("archive");
        let log = game_log(&source, "00000000.log", 0, &["one", "two"]);
        // An older archive held a whole copy named <name>_<first time>.log.
        let legacy = archive.join("FFXIV_CHR0001").join("log");
        fs::create_dir_all(&legacy).unwrap();
        fs::copy(&log, legacy.join("00000000_6553f100.log")).unwrap();
        game_log(&source, "00000000.log", 0, &["one", "two", "three"]);
        let report = archive_logs(&source, &archive, |_, _| {}).unwrap();
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(!legacy.join("00000000_6553f100.log").exists());
        assert_eq!(messages(&archive), ["one", "two", "three"]);
    }

    #[test]
    fn deleting_a_character_removes_only_its_archive_folder() {
        let dir = temp("delete");
        let source = dir.join("game");
        let archive = dir.join("archive");
        game_log(&source, "00000000.log", 0, &["one"]);
        archive_logs(&source, &archive, |_, _| {}).unwrap();
        fs::create_dir_all(archive.join("FFXIV_CHR0002").join("log")).unwrap();
        for bad in ["", "..", "../game", "FFXIV_CHR0001/log", "a b"] {
            assert!(delete_character(&archive, bad).is_err(), "{bad:?} must be refused");
        }
        assert!(source.exists());
        delete_character(&archive, "FFXIV_CHR0001").unwrap();
        assert!(!archive.join("FFXIV_CHR0001").exists());
        assert!(archive.join("FFXIV_CHR0002").exists());
        assert!(source.join("FFXIV_CHR0001").exists(), "the game's own logs are never touched");
        assert!(delete_character(&archive, "FFXIV_CHR0001").is_err());
    }

    #[test]
    fn main_character_is_the_one_with_most_logs() {
        let dir = temp("focus");
        game_log(&dir, "00000000.log", 0, &["one"]);
        assert_eq!(main_character(&dir).as_deref(), Some("FFXIV_CHR0001"));
        assert_eq!(main_character(&dir.join("nothing")), None);
    }

    #[test]
    fn archive_below_the_scanned_folder_is_not_imported_into_itself() {
        let dir = temp("nested");
        let game = dir.join("game");
        game_log(&game, "00000000.log", 0, &["one"]);
        let archive = game.join("tool").join("userdata").join("archive");
        for _ in 0..3 {
            archive_logs(&game, &archive, |_, _| {}).unwrap();
        }
        let files = find_files(&archive, None, &["alog", "log"]).unwrap().len();
        assert_eq!(files, 1, "repeated loads must not add copies of the archive");
        assert_eq!(messages(&archive), ["one"]);
    }
}
