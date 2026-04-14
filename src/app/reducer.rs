use crate::app::{focus::AppFocus, state::AppState, Action};

pub fn reduce(state: &mut AppState, action: Action) {
    match action {
        Action::ToggleFocus => {
            state.focus = match state.focus {
                AppFocus::Sidebar => state.last_main_focus,
                AppFocus::Terminal | AppFocus::Conversation => {
                    if state.focus == AppFocus::Conversation {
                        state.last_main_focus = AppFocus::Terminal;
                    }
                    AppFocus::Sidebar
                }
            };
        }
        Action::SetFocus(target) => {
            match target {
                AppFocus::Terminal | AppFocus::Conversation => {
                    state.last_main_focus = target;
                }
                AppFocus::Sidebar => {}
            }
            state.focus = target;
        }
        Action::ToggleHelp => {
            state.show_help = !state.show_help;
        }
        Action::ToggleSidebarCollapse => {
            state.sidebar_collapsed = !state.sidebar_collapsed;
        }
        Action::SelectNextRow => {
            state.selected_sidebar_row = state.selected_sidebar_row.saturating_add(1);
        }
        Action::SelectPrevRow => {
            state.selected_sidebar_row = state.selected_sidebar_row.saturating_sub(1);
        }
        Action::ToggleExpandSelected(session_id) => {
            if !state.expanded_session_ids.remove(&session_id) {
                state.expanded_session_ids.insert(session_id);
            }
        }
        Action::SetSelectedRow(row) => {
            state.selected_sidebar_row = row;
        }
    }
}
