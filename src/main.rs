mod app;
mod tmux;
mod ui;

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use app::{App, Mode, PanePreview};
use tmux::TmuxCli;

/// What the event loop decided to do after it ends.
enum Outcome {
    Quit,
    Attach(String),
}

/// Restores the terminal on drop — covers normal exit, early return, and panic.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn refresh(app: &mut App, runner: &impl tmux::CommandRunner) {
    match tmux::sessions(runner) {
        Ok(sessions) => app.set_sessions(sessions),
        Err(e) => app.set_status(format!("refresh failed: {e}")),
    }
}

/// Snapshot the captured content of every window in the selected session,
/// one `PanePreview` per window. No selected session → empty preview.
fn update_preview(app: &mut App, runner: &impl tmux::CommandRunner) {
    let previews = match app.selected_session() {
        Some(session) => {
            let targets: Vec<String> = session
                .windows
                .iter()
                .map(|window| format!("{}:{}", session.name, window.index))
                .collect();
            // One tmux call for the whole session; if any window fails the
            // batch aborts, so fall back to per-window captures to isolate it.
            let contents = tmux::capture_panes(runner, &targets).unwrap_or_else(|_| {
                targets
                    .iter()
                    .map(|target| {
                        tmux::capture_pane(runner, target)
                            .unwrap_or_else(|_| "(unavailable)".to_string())
                    })
                    .collect()
            });
            session
                .windows
                .iter()
                .zip(contents)
                .map(|(window, content)| PanePreview {
                    title: format!("{}:{}", window.index, window.name),
                    content,
                })
                .collect()
        }
        None => Vec::new(),
    };
    app.set_preview(previews);
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    runner: &impl tmux::CommandRunner,
) -> Result<Outcome> {
    let mut preview_stale = true;
    loop {
        // Capture only once pending input is drained. Held arrow keys can
        // repeat faster than tmux answers; capturing per keypress would make
        // the list trail behind and keep scrolling after the key is released.
        if preview_stale && !event::poll(Duration::ZERO)? {
            update_preview(app, runner);
            preview_stale = false;
        }
        terminal.draw(|frame| ui::render(app, frame))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match &app.mode {
            Mode::Filtering => match key.code {
                KeyCode::Esc => {
                    app.clear_filter();
                    preview_stale = true;
                }
                KeyCode::Enter => {
                    // Enter while filtering attaches straight to the
                    // highlighted match — no separate "confirm filter" step.
                    // With no matches, just leave filtering so hints return.
                    if let Some(session) = app.selected_session() {
                        return Ok(Outcome::Attach(session.name.clone()));
                    }
                    app.confirm_filter();
                }
                KeyCode::Backspace => {
                    app.filter_backspace();
                    preview_stale = true;
                }
                KeyCode::Down => {
                    app.select_next();
                    preview_stale = true;
                }
                KeyCode::Up => {
                    app.select_prev();
                    preview_stale = true;
                }
                KeyCode::Char(c) => {
                    app.filter_push(c);
                    preview_stale = true;
                }
                _ => {}
            },
            Mode::ConfirmKill { .. } => match key.code {
                KeyCode::Char('y') => {
                    if let Some(name) = app.confirm_kill() {
                        match tmux::kill(runner, &name) {
                            Ok(()) => {
                                refresh(app, runner);
                                preview_stale = true;
                                app.set_status(format!("killed \"{name}\""));
                            }
                            Err(e) => app.set_status(format!("kill failed: {e}")),
                        }
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => app.cancel(),
                _ => {}
            },
            Mode::Browsing => {
                app.clear_status();
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(Outcome::Quit),
                    KeyCode::Char('/') => app.start_filter(),
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.select_next();
                        preview_stale = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.select_prev();
                        preview_stale = true;
                    }
                    KeyCode::Enter => {
                        if let Some(session) = app.selected_session() {
                            return Ok(Outcome::Attach(session.name.clone()));
                        }
                    }
                    KeyCode::Char('x') => app.request_kill(),
                    KeyCode::Char('r') => {
                        refresh(app, runner);
                        preview_stale = true;
                    }
                    KeyCode::Char(c) => {
                        if let Some(session) = app.shortcut_session(c) {
                            return Ok(Outcome::Attach(session.name.clone()));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn main() -> Result<()> {
    let runner = TmuxCli;
    let mode = tmux::attach_mode();

    // Queried before any terminal setup, so a missing tmux binary prints a
    // clean error and exits non-zero with the shell intact.
    let sessions = tmux::sessions(&runner)?;
    let mut app = App::new(sessions);

    let outcome = {
        let _guard = TerminalGuard;
        let mut terminal = setup_terminal()?;
        run(&mut terminal, &mut app, &runner)?
        // _guard drops here → terminal restored before any attach.
    };

    match outcome {
        Outcome::Quit => {}
        Outcome::Attach(name) => tmux::attach(&name, mode)?,
    }
    Ok(())
}
