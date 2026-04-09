use crate::app::focus::AppFocus;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppState {
    pub focus: AppFocus,
    pub show_help: bool,
    pub sidebar_collapsed: bool,
    pub selected_sidebar_row: usize,
    pub expanded_session_ids: HashSet<String>,
}
