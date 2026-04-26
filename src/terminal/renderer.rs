use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

use crate::terminal::surface::TerminalSurface;

pub struct TerminalWidget<'a> {
    surface: &'a TerminalSurface,
}

impl<'a> TerminalWidget<'a> {
    pub fn new(surface: &'a TerminalSurface) -> Self {
        Self { surface }
    }
}

impl Widget for TerminalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let snapshot = self.surface.snapshot();
        let max_rows = area.height as usize;
        let max_cols = area.width as usize;

        for (row, row_data) in snapshot.iter().enumerate().take(max_rows) {
            for (col, cell) in row_data.iter().enumerate().take(max_cols) {
                let x = area.x + col as u16;
                let y = area.y + row as u16;
                buf[(x, y)].set_symbol(&cell.symbol).set_style(cell.style());
            }
        }
    }
}
