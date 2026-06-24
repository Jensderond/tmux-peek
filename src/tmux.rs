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
