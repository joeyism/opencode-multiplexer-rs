use crate::app::focus::AppFocus;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub focus: AppFocus,
    pub show_help: bool,
    pub sidebar_collapsed: bool,
    pub selected_sidebar_row: usize,
    pub expanded_session_ids: HashSet<String>,
    pub app_focused: bool,
    pub last_main_focus: AppFocus,
    pub show_files: Vec<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            focus: AppFocus::default(),
            show_help: false,
            sidebar_collapsed: false,
            selected_sidebar_row: 0,
            expanded_session_ids: HashSet::new(),
            app_focused: true,
            last_main_focus: AppFocus::Terminal,
            show_files: Vec::new(),
        }
    }
}
