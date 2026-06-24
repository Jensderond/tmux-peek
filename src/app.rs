//! UI-agnostic application state and transitions. No I/O.

use std::collections::HashSet;

use crate::tmux::Session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browsing,
    ConfirmKill { name: String },
}

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub expanded: HashSet<String>,
    pub mode: Mode,
    pub status: Option<String>,
}

impl App {
    pub fn new(sessions: Vec<Session>) -> Self {
        Self {
            sessions,
            selected: 0,
            expanded: HashSet::new(),
            mode: Mode::Browsing,
            status: None,
        }
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.sessions.get(self.selected)
    }

    pub fn is_expanded(&self, name: &str) -> bool {
        self.expanded.contains(name)
    }

    pub fn select_next(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.sessions.len();
    }

    pub fn select_prev(&mut self) {
        if self.sessions.is_empty() {
            return;
        }
        let len = self.sessions.len();
        self.selected = (self.selected + len - 1) % len;
    }

    pub fn toggle_expand(&mut self) {
        if let Some(session) = self.sessions.get(self.selected) {
            let name = session.name.clone();
            if !self.expanded.remove(&name) {
                self.expanded.insert(name);
            }
        }
    }

    pub fn request_kill(&mut self) {
        if let Some(session) = self.sessions.get(self.selected) {
            self.mode = Mode::ConfirmKill { name: session.name.clone() };
        }
    }

    /// If in confirm mode, return the target name and reset to Browsing.
    pub fn confirm_kill(&mut self) -> Option<String> {
        if let Mode::ConfirmKill { name } = &self.mode {
            let name = name.clone();
            self.mode = Mode::Browsing;
            Some(name)
        } else {
            None
        }
    }

    pub fn cancel(&mut self) {
        self.mode = Mode::Browsing;
    }

    pub fn set_sessions(&mut self, sessions: Vec<Session>) {
        let names: HashSet<&str> = sessions.iter().map(|s| s.name.as_str()).collect();
        self.expanded.retain(|n| names.contains(n.as_str()));
        if self.selected >= sessions.len() {
            self.selected = sessions.len().saturating_sub(1);
        }
        self.sessions = sessions;
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some(msg.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::Session;

    fn sess(name: &str) -> Session {
        Session { name: name.to_string(), windows: Vec::new(), attached: false }
    }

    fn app3() -> App {
        App::new(vec![sess("a"), sess("b"), sess("c")])
    }

    #[test]
    fn select_next_wraps() {
        let mut app = app3();
        assert_eq!(app.selected, 0);
        app.select_next();
        app.select_next();
        assert_eq!(app.selected, 2);
        app.select_next();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn select_prev_wraps() {
        let mut app = app3();
        app.select_prev();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn navigation_on_empty_list_is_noop() {
        let mut app = App::new(Vec::new());
        app.select_next();
        app.select_prev();
        assert_eq!(app.selected, 0);
        assert!(app.selected_session().is_none());
    }

    #[test]
    fn toggle_expand_adds_and_removes_by_name() {
        let mut app = app3();
        assert!(!app.is_expanded("a"));
        app.toggle_expand();
        assert!(app.is_expanded("a"));
        app.toggle_expand();
        assert!(!app.is_expanded("a"));
    }

    #[test]
    fn request_kill_enters_confirm_for_selected() {
        let mut app = app3();
        app.select_next(); // select "b"
        app.request_kill();
        assert_eq!(app.mode, Mode::ConfirmKill { name: "b".to_string() });
    }

    #[test]
    fn confirm_kill_returns_name_and_resets() {
        let mut app = app3();
        app.request_kill();
        assert_eq!(app.confirm_kill(), Some("a".to_string()));
        assert_eq!(app.mode, Mode::Browsing);
        // calling again in Browsing yields nothing
        assert_eq!(app.confirm_kill(), None);
    }

    #[test]
    fn cancel_returns_to_browsing() {
        let mut app = app3();
        app.request_kill();
        app.cancel();
        assert_eq!(app.mode, Mode::Browsing);
    }

    #[test]
    fn set_sessions_clamps_selection_and_prunes_expanded() {
        let mut app = app3();
        app.select_next();
        app.select_next(); // selected = 2 ("c")
        app.expanded.insert("c".to_string());
        app.set_sessions(vec![sess("a")]); // "c" gone
        assert_eq!(app.selected, 0);
        assert!(!app.is_expanded("c"));
    }
}
