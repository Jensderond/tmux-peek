//! All interaction with the `tmux` binary, plus pure parsing of its output.

use anyhow::{Context, Result, anyhow};
use std::os::unix::process::CommandExt;
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
        .map(|(name, attached)| Session {
            name,
            windows: Vec::new(),
            attached,
        })
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

/// Capture the visible content of a target's active pane, preserving colors.
/// `target` is `<session>:<window_index>`, which resolves to that window's
/// active pane. `-e` keeps ANSI escape sequences so the preview renders with
/// the same colors tmux would show (parsed back into styled text by the UI).
pub fn capture_pane(runner: &impl CommandRunner, target: &str) -> Result<String> {
    runner.run(&["capture-pane", "-p", "-e", "-t", target])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachMode {
    /// Not currently inside tmux ($TMUX unset).
    Outside,
    /// Already inside a tmux client ($TMUX set).
    Inside,
}

fn attach_mode_from(tmux_var: Option<std::ffi::OsString>) -> AttachMode {
    match tmux_var {
        Some(v) if !v.is_empty() => AttachMode::Inside,
        _ => AttachMode::Outside,
    }
}

/// Determine attach mode from the live environment.
pub fn attach_mode() -> AttachMode {
    attach_mode_from(std::env::var_os("TMUX"))
}

/// Hand the terminal over to tmux. The caller MUST restore the terminal
/// (leave raw mode / alternate screen) before calling this.
pub fn attach(name: &str, mode: AttachMode) -> Result<()> {
    match mode {
        AttachMode::Outside => {
            // Replaces the current process; only returns on failure.
            let err = Command::new("tmux").args(["attach", "-t", name]).exec();
            Err(anyhow!("failed to exec `tmux attach -t {name}`: {err}"))
        }
        AttachMode::Inside => {
            let status = Command::new("tmux")
                .args(["switch-client", "-t", name])
                .status()
                .context("failed to run `tmux switch-client`")?;
            if !status.success() {
                return Err(anyhow!("tmux switch-client -t {name} failed"));
            }
            Ok(())
        }
    }
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
                Some("list-sessions") if self.fail_no_server => Err(anyhow::anyhow!(
                    "no server running on /tmp/tmux-501/default"
                )),
                Some("list-sessions") => Ok(self.sessions_out.clone()),
                Some("list-windows") => Ok(self.windows_out.clone()),
                Some("kill-session") => Ok(String::new()),
                Some("capture-pane") => Ok("captured content".to_string()),
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
    fn capture_pane_issues_capture_with_target() {
        let runner = MockRunner::new("", "");
        let out = capture_pane(&runner, "work:0").unwrap();
        assert_eq!(out, "captured content");
        let calls = runner.calls.borrow();
        // `-e` makes tmux emit ANSI escape sequences so the preview keeps
        // its colors instead of being flattened to plain text.
        assert_eq!(calls[0], vec!["capture-pane", "-p", "-e", "-t", "work:0"]);
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

    use std::ffi::OsString;

    #[test]
    fn attach_mode_inside_when_tmux_set() {
        assert_eq!(
            attach_mode_from(Some(OsString::from("/tmp/tmux-501/default,123,0"))),
            AttachMode::Inside
        );
    }

    #[test]
    fn attach_mode_outside_when_unset_or_empty() {
        assert_eq!(attach_mode_from(None), AttachMode::Outside);
        assert_eq!(attach_mode_from(Some(OsString::new())), AttachMode::Outside);
    }
}
