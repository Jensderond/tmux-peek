//! Renders `&App` into a ratatui frame. Holds no state.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{App, Mode};

const HINTS: &str = "↑/↓ move · Tab expand · Enter attach · x kill · r refresh · q quit";

pub fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let block = Block::default().title(" tmux sessions ").borders(Borders::ALL);
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    if app.sessions.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from("No tmux sessions running"),
            Line::from(""),
            Line::from("Press r to refresh, q to quit."),
        ]);
        frame.render_widget(empty, inner);
    } else {
        let mut lines: Vec<Line> = Vec::new();
        for (i, session) in app.sessions.iter().enumerate() {
            let selected = i == app.selected;
            let marker = if selected { ">" } else { " " };
            let caret = if app.is_expanded(&session.name) { "▾" } else { "▸" };
            let attached = if session.attached { " *" } else { "" };
            let row = format!(
                "{marker} {caret} {} [{} win]{attached}",
                session.name,
                session.windows.len()
            );
            let style = if selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            lines.push(Line::styled(row, style));

            if app.is_expanded(&session.name) {
                for window in &session.windows {
                    let active = if window.active { "*" } else { " " };
                    lines.push(Line::from(format!(
                        "      {active}{}: {} ({})",
                        window.index, window.name, window.current_command
                    )));
                }
            }
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }

    let footer_text = app.status.clone().unwrap_or_else(|| HINTS.to_string());
    frame.render_widget(Paragraph::new(Line::from(footer_text)), chunks[1]);

    if let Mode::ConfirmKill { name } = &app.mode {
        let area = centered_rect(50, 20, frame.area());
        let popup = Paragraph::new(Line::from(format!("kill session \"{name}\"? (y/n)")))
            .block(Block::default().title(" confirm ").borders(Borders::ALL));
        frame.render_widget(Clear, area);
        frame.render_widget(popup, area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::tmux::{Session, Window};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn buffer_to_string(buf: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    fn render_to_string(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        terminal.draw(|f| render(app, f)).unwrap();
        buffer_to_string(terminal.backend().buffer())
    }

    fn session_with_window() -> Session {
        Session {
            name: "work".to_string(),
            attached: true,
            windows: vec![Window {
                index: 0,
                name: "editor".to_string(),
                active: true,
                current_command: "nvim".to_string(),
            }],
        }
    }

    #[test]
    fn renders_session_list() {
        let app = App::new(vec![session_with_window()]);
        let text = render_to_string(&app);
        assert!(text.contains("work"), "expected session name in:\n{text}");
        assert!(text.contains("1 win"), "expected window count in:\n{text}");
    }

    #[test]
    fn expanded_session_shows_windows() {
        let mut app = App::new(vec![session_with_window()]);
        app.toggle_expand();
        let text = render_to_string(&app);
        assert!(text.contains("editor"), "expected window name in:\n{text}");
        assert!(text.contains("nvim"), "expected command in:\n{text}");
    }

    #[test]
    fn empty_state_message() {
        let app = App::new(Vec::new());
        let text = render_to_string(&app);
        assert!(text.contains("No tmux sessions running"), "got:\n{text}");
    }

    #[test]
    fn confirm_kill_popup() {
        let mut app = App::new(vec![session_with_window()]);
        app.request_kill();
        let text = render_to_string(&app);
        assert!(text.contains("kill session"), "got:\n{text}");
        assert!(text.contains("(y/n)"), "got:\n{text}");
    }
}
