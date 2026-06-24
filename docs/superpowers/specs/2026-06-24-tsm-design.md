# tsm — tmux session manager

## Purpose

A full-screen terminal UI to browse running tmux sessions, see each session's
windows and what is running in them, then attach to ("jump into") or kill a
session. Built in Rust with `ratatui` + `crossterm`.

## Goals

- See all tmux sessions at a glance, including window count and attached state.
- Expand a session to see its windows and the command running in each.
- Attach to a session, auto-detecting whether we are inside or outside tmux.
- Kill a session with a confirmation prompt.

## Non-goals

- Creating, renaming, or otherwise editing sessions/windows.
- Managing panes individually.
- A daemon, config file, or persistent state. No async.

## Architecture

A thin `main.rs` owning the event loop, over three focused modules:

- **`tmux.rs`** — all interaction with the `tmux` binary. Builds session data
  from `tmux list-sessions` / `list-windows` using `-F` format strings (parsed,
  not screen-scraped). The command runner is abstracted behind a trait so the
  parser is testable without a real tmux server. Exposes:
  - `fn sessions(runner) -> Result<Vec<Session>>`
  - `fn kill(runner, name) -> Result<()>`
  - `fn attach_mode() -> AttachMode` (reads `$TMUX`)
  - `fn attach(name, mode)` — the terminal hand-off (see Behavior).
  No UI, no raw mode.
- **`app.rs`** — UI-agnostic application state and transitions. Holds the
  session list, selected index, per-session expanded flag, and a `Mode` enum
  (`Browsing` / `ConfirmKill { name }`) plus a transient status message.
  Methods: `select_next`, `select_prev`, `toggle_expand`, `request_kill`,
  `confirm_kill`, `cancel`, `set_sessions`. Pure state — no I/O, fully unit
  testable.
- **`ui.rs`** — renders `&App` into a `ratatui` frame. No state of its own.

`main.rs` wires it together: terminal setup (with a restore guard), the
read-key → mutate-`App` → redraw loop, and dispatching attach/kill via `tmux`.

## Data model

```rust
struct Session { name: String, windows: Vec<Window>, attached: bool }
struct Window  { index: u32, name: String, active: bool, current_command: String }
```

`current_command` is the active pane's `#{pane_current_command}` for that window
(e.g. `nvim`, `cargo`, `zsh`), so the user sees what each window is running.

## Behavior

- **List view**: one row per session — name, window count, attached marker.
  Navigate with `↑/↓` or `j/k` (selection wraps).
- **Expand**: `Tab` or `Space` toggles a session open to list its windows
  (index, name, running command); the active window is marked.
- **Attach** (`Enter`): leave raw mode / restore the terminal, then:
  - outside tmux (`$TMUX` unset): `exec tmux attach -t <name>` (replaces process).
  - inside tmux (`$TMUX` set): `tmux switch-client -t <name>`, then exit tsm.
- **Kill** (`x`): enter `ConfirmKill` mode showing `kill session "<name>"? (y/n)`.
  `y` runs `tmux kill-session -t <name>` and refreshes; `n`/`Esc` cancels.
- **Refresh** (`r`): re-query tmux.
- **Quit** (`q` / `Esc` when browsing).
- **Empty state**: "No tmux sessions running" with a short hint.

## Error handling

- `tmux` not on PATH or server not running → clean message to stderr, non-zero
  exit, terminal fully restored.
- Terminal raw-mode entered via a guard (Drop) so a panic or early return still
  restores the terminal — never leave the user with a broken shell.
- Kill/attach command failures surface in the status line, not as a crash.

## Testing

- `tmux.rs`: parser unit tests against captured `-F` output fixtures; command
  runner trait mocked, so no real tmux is required.
- `app.rs`: state-transition tests (selection wrapping, expand toggle, the
  request → confirm/cancel kill flow, status messages).
- `ui.rs`: `ratatui` `TestBackend` render assertions for list, expanded, and
  confirm-kill states.

## Dependencies

`ratatui`, `crossterm`, `anyhow`. Synchronous; no async runtime.
