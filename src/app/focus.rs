#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppFocus {
    #[default]
    Sidebar,
    Terminal,
    Conversation,
}
