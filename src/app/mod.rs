pub mod conversation;
pub mod focus;
pub mod reducer;
pub mod sessions;
pub mod state;

pub use focus::AppFocus;
pub use reducer::reduce;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ToggleFocus,
    SetFocus(AppFocus),
    ToggleHelp,
    ToggleSidebarCollapse,
    SelectNextRow,
    SelectPrevRow,
    ToggleExpandSelected(String),
    SetSelectedRow(usize),
}
