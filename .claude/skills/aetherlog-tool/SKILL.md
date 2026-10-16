---
name: aetherlog-tool
description: Use when reading, changing or building Aetherlog, the read-only FFXIV log viewer (Tauri 2 + Rust + Vue 3). Covers the file layout, entry points, Tauri commands, build and test steps, how the dictionary data is regenerated, and the constraints to keep. For the binary log format itself (SeString, auto-translate, channel kinds) use the ffxiv-log-format skill.
---

# Aetherlog tool notes

A read-only desktop viewer for FFXIV logs: Rust (Tauri 2) backend, Vue 3 + Vite front end. The log format is documented in the `ffxiv-log-format` skill; this note covers only the tool's layout and how to work on it. Repository rules (language, commits, hooks) are in `AGENTS.md`.

## 1. Entry points (log_analyzer/)

| Path | Role |
|---|---|
| `src-tauri/src/lib.rs` | Tauri commands, `AppState`, the loader thread, the filter cache, character-name storage, the start-up log location |
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
| `tools/publish.ps1` | Builds the web front end and the release exe (`tauri build --no-bundle`) and copies `Aetherlog.exe` into `artifacts/` (git-ignored). Refuses to run while a copy of Aetherlog is running |
| `tools/preview.ps1` | Front end only: starts the Vite dev server in a browser (`--open`) |
| `web/src/mock.js` | In-browser stand-in backend used by `npm run dev` outside Tauri: 150,000 fictional lines, a fake start-up load and slow large pages. Dev builds only, never shipped |
| `tools/make_demo_logs.ps1` | Writes fictional demo logs to `docs/demo-logs/` (used for README screenshots, so no real character names appear) |
| `tools/*.bat` | Double-click wrappers for the `.ps1` scripts (`publish`, `make_notices`, `preview`, `set_version`, `make_demo_logs`) |
| `tools/start.bat` | Launches `artifacts\Aetherlog.exe`, else `src-tauri\target\release\Aetherlog.exe` |
| `.githooks/`, `.claude/hooks/` | Policy enforcement for commits (see `AGENTS.md`) |

## 2. Tauri commands and the UI calls

| Command | Purpose | Call in `api.js` |
|---|---|---|
| `state` | load status (`idle/loading/ready/error`), progress, path, count | `get('/api/state')` |
| `meta` | channel catalog, groups, counts per channel, characters (name, whether guessed), date range, days with line counts, warning and error counts | `get('/api/meta')` |
| `load_logs(path)` | start loading; an error string if the path is missing or holds no `.log` | `post('/api/load')` |
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
web\node_modules\.bin\tauri build --no-bundle      # src-tauri\target\release\Aetherlog.exe
cd src-tauri; cargo test                           # unit tests
powershell -File tools\publish.ps1                # one step: build everything into artifacts/
powershell -File tools\preview.ps1                # front end only, in a browser, with mock data
$env:AETHERLOG_LOGS='<log folder>'; cargo test --release real_logs -- --ignored --nocapture
```

- The `tauri` CLI looks for `src-tauri` in sub-folders, so run it from the repository root (it is not found from `web/`).
- `tauri.conf.json` has no `beforeBuildCommand`: **run `npm run build` in `web/` first**.
- `real_logs_summary` prints counts, warnings and guessed names for a real log folder. It matched the retired Python backend exactly on file count, record count, text length and warning count.
- For development, run `npm run dev` in `web/` (port 5173, fixed) in another terminal, then `tauri dev` from the root.
- Icons originate from `src-tauri/app-icon.png`. After `tauri icon`, run `tools/generate_icons.ps1` (Python with Pillow required) to regenerate Windows sizes with legacy transparency masks for Task Manager.

## 4. Regenerating the data

Delete `userdata/dict` and start the app to download the tables again (`start_dictionary` in `lib.rs`: it loads the saved files, else downloads in a background thread, then reads the archive once more because search text and exports are decoded at load time; the UI keeps polling while `state.dict` is `downloading`). Missing entries degrade to a placeholder showing the raw payload in hex. `cargo test matches_the_script_output -- --ignored` compared the Rust conversion with the retired PowerShell output (it needs the old `data/` and `tools/.cache`).

## 5. Constraints to keep

- **Never write to the original logs.** Export refuses anything but `.csv` / `.json`. CSV cells that start with `=`, `+`, `-` or `@` get a leading `'` to stop formula execution.
- Network at runtime: only the one-time dictionary download in `dict.rs` (made by the backend, so the CSP and plugin permissions are unchanged) and `open_lodestone`, which gives the system browser a fixed Lodestone address built from encoded parts. The CSP is `default-src self` plus IPC, and the only plugin permissions are the dialogs (no file-system or shell plugin).
- A character id must be `FFXIV_CHR` plus up to 32 hex digits. Names are whitespace-normalised and cut to 40 characters.
- **All app data lives next to the executable**, in `userdata\` (override with `AETHERLOG_DATA`): `character_names.json` (manual names), `last_path.txt` (last opened location) and `archive\` (the local log archive). `tools/publish.ps1` replaces only `Aetherlog.exe`, so `artifacts\userdata` survives a republish. Older versions kept this data in `%APPDATA%\dev.aetherlog.app` and `%LOCALAPPDATA%\dev.aetherlog.app\archive`; the first start moves or copies it into `userdata` (skipped when `AETHERLOG_DATA` is set).
- **Local archive** (`logfile.rs`): every load first imports the source logs into `archive` and the app then reads only the archive. A session (one game log file) is a stream `<character>/log/<name>_<first record time>.alog`: 24-byte header (magic `ALG1`, first record time, next record number expected, record count, data length) and length-framed records. Import appends only the records whose number is not stored yet, so a growing log costs only its new lines. `.log` copies from older versions are folded into the streams and removed on the first run. The scan skips the archive itself, `node_modules`, `target`, `.git` and `demo-logs`. Originals are only read. Set `AETHERLOG_DATA` together with `AETHERLOG_LOGS` for demos so sample logs and names never mix with real ones.
- `Cargo.toml` turns debug information off for every profile (it only bloats `target/`).
- Default location: `AETHERLOG_LOGS` if set (not remembered as the last location), else the last opened one, else `Documents\My Games\FINAL FANTASY XIV - A Realm Reborn`.
- Screenshots for the README must come from the demo logs only: set `AETHERLOG_LOGS` to a copy of `docs/demo-logs` and check every image before committing.

## 6. UI direction (requested by the owner)

- Keep it simple. Always visible: search, export, menu, chat window. Filters and the log location live in the menu.
- Do not bring back: the log detail dialog, the analysis report, a quit button, or the seconds / newest-first toggles.
- The name is "Aetherlog", with only the heading in Cinzel (`@fontsource/cinzel`) for the FFXIV feel. Never use a game character name in the tool name or UI.
- The background follows the FFXIV character-select screen (bright blue, light streaks, small particles). No crystal ornaments. Keep particles small.
- The window frame follows the viewport width (no maximum width; side margins use `clamp`).
- Page size defaults to all and is chosen from a selector, not typed (50 up to all). A "loading" popup appears only when a query is still running after 400 ms.
- Background particles drift and twinkle (CSS animation; disabled for reduced motion).
- Characters are chosen by name; show the ID only when no name could be determined.

## 7. Known limits

- Name detection is a guess; for a character whose own name never appears, set it by hand.
- De-duplicating conversations across characters, battle analysis and recovering overwritten logs are out of scope.
