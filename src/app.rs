//! UI-agnostic application state and transitions. No I/O.

use crate::tmux::Session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browsing,
    Filtering,
    ConfirmKill { name: String },
}

/// Captured terminal content for a single window, shown in the preview pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanePreview {
    pub title: String,
    pub content: String,
}

/// Quick-select keys, assigned to visible sessions top to bottom: digits
/// first, then letters not already bound in browsing mode (j, k, q, r, x).
const SHORTCUT_KEYS: &[u8] = b"0123456789abcdefghilmnopstuvwyz";

/// The quick-select key for the session at `index` in the visible list, or
/// `None` once the keys run out.
pub fn shortcut_label(index: usize) -> Option<char> {
    SHORTCUT_KEYS.get(index).map(|&b| b as char)
}

pub struct App {
    pub sessions: Vec<Session>,
    pub selected: usize,
    pub filter: String,
    pub preview: Vec<PanePreview>,
    pub mode: Mode,
    pub status: Option<String>,
}

impl App {
    pub fn new(sessions: Vec<Session>) -> Self {
        Self {
            sessions,
            selected: 0,
            filter: String::new(),
            preview: Vec::new(),
            mode: Mode::Browsing,
            status: None,
        }
    }

    /// All sessions whose name contains `filter` as a case-insensitive
    /// substring. Every session when `filter` is empty.
    pub fn visible_sessions(&self) -> Vec<&Session> {
        if self.filter.is_empty() {
            return self.sessions.iter().collect();
        }
        let needle = self.filter.to_lowercase();
        self.sessions
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn selected_session(&self) -> Option<&Session> {
        self.visible_sessions().get(self.selected).copied()
    }

    /// The visible session labelled with quick-select key `key`, if any.
    pub fn shortcut_session(&self, key: char) -> Option<&Session> {
        let index = SHORTCUT_KEYS.iter().position(|&b| b as char == key)?;
        self.visible_sessions().get(index).copied()
    }

    pub fn select_next(&mut self) {
        let len = self.visible_sessions().len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    pub fn select_prev(&mut self) {
        let len = self.visible_sessions().len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected + len - 1) % len;
    }

    /// Enter filtering mode, keeping any existing filter so `/` re-edits it.
    pub fn start_filter(&mut self) {
        self.mode = Mode::Filtering;
    }

    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    pub fn filter_backspace(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    /// Leave filtering mode, keeping the filter applied.
    pub fn confirm_filter(&mut self) {
        self.mode = Mode::Browsing;
    }

    /// Clear the filter and return to browsing.
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.mode = Mode::Browsing;
        self.selected = 0;
    }

    pub fn request_kill(&mut self) {
        if let Some(session) = self.selected_session() {
            self.mode = Mode::ConfirmKill {
                name: session.name.clone(),
            };
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
        self.sessions = sessions;
        let len = self.visible_sessions().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
    }

    pub fn set_preview(&mut self, previews: Vec<PanePreview>) {
        self.preview = previews;
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some(msg.into());
    }

    pub fn clear_status(&mut self) {
        self.status = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::Session;

    fn sess(name: &str) -> Session {
        Session {
            name: name.to_string(),
            windows: Vec::new(),
            attached: false,
        }
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
    fn request_kill_enters_confirm_for_selected() {
        let mut app = app3();
        app.select_next(); // select "b"
        app.request_kill();
        assert_eq!(
            app.mode,
            Mode::ConfirmKill {
                name: "b".to_string()
            }
        );
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
    fn set_sessions_clamps_selection() {
        let mut app = app3();
        app.select_next();
        app.select_next(); // selected = 2 ("c")
        app.set_sessions(vec![sess("a")]);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn visible_sessions_narrows_case_insensitive_substring() {
        let app = App::new(vec![sess("web-app"), sess("web-api"), sess("db")]);
        let visible = app.visible_sessions();
        assert_eq!(visible.len(), 3);

        let mut app = App::new(vec![sess("web-app"), sess("web-api"), sess("db")]);
        app.filter = "WEB".to_string();
        let names: Vec<&str> = app
            .visible_sessions()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, vec!["web-app", "web-api"]);
    }

    #[test]
    fn visible_sessions_empty_filter_shows_all() {
        let app = app3();
        assert_eq!(app.visible_sessions().len(), 3);
    }

    #[test]
    fn selected_session_follows_filtered_list() {
        let mut app = App::new(vec![sess("web-app"), sess("db"), sess("web-api")]);
        app.filter = "web".to_string();
        assert_eq!(app.selected_session().unwrap().name, "web-app");
        app.select_next();
        assert_eq!(app.selected_session().unwrap().name, "web-api");
        // wraps within the visible list of two
        app.select_next();
        assert_eq!(app.selected_session().unwrap().name, "web-app");
    }

    #[test]
    fn filter_edit_resets_selection_to_top() {
        let mut app = App::new(vec![sess("web-app"), sess("web-api"), sess("db")]);
        app.select_next();
        app.select_next();
        assert_eq!(app.selected, 2);
        app.filter_push('w');
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn filter_push_keeps_selection_valid_when_matches_shrink() {
        // Two matches selected past the top, then narrow to a single match:
        // selection must land on a real session, not dangle out of bounds.
        let mut app = App::new(vec![sess("web-app"), sess("web-api"), sess("db")]);
        app.filter = "web".to_string();
        app.select_next();
        assert_eq!(app.selected, 1);
        // "web-a"/"web-ap" still match both web-app and web-api; narrow fully.
        for c in ['-', 'a', 'p', 'p'] {
            app.filter_push(c);
        }
        assert_eq!(app.visible_sessions().len(), 1);
        assert_eq!(app.selected, 0);
        assert_eq!(
            app.selected_session().map(|s| s.name.as_str()),
            Some("web-app")
        );
    }

    #[test]
    fn filter_backspace_resets_selection_to_top() {
        let mut app = app3();
        app.filter = "x".to_string();
        app.select_next();
        app.filter_backspace();
        assert_eq!(app.selected, 0);
        assert_eq!(app.filter, "");
    }

    #[test]
    fn start_filter_keeps_existing_query() {
        let mut app = app3();
        app.filter = "ab".to_string();
        app.start_filter();
        assert_eq!(app.mode, Mode::Filtering);
        assert_eq!(app.filter, "ab");
    }

    #[test]
    fn confirm_filter_keeps_filter() {
        let mut app = app3();
        app.start_filter();
        app.filter_push('a');
        app.confirm_filter();
        assert_eq!(app.mode, Mode::Browsing);
        assert_eq!(app.filter, "a");
    }

    #[test]
    fn clear_filter_empties_and_browses() {
        let mut app = app3();
        app.start_filter();
        app.filter_push('a');
        app.select_next();
        app.clear_filter();
        assert_eq!(app.mode, Mode::Browsing);
        assert_eq!(app.filter, "");
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn shortcut_labels_digits_then_unbound_letters() {
        assert_eq!(shortcut_label(0), Some('0'));
        assert_eq!(shortcut_label(9), Some('9'));
        assert_eq!(shortcut_label(10), Some('a'));
        // Browsing keys (j, k, q, r, x) are never used as labels.
        let labels: Vec<char> = (0..).map_while(shortcut_label).collect();
        for bound in ['j', 'k', 'q', 'r', 'x'] {
            assert!(!labels.contains(&bound), "{bound} must not be a label");
        }
        assert_eq!(shortcut_label(labels.len()), None);
    }

    #[test]
    fn shortcut_session_picks_by_label() {
        let app = app3();
        assert_eq!(app.shortcut_session('0').unwrap().name, "a");
        assert_eq!(app.shortcut_session('2').unwrap().name, "c");
        assert!(app.shortcut_session('3').is_none());
        assert!(app.shortcut_session('j').is_none());
    }

    #[test]
    fn shortcut_session_indexes_visible_list() {
        let mut app = App::new(vec![sess("web-app"), sess("db"), sess("web-api")]);
        app.filter = "web".to_string();
        assert_eq!(app.shortcut_session('1').unwrap().name, "web-api");
    }

    #[test]
    fn set_preview_stores_previews() {
        let mut app = app3();
        let previews = vec![PanePreview {
            title: "0:editor".to_string(),
            content: "nvim".to_string(),
        }];
        app.set_preview(previews.clone());
        assert_eq!(app.preview, previews);
    }
}
