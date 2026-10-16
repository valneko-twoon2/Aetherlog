//! Aetherlog backend: loads FFXIV logs read-only and answers the Vue front end
//! through Tauri commands.  The only network use is the one-time download of the lookup
//! tables (`dict.rs`) and the fixed Lodestone address handed to the system browser.

mod channels;
mod dict;
mod lodestone;
mod logfile;
mod sestring;

use logfile::{Entry, Filter};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{Manager, State};

/// Upper bound for one page; the UI's "all" option sends this value.
const MAX_PAGE: i64 = 1_000_000_000;

/// A panic elsewhere must not poison the state for good; keep serving what is there.
fn lock(m: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Turns a panic inside a command into an error message instead of a dead window.
fn guard<T>(job: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(job)).unwrap_or_else(|_| Err("内部エラーが発生しました".to_owned()))
}

#[derive(Default)]
struct Inner {
    status: &'static str,
    error: String,
    path: PathBuf,
    entries: Arc<Vec<Entry>>,
    errors: Vec<String>,
    files: usize,
    trailing_files: usize,
    guesses: HashMap<String, String>,
    generation: usize,
    /// "copy" while logs are being saved to the archive, "read" while they are parsed.
    phase: &'static str,
    /// Character to show after a log was picked or dropped by hand (empty: keep the current one).
    focus: String,
    cache: Option<(CacheKey, Arc<Vec<u32>>)>,
}

pub struct AppState {
    inner: Mutex<Inner>,
    progress: [AtomicUsize; 2],
    /// Everything the app keeps (archive, names, last location), next to the executable.
    data_dir: PathBuf,
    /// Local copy of every log seen so far (the game rotates and overwrites its own).
    archive_dir: PathBuf,
}

#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct QueryParams {
    query: String,
    channels: Option<Vec<u8>>,
    character: String,
    since: String,
    until: String,
    newest_first: bool,
    offset: i64,
    limit: i64,
}

#[derive(PartialEq)]
struct CacheKey {
    query: String,
    channels: Option<Vec<u8>>,
    character: String,
    since: String,
    until: String,
    newest_first: bool,
    generation: usize,
}

impl AppState {
    fn names_file(&self) -> PathBuf {
        self.data_dir.join("character_names.json")
    }

    fn load_names(&self) -> HashMap<String, String> {
        let Ok(text) = std::fs::read_to_string(self.names_file()) else { return HashMap::new() };
        let parsed: HashMap<String, String> = serde_json::from_str(&text).unwrap_or_default();
        parsed.into_iter().filter(|(k, _)| logfile::valid_folder_id(k)).collect()
    }

    /// `copy`: import new records from `path` into the archive first. `focus`: show the character of `path` afterwards.
    fn start_load(self: &Arc<Self>, path: PathBuf, copy: bool, focus: bool) -> bool {
        {
            let mut inner = lock(&self.inner);
            if inner.status == "loading" {
                return false;
            }
            inner.status = "loading";
            inner.error.clear();
            inner.path = path.clone();
            inner.focus = if focus { logfile::main_character(&path).unwrap_or_default() } else { String::new() };
        }
        self.progress[0].store(0, Ordering::Relaxed);
        self.progress[1].store(0, Ordering::Relaxed);
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let job = AssertUnwindSafe(|| {
            let store = |done: usize, total: usize| {
                state.progress[0].store(done, Ordering::Relaxed);
                state.progress[1].store(total, Ordering::Relaxed);
            };
            // 1. Save a local copy of everything the game still has.
            lock(&state.inner).phase = "copy";
            let mut notes = Vec::new();
            if copy {
                match logfile::archive_logs(&path, &state.archive_dir, &store) {
                    Ok(report) => notes.extend(report.errors),
                    Err(e) => notes.push(format!("{e}（保存済みのコピーを表示します）")),
                }
            }
            // 2. Read from the local copy, which also holds logs the game has since deleted.
            lock(&state.inner).phase = "read";
            store(0, 0);
            let result = logfile::load_logs(&state.archive_dir, &store).and_then(|loaded| {
                // Without a copy step (after a deletion) an empty archive is a valid, empty result.
                if copy && loaded.reports.is_empty() && loaded.errors.is_empty() {
                    Err(".log ファイルが見つかりません".to_owned())
                } else {
                    Ok(loaded)
                }
            });
            let guesses = result.as_ref().map(|l| logfile::guess_names(&l.entries)).unwrap_or_default();
            let mut inner = lock(&state.inner);
            match result {
                Ok(loaded) => {
                    inner.trailing_files = loaded.reports.iter().filter(|r| r.trailing_bytes_ignored > 0).count();
                    inner.files = loaded.reports.len();
                    inner.entries = Arc::new(loaded.entries);
                    notes.extend(loaded.errors);
                    inner.errors = notes;
                    inner.guesses = guesses;
                    inner.generation += 1;
                    inner.cache = None;
                    inner.status = "ready";
                }
                Err(e) => {
                    inner.status = "error";
                    inner.error = if notes.is_empty() { e } else { format!("{e}
{}", notes.join("
")) };
                }
            }
            });
            if catch_unwind(job).is_err() {
                let mut inner = lock(&state.inner);
                inner.status = "error";
                inner.error = "読み込み中に内部エラーが発生しました".to_owned();
            }
        });
        if std::env::var_os("AETHERLOG_LOGS").is_none() {
            let _ = std::fs::create_dir_all(&self.data_dir)
                .and_then(|()| std::fs::write(self.data_dir.join("last_path.txt"), self.path_text()));
        }
        true
    }

    fn path_text(&self) -> String {
        lock(&self.inner).path.to_string_lossy().into_owned()
    }

    /// Entries plus the matching row indices for a query (cached by filter).
    fn select(&self, p: &QueryParams) -> Result<(Arc<Vec<Entry>>, Arc<Vec<u32>>), String> {
        let since = logfile::parse_date(&p.since)?;
        let until = logfile::parse_date(&p.until)?;
        let mut channels = p.channels.clone();
        if let Some(c) = channels.as_mut() {
            c.sort_unstable();
            c.dedup();
        }
        let mut inner = lock(&self.inner);
        let key = CacheKey {
            query: p.query.clone(),
            channels,
            character: p.character.clone(),
            since: p.since.clone(),
            until: p.until.clone(),
            newest_first: p.newest_first,
            generation: inner.generation,
        };
        let entries = Arc::clone(&inner.entries);
        if let Some((k, rows)) = &inner.cache {
            if *k == key {
                return Ok((entries, Arc::clone(rows)));
            }
        }
        let filter = Filter {
            needle: p.query.clone(),
            channels: key.channels.as_ref().map(|c| c.iter().copied().collect::<HashSet<u8>>()),
            character: p.character.clone(),
            since,
            until,
        };
        let mut rows = logfile::select(&entries, &filter);
        if p.newest_first {
            rows.reverse();
        }
        let rows = Arc::new(rows);
        inner.cache = Some((key, Arc::clone(&rows)));
        Ok((entries, rows))
    }
}

#[derive(Serialize)]
struct StateView {
    status: &'static str,
    progress: [usize; 2],
    path: String,
    error: String,
    total: usize,
    phase: &'static str,
    focus: String,
    /// Lookup tables: "ready", "downloading" or "error" (see `dict_error`).
    dict: &'static str,
    dict_error: String,
}

#[derive(Serialize)]
struct Group {
    id: &'static str,
    name: &'static str,
}

#[derive(Serialize)]
struct Person {
    id: String,
    name: String,
    guess: bool,
    count: usize,
    /// Channel counts for this character only.
    counts: BTreeMap<String, usize>,
    /// Days (JST) on which this character has lines, oldest first.
    days: Vec<DayCount>,
}

#[derive(Serialize)]
struct Meta {
    channels: Vec<channels::Channel>,
    groups: Vec<Group>,
    counts: BTreeMap<String, usize>,
    characters: Vec<Person>,
    range: Vec<String>,
    total: usize,
    files: usize,
    trailing_files: usize,
    warning_records: usize,
    errors: Vec<String>,
}

#[derive(Serialize)]
struct DayCount {
    date: String,
    count: usize,
}

#[derive(Serialize)]
struct RowView {
    i: u32,
    t: u32,
    k: u8,
    who: Vec<sestring::Span>,
    msg: Vec<sestring::Span>,
    ch: String,
    w: bool,
    /// The day (JST) differs from the previous row, so the list shows a banner above it.
    d: bool,
}

#[derive(Serialize)]
struct Page {
    total: usize,
    offset: usize,
    limit: usize,
    rows: Vec<RowView>,
}

fn render(index: u32, e: &Entry, new_day: bool) -> RowView {
    let body = &e.raw[9..];
    let (who, text) = logfile::split_fields(body).unwrap_or((&[], body));
    RowView {
        i: index,
        t: e.timestamp,
        k: e.channel,
        who: sestring::spans(who),
        msg: sestring::spans(text),
        ch: e.character.to_string(),
        w: !e.warnings.is_empty(),
        d: new_day,
    }
}

/// Licence texts of the bundled open-source packages (`tools/make_notices.ps1`).
#[tauri::command]
fn notices() -> &'static str {
    include_str!("../../THIRD-PARTY-NOTICES.txt")
}

#[tauri::command]
async fn state(app: State<'_, Arc<AppState>>) -> Result<StateView, String> {
    let inner = lock(&app.inner);
    let (dict, dict_error) = dict::status();
    Ok(StateView {
        status: if inner.status.is_empty() { "idle" } else { inner.status },
        progress: [app.progress[0].load(Ordering::Relaxed), app.progress[1].load(Ordering::Relaxed)],
        path: inner.path.to_string_lossy().into_owned(),
        error: inner.error.clone(),
        total: inner.entries.len(),
        phase: inner.phase,
        focus: inner.focus.clone(),
        dict,
        dict_error,
    })
}

#[tauri::command]
async fn meta(app: State<'_, Arc<AppState>>) -> Result<Meta, String> {
    guard(|| meta_impl(&app))
}

fn meta_impl(app: &AppState) -> Result<Meta, String> {
    let manual = app.load_names();
    let inner = lock(&app.inner);
    let entries = &inner.entries;
    let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
    let mut people: BTreeMap<&str, BTreeMap<i32, usize>> = BTreeMap::new();
    let mut character_counts: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    for e in entries.iter() {
        *counts.entry(e.channel).or_default() += 1;
        *character_counts.entry(&e.character).or_default().entry(e.channel.to_string()).or_default() += 1;
        *people.entry(&e.character).or_default().entry(e.day).or_default() += 1;
    }
    let known: HashSet<u8> = channels::catalog().iter().map(|c| c.id).collect();
    let extra: Vec<channels::Channel> = counts.keys().filter(|k| !known.contains(k)).map(|&k| channels::fallback(k)).collect();
    let mut groups: Vec<Group> = channels::GROUPS.iter().map(|&(id, name)| Group { id, name }).collect();
    if !extra.is_empty() {
        groups.push(Group { id: "other", name: "その他" });
    }
    let mut all = channels::catalog().to_vec();
    all.extend(extra);
    let characters = people
        .into_iter()
        .map(|(id, per_day)| {
            let name = manual.get(id).or_else(|| inner.guesses.get(id)).cloned().unwrap_or_default();
            let count = per_day.values().sum();
            let days = per_day.into_iter().map(|(day, count)| DayCount { date: logfile::date_string(day), count }).collect();
            Person { id: id.to_owned(), name, guess: !manual.contains_key(id) && inner.guesses.contains_key(id), count, counts: character_counts.remove(id).unwrap_or_default(), days }
        })
        .collect();
    Ok(Meta {
        channels: all,
        groups,
        counts: counts.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        characters,
        range: match (entries.first(), entries.last()) {
            (Some(a), Some(b)) => vec![logfile::date_string(a.day), logfile::date_string(b.day)],
            _ => vec![],
        },
        total: entries.len(),
        files: inner.files,
        trailing_files: inner.trailing_files,
        warning_records: entries.iter().filter(|e| !e.warnings.is_empty()).count(),
        errors: inner.errors.clone(),
    })
}

#[tauri::command]
async fn load_logs(app: State<'_, Arc<AppState>>, path: String, focus: Option<bool>) -> Result<serde_json::Value, String> {
    let target = PathBuf::from(path.trim());
    if !target.exists() {
        return Err(format!("パスが見つかりません: {}", target.display()));
    }
    if logfile::find_logs(&target)?.is_empty() {
        return Err(".log ファイルが見つかりません".into());
    }
    Ok(serde_json::json!({ "started": app.inner().start_load(target, true, focus.unwrap_or(false)) }))
}

/// Deletes the archived logs of one character and shows what is left. The game's own logs are
/// not touched, so a later reload imports them again if the game still has them.
#[tauri::command]
async fn delete_character(app: State<'_, Arc<AppState>>, id: String) -> Result<serde_json::Value, String> {
    if lock(&app.inner).status == "loading" {
        return Err("読み込み中は削除できません".into());
    }
    logfile::delete_character(&app.archive_dir, &id)?;
    let mut names = app.load_names();
    if names.remove(&id).is_some() {
        let text = serde_json::to_string_pretty(&names).map_err(|e| e.to_string())?;
        std::fs::write(app.names_file(), text).map_err(|e| e.to_string())?;
    }
    let path = lock(&app.inner).path.clone();
    Ok(serde_json::json!({ "started": app.inner().start_load(path, false, false) }))
}

#[tauri::command]
async fn query(app: State<'_, Arc<AppState>>, params: QueryParams) -> Result<Page, String> {
    guard(|| query_impl(&app, &params))
}

fn query_impl(app: &AppState, params: &QueryParams) -> Result<Page, String> {
    let (entries, rows) = app.select(params)?;
    let limit = if params.limit <= 0 { 200 } else { params.limit.min(MAX_PAGE) } as usize;
    let last_page = rows.len().saturating_sub(1) / limit * limit;
    let offset = if params.offset < 0 { last_page } else { (params.offset as usize).min(rows.len().saturating_sub(1)) };
    let offset = if rows.is_empty() { 0 } else { offset };
    let day = |i: u32| (i64::from(entries[i as usize].timestamp) + 9 * 3600).div_euclid(86400);
    let page = (offset..(offset + limit).min(rows.len()))
        .map(|p| render(rows[p], &entries[rows[p] as usize], p == 0 || day(rows[p - 1]) != day(rows[p])))
        .collect();
    Ok(Page { total: rows.len(), offset, limit, rows: page })
}

#[tauri::command]
async fn export_entries(app: State<'_, Arc<AppState>>, params: QueryParams, path: String) -> Result<(), String> {
    guard(|| {
        // Only plain data files are ever written, whatever the front end asks for.
        let ext = Path::new(&path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("csv" | "json")) {
            return Err("保存できるのは .csv と .json だけです".to_owned());
        }
        let (entries, rows) = app.select(&params)?;
        logfile::export(Path::new(&path), &entries, &rows)
    })
}

#[tauri::command]
async fn set_name(app: State<'_, Arc<AppState>>, id: String, name: String) -> Result<(), String> {
    if !logfile::valid_folder_id(&id) {
        return Err("キャラクターIDが不正です".into());
    }
    let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(40).collect();
    let mut names = app.load_names();
    if name.is_empty() {
        names.remove(&id);
    } else {
        names.insert(id, name);
    }
    std::fs::create_dir_all(&app.data_dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&names).map_err(|e| e.to_string())?;
    std::fs::write(app.names_file(), text).map_err(|e| e.to_string())
}

/// Opens the player's Lodestone search page in the system browser.  The address is built
/// here from encoded parts; nothing from the page is ever run as a command.
#[tauri::command]
async fn open_lodestone(name: String, world: String) -> Result<(), String> {
    guard(|| {
        let name = name.trim();
        let world = world.trim();
        let clean = |t: &str, max: usize| t.chars().count() <= max && !t.chars().any(char::is_control);
        if name.is_empty() || !clean(name, 40) || !clean(world, 20) || !world.chars().all(char::is_alphanumeric) {
            return Err("キャラクター名が不正です".to_owned());
        }
        let url = lodestone::search_url(name, world);
        std::process::Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", &url]).spawn().map(drop).map_err(|e| e.to_string())
    })
}

/// The folder that holds the executable; all app data lives beside it.
fn exe_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for item in std::fs::read_dir(from)? {
        let item = item?;
        let target = to.join(item.file_name());
        if item.file_type()?.is_dir() {
            copy_dir(&item.path(), &target)?;
        } else {
            std::fs::copy(item.path(), target)?;
        }
    }
    Ok(())
}

/// Earlier versions kept their data in the per-user app folders; bring it along once.
fn migrate_legacy(app: &tauri::App, data_dir: &Path) {
    let _ = std::fs::create_dir_all(data_dir);
    if let Ok(old) = app.path().app_config_dir() {
        for name in ["character_names.json", "last_path.txt"] {
            if old.join(name).is_file() && !data_dir.join(name).exists() {
                let _ = std::fs::copy(old.join(name), data_dir.join(name));
            }
        }
    }
    if let Ok(old) = app.path().app_local_data_dir().map(|d| d.join("archive")) {
        let new = data_dir.join("archive");
        if old.is_dir() && !new.exists() && std::fs::rename(&old, &new).is_err() {
            // Different drive: copy instead and leave the old folder in place.
            let _ = copy_dir(&old, &new);
        }
    }
}

/// Loads the saved lookup tables, or downloads them in the background when they are missing.
/// Rows are rendered from the live tables, but search text and exports are decoded when the
/// logs are read, so the logs are read once more after a download.
fn start_dictionary(state: &Arc<AppState>) {
    let dir = state.data_dir.join("dict");
    if let Some(saved) = dict::load(&dir) {
        dict::install(saved);
        dict::set_status("ready", "");
        return;
    }
    dict::set_status("downloading", "");
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        match catch_unwind(|| dict::download(&dir)).unwrap_or_else(|_| Err("内部エラーが発生しました".to_owned())) {
            Ok(fresh) => {
                dict::install(fresh);
                // Wait for a load in progress, then read the archive again with the new tables.
                loop {
                    let (busy, path, ready) = {
                        let inner = lock(&state.inner);
                        (inner.status == "loading", inner.path.clone(), inner.status == "ready")
                    };
                    if !busy {
                        if ready {
                            state.start_load(path, false, false);
                        }
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                // Only now: the UI keeps polling while "downloading", so it sees the reload start.
                dict::set_status("ready", "");
            }
            Err(e) => dict::set_status("error", e),
        }
    });
}

fn default_log_root(app: &tauri::App, data_dir: &Path) -> PathBuf {
    // An explicit override, handy for demos and development.
    if let Some(root) = std::env::var_os("AETHERLOG_LOGS").map(PathBuf::from).filter(|p| p.exists()) {
        return root;
    }
    if let Ok(saved) = std::fs::read_to_string(data_dir.join("last_path.txt")) {
        let saved = PathBuf::from(saved.trim());
        if saved.exists() {
            return saved;
        }
    }
    app.path()
        .document_dir()
        .map(|d| d.join("My Games").join("FINAL FANTASY XIV - A Realm Reborn"))
        .unwrap_or_default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let overridden = std::env::var_os("AETHERLOG_DATA");
            let data_dir = overridden.clone().map(PathBuf::from).unwrap_or_else(|| exe_dir().join("userdata"));
            if overridden.is_none() {
                migrate_legacy(app, &data_dir);
            }
            let root = default_log_root(app, &data_dir);
            let shared = Arc::new(AppState {
                inner: Mutex::new(Inner { path: root.clone(), ..Inner::default() }),
                progress: [AtomicUsize::new(0), AtomicUsize::new(0)],
                archive_dir: data_dir.join("archive"),
                data_dir,
            });
            if root.exists() {
                shared.start_load(root, true, false);
            }
            start_dictionary(&shared);
            app.manage(shared);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![notices, state, meta, load_logs, delete_character, query, export_entries, set_name, open_lodestone])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| eprintln!("Aetherlog stopped: {e}"));
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[test]
    fn channel_counts_are_scoped_to_each_character() {
        let entries = [("first", 10), ("first", 10), ("second", 10), ("second", 60)]
            .into_iter()
            .map(|(character, channel)| Entry {
                timestamp: 0, day: 0, channel, code: channel as u32,
                sender: String::new(), message: String::new(), hay: String::new(),
                character: Arc::from(character), source: Arc::from("fixture"),
                record: 0, offset: 0, raw: vec![], warnings: String::new(),
            })
            .collect();
        let app = AppState {
            inner: Mutex::new(Inner { entries: Arc::new(entries), ..Inner::default() }),
            progress: [AtomicUsize::new(0), AtomicUsize::new(0)],
            data_dir: std::env::temp_dir().join("aetherlog-metadata-test-missing"),
            archive_dir: PathBuf::new(),
        };
        let meta = meta_impl(&app).unwrap();
        assert_eq!(meta.counts["10"], 3);
        assert_eq!(meta.counts["60"], 1);
        let first = meta.characters.iter().find(|c| c.id == "first").unwrap();
        let second = meta.characters.iter().find(|c| c.id == "second").unwrap();
        assert_eq!(first.counts["10"], 2);
        assert!(!first.counts.contains_key("60"));
        assert_eq!(second.counts["10"], 1);
        assert_eq!(second.counts["60"], 1);
        for person in &meta.characters {
            assert_eq!(person.counts.values().sum::<usize>(), person.count);
        }
    }
}
