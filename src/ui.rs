//! Renders `&App` into a ratatui frame. Holds no state.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{App, Mode};

const HINTS: &str = "↑/↓ move · / filter · Enter attach · x kill · r refresh · q quit";

pub fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    render_list(app, frame, body[0]);
    render_preview(app, frame, body[1]);
    render_footer(app, frame, chunks[1]);

    if let Mode::ConfirmKill { name } = &app.mode {
        let area = centered_rect(50, 20, frame.area());
        let popup = Paragraph::new(Line::from(format!("kill session \"{name}\"? (y/n)")))
            .block(Block::default().title(" confirm ").borders(Borders::ALL));
        frame.render_widget(Clear, area);
        frame.render_widget(popup, area);
    }
}

fn render_list(app: &App, frame: &mut Frame, area: Rect) {
    let title = if app.filter.is_empty() {
        " tmux sessions ".to_string()
    } else {
        format!(" tmux sessions — /{} ", app.filter)
    };
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.sessions.is_empty() {
        let empty = Paragraph::new(vec![
            Line::from("No tmux sessions running"),
            Line::from(""),
            Line::from("Press r to refresh, q to quit."),
        ]);
        frame.render_widget(empty, inner);
        return;
    }

    let visible = app.visible_sessions();
    if visible.is_empty() {
        let msg = Paragraph::new(Line::from(format!("No sessions match /{}", app.filter)));
        frame.render_widget(msg, inner);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    for (i, session) in visible.iter().enumerate() {
        let selected = i == app.selected;
        let marker = if selected { ">" } else { " " };
        let attached = if session.attached { " *" } else { "" };
        let row = format!(
            "{marker} {} [{} win]{attached}",
            session.name,
            session.windows.len()
        );
        let style = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::styled(row, style));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_preview(app: &App, frame: &mut Frame, area: Rect) {
    if app.preview.is_empty() {
        return;
    }

    // len > 0 is guaranteed by the is_empty() guard above, so Ratio's
    // denominator is never zero (which would panic in ratatui's layout).
    let constraints: Vec<Constraint> = (0..app.preview.len())
        .map(|_| Constraint::Ratio(1, app.preview.len() as u32))
        .collect();
    let cells = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (cell, preview) in cells.iter().zip(app.preview.iter()) {
        let block = Block::default()
            .title(preview.title.clone())
            .borders(Borders::ALL);
        let para = Paragraph::new(preview.content.clone()).block(block);
        frame.render_widget(para, *cell);
    }
}

fn render_footer(app: &App, frame: &mut Frame, area: Rect) {
    let text = match app.mode {
        Mode::Filtering => format!("/{}▏", app.filter),
        _ => app.status.clone().unwrap_or_else(|| HINTS.to_string()),
    };
    frame.render_widget(Paragraph::new(Line::from(text)), area);
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
    use crate::app::{App, PanePreview};
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

    fn sess(name: &str) -> Session {
        Session {
            name: name.to_string(),
            attached: false,
            windows: Vec::new(),
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

    #[test]
    fn preview_renders_one_titled_cell_per_window() {
        let mut app = App::new(vec![session_with_window()]);
        app.set_preview(vec![
            PanePreview {
                title: "0:editor".to_string(),
                content: "nvim running".to_string(),
            },
            PanePreview {
                title: "1:server".to_string(),
                content: "cargo run".to_string(),
            },
        ]);
        let text = render_to_string(&app);
        assert!(text.contains("0:editor"), "got:\n{text}");
        assert!(text.contains("nvim running"), "got:\n{text}");
        assert!(text.contains("1:server"), "got:\n{text}");
        assert!(text.contains("cargo run"), "got:\n{text}");
    }

    #[test]
    fn filter_prompt_shows_in_footer_while_filtering() {
        let mut app = App::new(vec![sess("web-app")]);
        app.start_filter();
        app.filter_push('w');
        app.filter_push('e');
        let text = render_to_string(&app);
        assert!(text.contains("/we"), "expected filter prompt in:\n{text}");
    }

    #[test]
    fn non_matching_sessions_are_hidden() {
        let mut app = App::new(vec![sess("web-app"), sess("db")]);
        app.filter = "web".to_string();
        let text = render_to_string(&app);
        assert!(text.contains("web-app"), "got:\n{text}");
        assert!(!text.contains("db "), "expected db hidden in:\n{text}");
    }

    #[test]
    fn empty_match_message_renders() {
        let mut app = App::new(vec![sess("web-app")]);
        app.filter = "zzz".to_string();
        let text = render_to_string(&app);
        assert!(text.contains("No sessions match /zzz"), "got:\n{text}");
    }

    #[test]
    fn filtered_title_shows_query() {
        let mut app = App::new(vec![sess("web-app")]);
        app.filter = "web".to_string();
        let text = render_to_string(&app);
        assert!(text.contains("tmux sessions — /web"), "got:\n{text}");
    }
}
