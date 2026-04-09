use ratatui::layout::{Constraint, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootLayout {
    pub sidebar: Rect,
    pub main: Rect,
    pub footer: Rect,
}

pub fn split_root(area: Rect, sidebar_width: u16, footer_height: u16) -> RootLayout {
    let [body, footer] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(footer_height)]).areas(area);

    let [sidebar, main] = Layout::horizontal([
        Constraint::Length(sidebar_width.min(body.width)),
        Constraint::Min(0),
    ])
    .areas(body);

    RootLayout {
        sidebar,
        main,
        footer,
    }
}
