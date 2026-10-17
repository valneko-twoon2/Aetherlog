---
name: aethelog-tool
description: Use when reading, changing or building Aethelog, the read-only FFXIV log viewer (Tauri 2 + Rust + Vue 3). Covers the file layout, entry points, Tauri commands, build and test steps, how the dictionary data is regenerated, and the constraints to keep. For the binary log format itself (SeString, auto-translate, channel kinds) use the ffxiv-log-format skill.
---

# Aethelog tool notes

A read-only desktop viewer for FFXIV logs: Rust (Tauri 2) backend, Vue 3 + Vite front end. The log format is documented in the `ffxiv-log-format` skill; this note covers only the tool's layout and how to work on it. Repository rules (language, commits, hooks) are in `AGENTS.md`.

## 1. Entry points (log_analyzer/)

| Path | Role |
|---|---|
| `src-tauri/src/lib.rs` | Tauri commands, `AppState`, the loader thread, the filter cache, character-name storage, the start-up log location |
| `src-tauri/src/preferences.rs` | Saved animation toggle and its persistence (unknown fields from older versions are ignored) |
| `src-tauri/src/logfile.rs` | File and record parsing (`parse_file`, `split_fields`), `find_logs`, `select`, CSV/JSON export (`export`), JST date maths, name guessing (`guess_names`) |
| `src-tauri/src/sestring.rs` | `tokenize`, `decode_text` (plain text), `spans` (styled spans for the UI), `packed_uint`, dictionary loading |
| `src-tauri/src/channels.rs` | Channel catalog (`catalog`, `name_of`, `fallback`, `GROUPS`) |
| `src-tauri/tauri.conf.json`, `capabilities/default.json` | Window, CSP, permissions (`core:default`, `dialog:allow-open`, `dialog:allow-save` only) |
| `web/src/api.js` | Thin layer that maps the old REST-style paths onto Tauri commands |
| `web/src/useLog.js` | Shared UI state (tab, selected channels, filters, paging) |
| `web/src/App.vue` | Top bar (search, export, menu) and the menu |
| `web/src/components/` | `ChatWindow.vue` (tabs, chips, day banners, pager), `ChatLine.vue`, `SeText.vue` |
| `web/src/glyphs.js` | Stand-in symbols for private-use glyphs and bitmap icons |
| `web/src/style.css` | The FFXIV-style theme (background, windows, buttons) |
| `src-tauri/src/dict.rs` | Lookup tables (`autotranslate.json` {text, lookup}, `bitmapicons.json`, `uicolor.json`): `current`/`install`, `load`, `download` (xivapi/ffxiv-datamining sheets and Dalamud's `BitmapFontIcon.cs`, converted in Rust), `status`. They are game text, so they are downloaded to `userdata/dict` on the first start and never compiled in or committed |
| `THIRD-PARTY-NOTICES.txt`, `tools/make_notices.mjs` (`.ps1`/`.bat`) | Licence texts of the Rust crates (Windows runtime closure) and npm packages that are in the exe, each text printed once with its packages. Committed. `src-tauri/notices.json` is compiled into the exe (`notices` command; `open_homepage` opens one package's stored https address); the about dialog's licence window lists the packages with a licence and homepage link each. Nothing is copied next to the exe. `publish.ps1` regenerates both files before every build, and `.githooks/pre-commit` runs `make_notices.mjs --check` whenever a `Cargo`/`package` manifest or lock file is staged, so a dependency change cannot be committed without regenerated notices |
| `tools/publish.ps1` | Builds the web front end and the release exe (`tauri build --no-bundle`) and copies `Aethelog.exe` and an executable-only `Aethelog_v<version>.zip` into `artifacts/` (git-ignored). Refuses to run while a copy of Aethelog is running |
| `tools/preview.ps1` | Front end only: starts the Vite dev server in a browser (`--open`) |
| `web/src/mock.js` | In-browser stand-in backend used by `npm run dev` outside Tauri: 150,000 fictional lines, a fake start-up load and slow large pages. Dev builds only, never shipped |
| `tools/make_demo_logs.ps1` | Writes fictional demo logs to `docs/demo-logs/` (used for README screenshots, so no real character names appear) |
| `tools/make_screenshots.ps1` (`.bat`, `.mjs`, `capture_window.ps1`) | Retakes `docs/images/main.png`, `menu.png` and `battle.png` from the built exe: copies the demo logs to `C:\Users\Public\Aethelog-demo-logs`, starts the exe with `AETHELOG_DATA` pointing at a scratch folder and the WebView2 debugging port open, clicks the tabs and the menu through that port and saves the window (PrintWindow, 1282 x 852) as PNG. `-Build` runs `publish.ps1` first. Needs Node 22+; do not touch the mouse while it runs. Always look at the pictures before committing |
| `tools/*.bat` | Double-click wrappers for the `.ps1` scripts (`publish`, `make_notices`, `preview`, `set_version`, `make_demo_logs`, `make_screenshots`) |
| `tools/start.bat` | Launches `artifacts\Aethelog.exe`, else `src-tauri\target\release\Aethelog.exe` |
| `.githooks/`, `.claude/hooks/` | Policy enforcement for commits (see `AGENTS.md`) |

## 2. Tauri commands and the UI calls

| Command | Purpose | Call in `api.js` |
|---|---|---|
| `state` | load status (`idle/loading/ready/error`), progress, path, count | `get('/api/state')` |
| `meta` | channel catalog, groups, counts per channel, characters (name, whether guessed), date range, days with line counts, warning and error counts | `get('/api/meta')` |
| `load_logs(path)` | start loading; an error string if the path is missing or holds no `.log` | `post('/api/load')` |
| `set_preferences(preferences)` | Save the animation toggle in `userdata/preferences.json`; state includes the preferences | `post('/api/preferences')` |
| `notices` | JSON of the bundled packages and licence texts (`src-tauri/notices.json`) | `get('/api/notices')` |
| `query(params)` | filter and fetch a page; `offset = -1` is the last page; `limit` is 1 to 1,000,000,000 (0 or less means 200); the UI's "all" sends 1,000,000,000 | `post('/api/query')` |
| `export_entries(params, path)` | write the filtered rows to a `.csv` or `.json` file | `download('/api/export')` (save dialog, then the command) |
| `set_name(id, name)` | set a character name by hand (empty clears it) | `post('/api/name')` |
| `open_lodestone(name, world)` | open the Lodestone search for that name in the system browser (Ctrl+click on a name) | `post('/api/lodestone')` |

- Row shape: `{i, t, k, who, msg, ch, w}` (`i` position overall, `t` unix seconds, `k` kind, `who`/`msg` styled spans, `ch` character folder name, `w` has warnings).
- Span keys: `s` text, `c` colour, `e` edge colour, `b` bold, `i` italic, `l` link type, `k` kind (`at` auto-translate, `icon`), `n`/`a` icon id/name, `u` unresolved.
- Only the most recent filter result is cached; a generation counter invalidates it on every load.
- The UI fetches a page in chunks of 4,000 rows (a count request first, then `offset`/`limit` chunks) so progress can be shown; the rows are `markRaw` so they are not wrapped in proxies.
- Loading has two phases reported through `state.phase`: `copy` (saving to the archive) then `read` (parsing the archive).
- Folder, file and save pickers come from the front end's dialog plugin; the backend has no file picker.

## 3. Build and test

```powershell
cd web; npm install; npm run build; cd ..          # web/dist
web\node_modules\.bin\tauri build --no-bundle      # src-tauri\target\release\Aethelog.exe
cd src-tauri; cargo test                           # unit tests
powershell -File tools\publish.ps1                # one step: build everything into artifacts/
powershell -File tools\preview.ps1                # front end only, in a browser, with mock data
$env:AETHELOG_LOGS='<log folder>'; cargo test --release real_logs -- --ignored --nocapture
```

- The `tauri` CLI looks for `src-tauri` in sub-folders, so run it from the repository root (it is not found from `web/`).
- `tauri.conf.json` has no `beforeBuildCommand`: **run `npm run build` in `web/` first**.
- `real_logs_summary` prints counts, warnings and guessed names for a real log folder. It matched the retired Python backend exactly on file count, record count, text length and warning count.
- For development, run `npm run dev` in `web/` (port 5173, fixed) in another terminal, then `tauri dev` from the root.
- The window icon that tao builds has a broken AND mask (one byte per pixel instead of one bit), which the Task Manager shows as stripes. `fix_window_icon` in `lib.rs` replaces the small and big window icons at start-up with ones extracted from the exe's own resources (`PrivateExtractIconsW`, no extra crate). Check with a `GetIconInfo` mask dump of `WM_GETICON` if it ever regresses.
- Icons originate from `src-tauri/app-icon.png`. After `tauri icon`, run `tools/generate_icons.ps1` (Python with Pillow required) to regenerate Windows sizes with legacy transparency masks for Task Manager.

## 4. Regenerating the data

Delete `userdata/dict` and start the app to download the tables again. `start_dictionary` in `lib.rs` runs first at start-up: it loads the saved files, else downloads in a background thread, and only then does the first read of the logs (`start_first_load`), because search text and exports are decoded at load time. While `state.dict` is `downloading`, `start_load_mode` and `load_logs` refuse to read, and the UI shows only a notice. If the download fails the logs are read anyway and missing entries degrade to a placeholder showing the raw payload in hex. `dict::download` fetches the Completion sheet, UIColor and the icon enum side by side, then every lookup sheet it names with 8 connections, parsing each sheet and cutting it to its id and name columns on the thread that fetched it (about 1.3 s on a good connection, against 5 s one at a time); keep it parallel. `cargo test matches_the_script_output -- --ignored` compared the Rust conversion with the retired PowerShell output (it needs the old `data/` and `tools/.cache`).

## 5. Constraints to keep

- **Never write to the original logs.** Export refuses anything but `.csv` / `.json`. CSV cells that start with `=`, `+`, `-` or `@` get a leading `'` to stop formula execution.
- Network at runtime: only the one-time dictionary download in `dict.rs` (made by the backend, so the CSP and plugin permissions are unchanged) and `open_lodestone`, which gives the system browser a fixed Lodestone address built from encoded parts. The CSP is `default-src self` plus IPC, and the only plugin permissions are the dialogs (no file-system or shell plugin).
- A character id must be `FFXIV_CHR` plus up to 32 hex digits. Names are whitespace-normalised and cut to 40 characters.
- **All app data lives next to the executable**, in `userdata\` (override with `AETHELOG_DATA`): `preferences.json` (animation toggle), `character_names.json` (manual names), `last_path.txt` (last opened location) and `archive\` (the local log archive). `tools/publish.ps1` replaces only `Aethelog.exe`, so `artifacts\userdata` survives a republish. Older versions kept this data in `%APPDATA%\dev.aetherlog.app` and `%LOCALAPPDATA%\dev.aetherlog.app\archive`; the first start moves or copies it into `userdata` (skipped when `AETHELOG_DATA` is set).
- **Local archive** (`logfile.rs`): every load first imports the source logs into `archive` and the app then reads only the archive. A session (one game log file) is a stream `<character>/log/<name>_<first record time>.alog`: 24-byte header (magic `ALG1`, first record time, next record number expected, record count, data length) and length-framed records. Import appends only the records whose number is not stored yet, so a growing log costs only its new lines. `.log` copies from older versions are folded into the streams and removed on the first run. The scan skips the archive itself, `node_modules`, `target`, `.git` and `demo-logs`. Originals are only read. Set `AETHELOG_DATA` together with `AETHELOG_LOGS` for demos so sample logs and names never mix with real ones.
- `Cargo.toml` turns debug information off for every profile (it only bloats `target/`).
- Default location: `AETHELOG_LOGS` if set (not remembered as the last location), else the last opened one, else `Documents\My Games\FINAL FANTASY XIV - A Realm Reborn`.
- Screenshots for the README must come from the demo logs only: run `tools/make_screenshots.ps1` (it does that) and check every image before committing.

## 6. UI direction (requested by the owner)

- Keep it simple. Always visible: search, export, menu, chat window. Filters and the log location live in the menu.
- Do not bring back: the log detail dialog, the analysis report, a quit button, or the seconds / newest-first toggles.
- The name is "Aethelog", with only the heading in Cinzel (`@fontsource/cinzel`) for the FFXIV feel. Never use a game character name in the tool name or UI.
- The background follows the FFXIV character-select screen (bright blue, light streaks, small particles). No crystal ornaments. Keep particles small.
- The window frame follows the viewport width (no maximum width; side margins use `clamp`).
- Page size defaults to all and is chosen from a selector, not typed (50 up to all). A "loading" popup appears only when a query is still running after 400 ms.
- Background particles drift and twinkle (CSS animation; disabled for reduced motion or the saved animation preference). The menu offers animation on/off; there is no import interval setting. Preferences are restored on launch; monitoring itself starts enabled.
- Automatic import (`start_monitor` in `lib.rs`) is event driven: a `notify` watch on the log location triggers a background import 1 second after the last `.log` change, plus a rescan every 60 seconds for missed notifications. There is no on/off switch in the UI; deleting a character pauses it (so the rescan does not restore what was deleted) and the next manual `load_logs` resumes it. Do not go back to fast polling: the game writes logs only at 1,000 lines or logout (see the `ffxiv-log-format` skill).
- Characters are chosen by name; show the ID only when no name could be determined.

## 7. Known limits

- Name detection is a guess; for a character whose own name never appears, set it by hand.
- De-duplicating conversations across characters, battle analysis and recovering overwritten logs are out of scope.
