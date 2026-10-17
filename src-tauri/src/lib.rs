//! Aethelog backend: loads FFXIV logs read-only and answers the Vue front end
//! through Tauri commands.  The only network use is the one-time download of the lookup
//! tables (`dict.rs`) and the fixed Lodestone address handed to the system browser.

mod channels;
mod dict;
mod lodestone;
mod logfile;
mod preferences;
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
    monitoring: bool,
    refreshing: bool,
    preferences: preferences::Preferences,
}

pub struct AppState {
    inner: Mutex<Inner>,
    progress: [AtomicUsize; 2],
    /// Everything the app keeps (archive, names, last location), next to the executable.
    data_dir: PathBuf,
    /// Local copy of every log seen so far (the game rotates and overwrites its own).
    archive_dir: PathBuf,
    source_cache: Mutex<logfile::SourceCache>,
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
        self.start_load_mode(path, copy, focus, false)
    }

    fn start_load_mode(self: &Arc<Self>, path: PathBuf, copy: bool, focus: bool, background: bool) -> bool {
        {
            let mut inner = lock(&self.inner);
            if inner.status == "loading" || inner.refreshing || (background && (!inner.monitoring || inner.path != path)) {
                return false;
            }
            inner.refreshing = background;
            if !background { inner.status = "loading"; }
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
                let mut cache = state.source_cache.lock().unwrap_or_else(|e| e.into_inner());
                if !background { *cache = logfile::SourceCache::default(); }
                match logfile::archive_logs_cached(&path, &state.archive_dir, &mut cache, &store) {
                    Ok(report) => {
                        if background && report.records == 0 {
                            let mut inner = lock(&state.inner);
                            inner.error = report.errors.join("\n");
                            inner.refreshing = false;
                            return;
                        }
                        notes.extend(report.errors);
                    }
                    Err(e) if background => {
                        let mut inner = lock(&state.inner);
                        inner.error = e;
                        inner.refreshing = false;
                        return;
                    }
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
            inner.refreshing = false;
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
                inner.refreshing = false;
                if !background { inner.status = "error"; }
                inner.error = "読み込み中に内部エラーが発生しました".to_owned();
            }
        });
        if !background && std::env::var_os("AETHELOG_LOGS").is_none() {
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
    generation: usize,
    refreshing: bool,
    preferences: preferences::Preferences,
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

const NOTICES: &str = include_str!("../notices.json");

/// The bundled open-source packages with their licence texts, as JSON (`tools/make_notices.ps1`).
#[tauri::command]
fn notices() -> &'static str {
    NOTICES
}

/// Opens the homepage of package number `index` of the list above in the system browser.
/// Only the https address stored in the list is used, never one sent by the page.
#[tauri::command]
async fn open_homepage(index: usize) -> Result<(), String> {
    guard(|| {
        let list: serde_json::Value = serde_json::from_str(NOTICES).map_err(|e| e.to_string())?;
        let url = list["packages"][index]["h"].as_str().filter(|u| u.starts_with("https://")).ok_or("ホームページがありません")?;
        std::process::Command::new("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).spawn().map(drop).map_err(|e| e.to_string())
    })
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
        generation: inner.generation,
        refreshing: inner.refreshing,
        preferences: inner.preferences,
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
    let started = app.inner().start_load(target, true, focus.unwrap_or(false));
    if started {
        // A manual reload also resumes the automatic import that deleting a character paused.
        lock(&app.inner).monitoring = true;
    }
    Ok(serde_json::json!({ "started": started }))
}

#[tauri::command]
async fn set_preferences(app: State<'_, Arc<AppState>>, preferences: preferences::Preferences) -> Result<preferences::Preferences, String> {
    let mut inner = lock(&app.inner);
    preferences.save(&app.data_dir)?;
    inner.preferences = preferences;
    Ok(preferences)
}

/// Deletes the archived logs of one character and shows what is left. The game's own logs are
/// not touched, so a later reload imports them again if the game still has them.
#[tauri::command]
async fn delete_character(app: State<'_, Arc<AppState>>, id: String) -> Result<serde_json::Value, String> {
    {
        let mut inner = lock(&app.inner);
        if inner.status == "loading" || inner.refreshing {
            return Err("読み込み中は削除できません".into());
        }
        // Pause automatic imports so deletion is not immediately undone by monitoring.
        inner.monitoring = false;
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
    // Keep the former identifier only as an input for migration after the rename.
    let config_dirs = app.path().app_config_dir().ok().into_iter()
        .chain(std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("dev.aetherlog.app")));
    for old in config_dirs {
        for name in ["character_names.json", "last_path.txt"] {
            if old.join(name).is_file() && !data_dir.join(name).exists() {
                let _ = std::fs::copy(old.join(name), data_dir.join(name));
            }
        }
    }
    let archive_dirs = app.path().app_local_data_dir().ok().map(|d| d.join("archive")).into_iter()
        .chain(std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("dev.aetherlog.app").join("archive")));
    for old in archive_dirs {
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
                        (inner.status == "loading" || inner.refreshing, inner.path.clone(), inner.status == "ready")
                    };
                    if !busy && (!ready || state.start_load(path, false, false)) {
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

/// Imports again when the game writes a `.log`.  The game writes a log only when it holds 1,000
/// records or at logout, so a file-system watch replaces polling.  A slow rescan covers missed
/// notifications (synced or network folders) and locations that cannot be watched.  One worker
/// does both; start_load_mode serializes it with all other archive work.
fn start_monitor(state: &Arc<AppState>) {
    use notify::Watcher;
    use std::time::{Duration, Instant};
    // The game writes a file in several calls; import once they have stopped.
    const SETTLE: Duration = Duration::from_secs(1);
    const RESCAN: Duration = Duration::from_secs(60);
    let weak = Arc::downgrade(state);
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
        let is_log = |p: &Path| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("log"));
        // The location being watched, its watcher unless setting it up failed, and when it was set up.
        let mut watched: Option<(PathBuf, Option<notify::RecommendedWatcher>, Instant)> = None;
        let mut due: Option<Instant> = None;
        let mut last_scan = Instant::now();
        let mut was_monitoring = true;
        loop {
            let mut changed = false;
            if let Ok(first) = rx.recv_timeout(Duration::from_millis(200)) {
                for event in std::iter::once(first).chain(rx.try_iter()) {
                    // An error (such as an overflowing event buffer) may hide a change.
                    changed |= event.map_or(true, |e| e.paths.iter().any(|p| is_log(p)));
                }
            }
            let Some(state) = weak.upgrade() else { break };
            let (path, monitoring) = {
                let inner = lock(&state.inner);
                (inner.path.clone(), inner.monitoring)
            };
            let rescan = last_scan.elapsed() >= RESCAN;
            let stale = watched.as_ref().map_or(true, |(p, w, at)| *p != path || (w.is_none() && at.elapsed() >= RESCAN));
            if stale {
                let watcher = notify::recommended_watcher(tx.clone()).ok().and_then(|mut w| {
                    w.watch(&path, notify::RecursiveMode::Recursive).ok().map(|()| w)
                });
                watched = Some((path.clone(), watcher, Instant::now()));
            }
            if changed {
                due = Some(Instant::now() + SETTLE);
            }
            if monitoring && !was_monitoring {
                due = Some(Instant::now());
            }
            was_monitoring = monitoring;
            let ready = due.is_some_and(|t| Instant::now() >= t);
            if monitoring && (ready || rescan) && state.start_load_mode(path, true, false, true) {
                due = None;
                last_scan = Instant::now();
            }
        }
    });
}

/// The window icon the toolkit creates carries a broken transparency mask, which the Task
/// Manager (and any other shell view that draws with the mask) shows as stripes.  Replace it with
/// icons taken from the exe's own resources, which are built with correct masks.
#[cfg(windows)]
fn fix_window_icon(window: &tauri::WebviewWindow) {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "user32")]
    extern "system" {
        fn PrivateExtractIconsW(file: *const u16, index: i32, cx: i32, cy: i32, icons: *mut isize, ids: *mut u32, count: u32, flags: u32) -> u32;
        fn SendMessageW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
        fn GetDpiForWindow(hwnd: isize) -> u32;
    }
    const WM_SETICON: u32 = 0x0080;
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as isize;
    let Ok(exe) = std::env::current_exe() else { return };
    let file: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as i32;
    // (ICON_SMALL, 16 px) and (ICON_BIG, 32 px) at the window's scale.
    for (kind, base) in [(0usize, 16), (1usize, 32)] {
        let size = base * dpi / 96;
        let (mut icon, mut id) = (0isize, 0u32);
        let found = unsafe { PrivateExtractIconsW(file.as_ptr(), 0, size, size, &mut icon, &mut id, 1, 0) };
        if found > 0 && icon != 0 {
            // The icon lives as long as the window; it is intentionally never destroyed.
            unsafe { SendMessageW(hwnd, WM_SETICON, kind, icon) };
        }
    }
}

fn default_log_root(app: &tauri::App, data_dir: &Path) -> PathBuf {
    // An explicit override, handy for demos and development.
    if let Some(root) = std::env::var_os("AETHELOG_LOGS").map(PathBuf::from).filter(|p| p.exists()) {
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
            let overridden = std::env::var_os("AETHELOG_DATA");
            let data_dir = overridden.clone().map(PathBuf::from).unwrap_or_else(|| exe_dir().join("userdata"));
            if overridden.is_none() {
                migrate_legacy(app, &data_dir);
            }
            let root = default_log_root(app, &data_dir);
            let preferences = preferences::Preferences::load(&data_dir);
            let shared = Arc::new(AppState {
                inner: Mutex::new(Inner { path: root.clone(), monitoring: true, preferences, ..Inner::default() }),
                progress: [AtomicUsize::new(0), AtomicUsize::new(0)],
                archive_dir: data_dir.join("archive"),
                data_dir,
                source_cache: Mutex::new(logfile::SourceCache::default()),
            });
            if root.exists() {
                shared.start_load(root, true, false);
            }
            start_dictionary(&shared);
            start_monitor(&shared);
            app.manage(shared);
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                fix_window_icon(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![notices, open_homepage, state, meta, load_logs, set_preferences, delete_character, query, export_entries, set_name, open_lodestone])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| eprintln!("Aethelog stopped: {e}"));
}

#[cfg(test)]
mod notices_tests {
    #[test]
    fn notices_list_is_well_formed() {
        let list: serde_json::Value = serde_json::from_str(super::NOTICES).unwrap();
        let texts = list["texts"].as_array().unwrap().len();
        let packages = list["packages"].as_array().unwrap();
        assert!(packages.len() > 100);
        for p in packages {
            assert!(p["h"].as_str().unwrap().starts_with("https://"), "{p}");
            assert!(p["t"].as_array().unwrap().iter().all(|i| (i.as_u64().unwrap() as usize) < texts));
        }
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    fn wait_for_load(app: &AppState) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let inner = lock(&app.inner);
            if inner.status != "loading" && !inner.refreshing { break; }
            drop(inner);
            assert!(std::time::Instant::now() < deadline, "load timed out");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn live_refresh_publishes_only_new_records_and_respects_pause() {
        let dir = logfile::tests::temp("live-state");
        let source = dir.join("game");
        logfile::tests::game_log(&source, "00000000.log", 0, &["first"]);
        let app = Arc::new(AppState {
            inner: Mutex::new(Inner { monitoring: true, ..Inner::default() }),
            progress: [AtomicUsize::new(0), AtomicUsize::new(0)],
            data_dir: dir.clone(), archive_dir: dir.join("archive"),
            source_cache: Mutex::new(logfile::SourceCache::default()),
        });
        assert!(app.start_load(source.clone(), true, false));
        wait_for_load(&app);
        assert_eq!(lock(&app.inner).generation, 1);
        assert!(app.start_load_mode(source.clone(), true, false, true));
        wait_for_load(&app);
        assert_eq!(lock(&app.inner).generation, 1, "unchanged sources do not invalidate queries");
        logfile::tests::game_log(&source, "00000000.log", 0, &["first", "second"]);
        lock(&app.inner).monitoring = false;
        assert!(!app.start_load_mode(source.clone(), true, false, true));
        assert_eq!(lock(&app.inner).entries.len(), 1);
        lock(&app.inner).monitoring = true;
        assert!(!app.start_load_mode(dir.join("old-location"), true, false, true));
        assert!(app.start_load_mode(source.clone(), true, false, true));
        wait_for_load(&app);
        let inner = lock(&app.inner);
        assert_eq!(inner.status, "ready");
        assert_eq!(inner.generation, 2);
        assert_eq!(inner.entries.len(), 2);
        drop(inner);
        // A new game file is discovered even when the current one has not changed.
        logfile::tests::game_log(&source, "00000001.log", 2, &["third"]);
        assert!(app.start_load_mode(source, true, false, true));
        wait_for_load(&app);
        assert_eq!(lock(&app.inner).entries.len(), 3);
    }

    fn wait_for_entries(app: &AppState, count: usize) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if lock(&app.inner).entries.len() == count { return true; }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn monitor_imports_when_the_game_writes_a_log() {
        let dir = logfile::tests::temp("live-watch");
        let source = dir.join("game");
        logfile::tests::game_log(&source, "00000000.log", 0, &["first"]);
        let app = Arc::new(AppState {
            inner: Mutex::new(Inner { monitoring: true, ..Inner::default() }),
            progress: [AtomicUsize::new(0), AtomicUsize::new(0)],
            data_dir: dir.clone(), archive_dir: dir.join("archive"),
            source_cache: Mutex::new(logfile::SourceCache::default()),
        });
        assert!(app.start_load(source.clone(), true, false));
        wait_for_load(&app);
        start_monitor(&app);
        // Let the watch start before the game writes.
        std::thread::sleep(std::time::Duration::from_millis(500));
        logfile::tests::game_log(&source, "00000000.log", 0, &["first", "second"]);
        assert!(wait_for_entries(&app, 2), "a written log is imported without polling");
        // Paused: changes wait until monitoring is enabled again.
        lock(&app.inner).monitoring = false;
        std::thread::sleep(std::time::Duration::from_millis(500));
        logfile::tests::game_log(&source, "00000001.log", 2, &["third"]);
        std::thread::sleep(std::time::Duration::from_millis(2000));
        assert_eq!(lock(&app.inner).entries.len(), 2);
        lock(&app.inner).monitoring = true;
        assert!(wait_for_entries(&app, 3), "resuming imports what was written meanwhile");
    }

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
            data_dir: std::env::temp_dir().join("aethelog-metadata-test-missing"),
            archive_dir: PathBuf::new(),
            source_cache: Mutex::new(logfile::SourceCache::default()),
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
