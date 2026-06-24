mod app;
mod tmux;
mod ui;

use std::io::{self, Stdout};

use anyhow::Result;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use app::{App, Mode};
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

fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    runner: &impl tmux::CommandRunner,
) -> Result<Outcome> {
    loop {
        terminal.draw(|frame| ui::render(app, frame))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        let confirming = matches!(app.mode, Mode::ConfirmKill { .. });
        if confirming {
            match key.code {
                KeyCode::Char('y') => {
                    if let Some(name) = app.confirm_kill() {
                        match tmux::kill(runner, &name) {
                            Ok(()) => {
                                refresh(app, runner);
                                app.set_status(format!("killed \"{name}\""));
                            }
                            Err(e) => app.set_status(format!("kill failed: {e}")),
                        }
                    }
                }
                KeyCode::Char('n') | KeyCode::Esc => app.cancel(),
                _ => {}
            }
        } else {
            app.status = None;
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(Outcome::Quit),
                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                KeyCode::Tab | KeyCode::Char(' ') => app.toggle_expand(),
                KeyCode::Enter => {
                    if let Some(session) = app.selected_session() {
                        return Ok(Outcome::Attach(session.name.clone()));
                    }
                }
                KeyCode::Char('x') => app.request_kill(),
                KeyCode::Char('r') => refresh(app, runner),
                _ => {}
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
