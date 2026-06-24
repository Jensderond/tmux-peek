//! All interaction with the `tmux` binary, plus pure parsing of its output.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub windows: Vec<Window>,
    pub attached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub index: u32,
    pub name: String,
    pub active: bool,
    pub current_command: String,
}

pub const SESSION_FORMAT: &str = "#{session_name}\t#{?session_attached,1,0}";
pub const WINDOW_FORMAT: &str =
    "#{session_name}\t#{window_index}\t#{window_name}\t#{window_active}\t#{pane_current_command}";

fn parse_session_line(line: &str) -> Option<(String, bool)> {
    let mut parts = line.splitn(2, '\t');
    let name = parts.next()?.to_string();
    let attached = parts.next()? == "1";
    Some((name, attached))
}

fn parse_window_line(line: &str) -> Option<(String, Window)> {
    let parts: Vec<&str> = line.splitn(5, '\t').collect();
    if parts.len() < 5 {
        return None;
    }
    let index = parts[1].parse().ok()?;
    let window = Window {
        index,
        name: parts[2].to_string(),
        active: parts[3] == "1",
        current_command: parts[4].to_string(),
    };
    Some((parts[0].to_string(), window))
}

/// Combine raw `list-sessions` and `list-windows -a` output into sessions.
/// Windows are matched to their session by name and sorted by window index.
pub fn build_sessions(sessions_out: &str, windows_out: &str) -> Vec<Session> {
    let mut sessions: Vec<Session> = sessions_out
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(parse_session_line)
        .map(|(name, attached)| Session { name, windows: Vec::new(), attached })
        .collect();

    for line in windows_out.lines().filter(|l| !l.is_empty()) {
        if let Some((session_name, window)) = parse_window_line(line) {
            if let Some(session) = sessions.iter_mut().find(|s| s.name == session_name) {
                session.windows.push(window);
            }
        }
    }

    for session in &mut sessions {
        session.windows.sort_by_key(|w| w.index);
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_sessions_assembles_and_sorts_windows() {
        let sessions_out = "work\t1\nplay\t0\n";
        // note: window index 2 listed before 0 to prove sorting
        let windows_out = "\
work\t2\teditor\t0\tnvim
work\t0\tshell\t1\tzsh
play\t0\tbuild\t1\tcargo
";
        let got = build_sessions(sessions_out, windows_out);

        assert_eq!(got.len(), 2);

        assert_eq!(got[0].name, "work");
        assert!(got[0].attached);
        assert_eq!(got[0].windows.len(), 2);
        assert_eq!(got[0].windows[0].index, 0);
        assert_eq!(got[0].windows[0].name, "shell");
        assert!(got[0].windows[0].active);
        assert_eq!(got[0].windows[0].current_command, "zsh");
        assert_eq!(got[0].windows[1].index, 2);
        assert!(!got[0].windows[1].active);

        assert_eq!(got[1].name, "play");
        assert!(!got[1].attached);
        assert_eq!(got[1].windows.len(), 1);
        assert_eq!(got[1].windows[0].current_command, "cargo");
    }

    #[test]
    fn build_sessions_handles_empty_input() {
        assert_eq!(build_sessions("", ""), Vec::new());
    }

    #[test]
    fn build_sessions_ignores_windows_for_unknown_session() {
        let got = build_sessions("work\t0\n", "ghost\t0\tx\t1\tzsh\n");
        assert_eq!(got.len(), 1);
        assert!(got[0].windows.is_empty());
    }
}
