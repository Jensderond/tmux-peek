# Design: `/` filter + auto window-preview pane

## Goal

Two additions to the `tsm` tmux session browser:

1. **`/` filtering** — press `/`, type, and the session list narrows live to names
   matching the query.
2. **Auto preview pane** — a bottom split that, with no extra keypress, shows the
   live-captured terminal content of *every window* in the currently selected
   session, tiled side by side (like tmux `choose-tree`'s preview, but for all
   windows at once).

This replaces the existing Tab/inline expand feature entirely.

## Layout

Three vertical regions:

```
┌ tmux sessions — /web ──────────────────────────┐
│ > web-app   [3 win] *                           │   list (top, 50%)
│   web-api   [1 win]                             │
├──────────────┬──────────────┬───────────────────┤
│ 0:editor     │ 1:server     │ 2:logs            │   preview (bottom, 50%)
│ $ nvim       │ $ cargo run  │ $ tail -f log     │   one column per window,
│ ...          │ Compiling... │ ...               │   even widths
└──────────────┴──────────────┴───────────────────┘
 /web▏                                                footer (1 line)
```

- **List** — flat session rows. Inline Tab-expand is removed.
- **Preview** — the bottom 50%. Subdivided into one equal-width column per window
  of the selected session. Each column is a bordered block titled `idx:name`
  whose body is that window's active-pane captured text. Cramped is acceptable.
- **Footer** — one line: the live filter input while filtering, otherwise a
  status message or the key hints.

## Preview content (tmux.rs)

New function:

```rust
pub fn capture_pane(runner: &impl CommandRunner, target: &str) -> Result<String>
```

Runs `tmux capture-pane -p -t <target>`. Targets are per-window:
`<session>:<window_index>`, which resolves to that window's active pane — the
same content choose-tree previews, captured for each window.

- Plain text only (no `-e` / ANSI) for v1.
- **Snapshot semantics**: panes are captured when the selection changes and on
  `r` refresh. No background polling, so the event loop stays blocking and idle
  CPU stays at zero.

## State (app.rs)

`app.rs` remains I/O-free; capture happens in `main.rs` and results are pushed in.

Changes to `App`:

- **Remove** `expanded: HashSet<String>`, `is_expanded`, `toggle_expand`, and the
  expanded-pruning logic in `set_sessions`.
- **Add** `filter: String` (empty = no filter).
- **Add** `preview: Vec<PanePreview>` where
  `PanePreview { title: String, content: String }`.
- `Mode` gains a `Filtering` variant:
  `Browsing | Filtering | ConfirmKill { name: String }`.

Selection now indexes the **filtered (visible)** list:

- `visible_sessions(&self) -> Vec<&Session>` — all sessions whose name contains
  `filter` as a case-insensitive substring (all sessions when `filter` is empty).
- `selected_session()` returns `visible_sessions().get(selected)`.
- `select_next` / `select_prev` wrap within the visible list length.

Filter methods (all clamp/reset selection to the top of the visible list):

- `start_filter()` — `mode = Filtering`, keeping any existing `filter` so `/`
  re-edits the current query.
- `filter_push(c)` — append char, reset `selected = 0`.
- `filter_backspace()` — pop char, reset `selected = 0`.
- `confirm_filter()` — `mode = Browsing`, keep `filter` applied.
- `clear_filter()` — empty `filter`, `mode = Browsing`, reset `selected = 0`.

Preview setter (pure):

- `set_preview(previews: Vec<PanePreview>)`.

## Event loop (main.rs)

Helper:

```rust
fn update_preview(app: &mut App, runner: &impl CommandRunner)
```

Captures the selected session's windows: for each window, `capture_pane` with
target `<name>:<index>`, building a `PanePreview { title: "idx:name", content }`.
A failed capture for one window yields `content = "(unavailable)"` rather than
aborting. With no selected session, sets an empty `Vec`. Called at startup and
after anything that changes the selection (navigation, refresh, filter edits,
kill).

Mode branches:

- **Filtering**: `Esc` → `clear_filter` + update preview; `Enter` → `confirm_filter`;
  `Backspace` → `filter_backspace` + update preview; `↑/↓` → navigate filtered
  list + update preview; any `Char(c)` → `filter_push(c)` + update preview.
  (All letters are literal text here — j/k/q/x do not act as commands.)
- **ConfirmKill**: unchanged (`y` kills + refreshes + updates preview, `n`/`Esc`
  cancels).
- **Browsing**: `/` → `start_filter`; Tab/space expand removed; `↑/↓`/`j`/`k`
  navigate + update preview; `Enter` attach; `x` request kill; `r` refresh +
  update preview; `q`/`Esc` quit.

## Rendering (ui.rs)

- Outer split: body `Min(1)` + footer `Length(1)`. Body splits into list and
  preview at `Percentage(50)` / `Percentage(50)`.
- **List**: title ` tmux sessions ` normally; ` tmux sessions — /<query> ` when
  `filter` is non-empty. Renders only `visible_sessions()`. When the filter
  matches nothing: "No sessions match /<query>". Existing empty-server state
  ("No tmux sessions running") is preserved for when there are genuinely no
  sessions.
- **Preview**: split the preview rect into N equal-width columns
  (`Constraint::Ratio(1, n)` per cell), one per `PanePreview`. Each cell is a
  bordered `Block` titled with `preview.title`, body is `preview.content` clipped
  to the cell (no scroll in v1). Empty `preview` (no session) → blank pane.
- **Footer**: `Filtering` → `/<query>▏`; otherwise `status` if present, else
  `HINTS`. `HINTS` updated to include `/ filter` and drop `Tab expand`, e.g.
  `↑/↓ move · / filter · Enter attach · x kill · r refresh · q quit`.
- The `ConfirmKill` popup is unchanged.

## Testing

- **app.rs**
  - `visible_sessions` narrows by case-insensitive substring; empty filter shows all.
  - `selected_session` follows the filtered list; navigation wraps within it.
  - Editing the filter resets selection to the top; clamps when matches shrink.
  - `start_filter` / `confirm_filter` / `clear_filter` mode + filter transitions.
  - `set_preview` stores the previews.
- **tmux.rs**
  - `capture_pane` issues `capture-pane -p -t <session>:<index>` (MockRunner gains
    a `capture-pane` arm returning canned content).
- **ui.rs**
  - Preview renders one titled cell per window with its captured content.
  - Filter prompt shows in the footer while filtering.
  - Non-matching sessions are hidden; empty-match message renders.
  - Existing tests updated for the removed inline-expand behavior.

## Out of scope (v1)

- ANSI/color in the preview (plain text only).
- Preview scrolling.
- Live/polled preview refresh (snapshot on selection only).
- Fuzzy matching or matching on window names (name substring only).
- Grid wrapping of preview cells (even columns only).
