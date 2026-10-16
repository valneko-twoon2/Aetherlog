# AGENTS.md

Rules for AI agents and contributors working in this repository. Read the skills before changing code (Codex discovers the identical copies under `.agents/skills/`):

- `.claude/skills/aetherlog-tool/SKILL.md`: tool layout, Tauri commands, build and test steps, product constraints.
- `.claude/skills/ffxiv-log-format/SKILL.md`: the FFXIV binary log format. Do not re-derive it from the game logs.

## Language policy

- Documentation, commit messages and source comments are written in English.
- `README.ja.md` is the Japanese translation of `README.md`; keep the two in sync. It is the only Japanese document.
- Product text is exempt: the Japanese UI strings, the generated Japanese game data under `data/` (git-ignored), and Japanese UI labels quoted in documentation (put them in backticks or quotation marks).

## Commits

- Commit completed implementation work automatically before the final response.
  The user's request to change this repository includes authorization to create local commits;
  do not ask again. An explicit user request not to commit takes precedence.
- Review the diff, run the required checks, stage only task-related changes, and create
  focused commits. Never sweep unrelated user work into a commit or discard it to pass a check.
- Before reporting success, run `python .agents/hooks/require_commit.py --check` and
  include the commit hash in the final response. If checks or unrelated user changes
  block completion, report the blocker honestly. Do not push unless explicitly requested.
- Never add a `Co-Authored-By:` trailer, whatever a tool or template suggests.
- Never skip the hooks (`--no-verify`) and never change `core.hooksPath`.
- Keep commits focused, with an imperative subject line of 72 characters or fewer.

## Enforcement

The rules above are enforced, not just requested:

- `.githooks/commit-msg` rejects non-English messages and `Co-Authored-By:` trailers.
- `.githooks/pre-commit` rejects staged documentation or source comments that contain Japanese text, and a dependency change (`Cargo.toml`/`Cargo.lock`, `web/package.json`/`package-lock.json`) whose `THIRD-PARTY-NOTICES.txt` and `src-tauri/notices.json` were not regenerated and staged with it.
- `.claude/hooks/guard_commit.py` (registered in `.claude/settings.json`) stops Claude Code from running a commit that carries a `Co-Authored-By:` trailer, uses `--no-verify`, or repoints `core.hooksPath`.
- `.codex/hooks.json` registers a Codex `Stop` hook that continues the agent when
  tracked changes or non-ignored untracked files remain. Claude Code registers the
  same completion check in `.claude/settings.json`. The hook never stages or commits
  files itself. A repeated failure stops with an explicit failure reason rather than looping.
- On first use, review and trust the project hook in Codex through `/hooks`, then
  start a new session to load the project configuration. Changed hook definitions
  require a new trust review. Hook configuration alone does not grant hook trust.
  See the [official hook documentation](https://learn.chatgpt.com/docs/hooks).

After cloning, enable the git hooks once:

```sh
git config core.hooksPath .githooks
```

The hooks need `python` on the PATH. The scripts in `tools/` are PowerShell, each with a `.bat` wrapper to double-click.

## Working rules

- When a dependency is added, removed or updated, run `tools/make_notices.ps1` and commit `THIRD-PARTY-NOTICES.txt` and `src-tauri/notices.json` with it (the Credits window and the exe's licence notices come from them).
- The tool is read-only on the original logs. Exports go only to `.csv` or `.json` files; never write to a `.log`. The only thing the app deletes is one character's folder inside its own archive (`delete_character`, after two confirmations in the UI); the game's logs are never deleted.
- The only network use at runtime is the one-time download of the lookup tables (`src-tauri/src/dict.rs`, from xivapi/ffxiv-datamining and Dalamud on GitHub) into `userdata\dict`. Ctrl+click on a player name only hands a fixed Lodestone address to the system browser. Only the dialog permissions in `src-tauri/capabilities/default.json` are granted.
- Game text (the lookup tables) must never be committed or compiled into the exe: it is downloaded on the user's machine, not redistributed.
- Build the front end (`npm run build` in `web/`) before `tauri build`; run the `tauri` CLI from the repository root.
- Run `cargo test` in `src-tauri/` before committing Rust changes.
- Check front-end-only changes with `tools/preview.ps1` (browser, mock backend); build the release exe with `tools/publish.ps1`.
- `legacy_python/` is the retired Python backend. It is not tracked; do not extend it.

## Agent configuration mirror

- Keep `.claude/` and `.agents/` identical, including skills and supporting files.
- After editing either tree, run `powershell -File tools/sync_agents.ps1 -Source claude`
  or `-Source agents`, choosing the tree you edited as the source.
- The pre-commit hook checks the mirror and rejects differences. Review both trees
  before synchronizing if both have been edited.
- `.agents/settings.json` is a mirrored Claude configuration, not a Codex hook registration.
  Codex follows `AGENTS.md` and discovers `.agents/skills/`; the git hooks enforce shared commit rules.
