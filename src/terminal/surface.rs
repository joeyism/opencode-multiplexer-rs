use alacritty_terminal::{
    event::VoidListener,
    term::{cell::Flags, test::TermSize, Config, Term},
    vte::ansi::Processor,
};
use ratatui::style::Color;

use crate::terminal::color::{convert_ansi_color, style_from_flags};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCell {
    pub symbol: String,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

impl Default for RenderCell {
    fn default() -> Self {
        Self {
            symbol: " ".into(),
            fg: Color::Reset,
            bg: Color::Reset,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
        }
    }
}

impl RenderCell {
    pub fn style(&self) -> ratatui::style::Style {
        style_from_flags(
            self.fg,
            self.bg,
            self.bold,
            self.italic,
            self.underline,
            self.strike,
        )
    }
}

pub struct TerminalSurface {
    term: Term<VoidListener>,
    parser: Processor,
    rows: usize,
    cols: usize,
}

impl TerminalSurface {
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let size = TermSize::new(cols, rows);
        let term = Term::new(Config::default(), &size, VoidListener);
        Self {
            term,
            parser: Processor::new(),
            rows,
            cols,
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        self.rows = rows;
        self.cols = cols;
        self.term.resize(TermSize::new(cols, rows));
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn snapshot(&self) -> Vec<Vec<RenderCell>> {
        let mut output = vec![vec![RenderCell::default(); self.cols]; self.rows];

        for indexed in self.term.grid().display_iter() {
            let point = indexed.point;
            if point.line.0 < 0 {
                continue;
            }

            let row = point.line.0 as usize;
            let col = point.column.0;
            if row >= self.rows || col >= self.cols {
                continue;
            }

            let cell = indexed.cell;
            output[row][col] = RenderCell {
                symbol: cell_symbol(cell.c, cell.flags),
                fg: convert_ansi_color(cell.fg),
                bg: convert_ansi_color(cell.bg),
                bold: cell.flags.contains(Flags::BOLD) || cell.flags.contains(Flags::DIM_BOLD),
                italic: cell.flags.contains(Flags::ITALIC),
                underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
                strike: cell.flags.contains(Flags::STRIKEOUT),
            };
        }

        output
    }

    pub fn cursor(&self) -> (usize, usize) {
        let cursor = self.term.grid().cursor.point;
        let row = (cursor.line.0.max(0) as usize).min(self.rows.saturating_sub(1));
        let col = cursor.column.0.min(self.cols.saturating_sub(1));
        (row, col)
    }
}

fn cell_symbol(ch: char, flags: Flags) -> String {
    if flags.contains(Flags::WIDE_CHAR_SPACER) || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
        " ".into()
    } else {
        ch.to_string()
    }
}
