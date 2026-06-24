//! All interaction with the `tmux` binary, plus pure parsing of its output.

use anyhow::{anyhow, Context, Result};
use std::process::Command;

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

/// Abstraction over running `tmux` subcommands, so parsing is testable
/// without a real tmux server.
pub trait CommandRunner {
    fn run(&self, args: &[&str]) -> Result<String>;
}

/// The real runner: shells out to the `tmux` binary on PATH.
pub struct TmuxCli;

impl CommandRunner for TmuxCli {
    fn run(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("tmux")
            .args(args)
            .output()
            .context("failed to run tmux; is it installed and on PATH?")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("tmux {}: {}", args.join(" "), stderr.trim()));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// Query all sessions with their windows. A missing tmux *server* (no
/// sessions exist) is reported by tmux as an error; we treat that as an
/// empty list so the UI shows its empty state rather than crashing.
pub fn sessions(runner: &impl CommandRunner) -> Result<Vec<Session>> {
    let sessions_out = match runner.run(&["list-sessions", "-F", SESSION_FORMAT]) {
        Ok(out) => out,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("no server running") || msg.contains("no sessions") {
                return Ok(Vec::new());
            }
            return Err(e);
        }
    };
    let windows_out = runner.run(&["list-windows", "-a", "-F", WINDOW_FORMAT])?;
    Ok(build_sessions(&sessions_out, &windows_out))
}

/// Kill a session by name.
pub fn kill(runner: &impl CommandRunner, name: &str) -> Result<()> {
    runner.run(&["kill-session", "-t", name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct MockRunner {
        sessions_out: String,
        windows_out: String,
        fail_no_server: bool,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl MockRunner {
        fn new(sessions_out: &str, windows_out: &str) -> Self {
            Self {
                sessions_out: sessions_out.to_string(),
                windows_out: windows_out.to_string(),
                fail_no_server: false,
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for MockRunner {
        fn run(&self, args: &[&str]) -> anyhow::Result<String> {
            self.calls
                .borrow_mut()
                .push(args.iter().map(|s| s.to_string()).collect());
            match args.first().copied() {
                Some("list-sessions") if self.fail_no_server => {
                    Err(anyhow::anyhow!("no server running on /tmp/tmux-501/default"))
                }
                Some("list-sessions") => Ok(self.sessions_out.clone()),
                Some("list-windows") => Ok(self.windows_out.clone()),
                Some("kill-session") => Ok(String::new()),
                _ => Ok(String::new()),
            }
        }
    }

    #[test]
    fn sessions_parses_via_runner() {
        let runner = MockRunner::new("work\t1\n", "work\t0\tshell\t1\tzsh\n");
        let got = sessions(&runner).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "work");
        assert_eq!(got[0].windows[0].current_command, "zsh");
    }

    #[test]
    fn sessions_returns_empty_when_no_server() {
        let mut runner = MockRunner::new("", "");
        runner.fail_no_server = true;
        assert_eq!(sessions(&runner).unwrap(), Vec::new());
    }

    #[test]
    fn kill_invokes_kill_session_with_target() {
        let runner = MockRunner::new("", "");
        kill(&runner, "work").unwrap();
        let calls = runner.calls.borrow();
        assert_eq!(calls[0], vec!["kill-session", "-t", "work"]);
    }

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
