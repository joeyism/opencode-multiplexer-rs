use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor, Rgb};
use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

pub fn ansi_named_color(index: u8) -> Color {
    match index {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        _ => Color::White,
    }
}

pub fn convert_color(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

pub fn convert_ansi_color(color: AnsiColor) -> Color {
    match color {
        AnsiColor::Named(named) => map_named_color(named),
        AnsiColor::Spec(Rgb { r, g, b }) => convert_color(r, g, b),
        AnsiColor::Indexed(idx) => Color::Indexed(idx),
    }
}

fn map_named_color(color: NamedColor) -> Color {
    match color {
        NamedColor::Black | NamedColor::DimBlack => ansi_named_color(0),
        NamedColor::Red | NamedColor::DimRed => ansi_named_color(1),
        NamedColor::Green | NamedColor::DimGreen => ansi_named_color(2),
        NamedColor::Yellow | NamedColor::DimYellow => ansi_named_color(3),
        NamedColor::Blue | NamedColor::DimBlue => ansi_named_color(4),
        NamedColor::Magenta | NamedColor::DimMagenta => ansi_named_color(5),
        NamedColor::Cyan | NamedColor::DimCyan => ansi_named_color(6),
        NamedColor::White
        | NamedColor::DimWhite
        | NamedColor::Foreground
        | NamedColor::DimForeground => ansi_named_color(7),
        NamedColor::BrightBlack => ansi_named_color(8),
        NamedColor::BrightRed => ansi_named_color(9),
        NamedColor::BrightGreen => ansi_named_color(10),
        NamedColor::BrightYellow => ansi_named_color(11),
        NamedColor::BrightBlue => ansi_named_color(12),
        NamedColor::BrightMagenta => ansi_named_color(13),
        NamedColor::BrightCyan => ansi_named_color(14),
        NamedColor::BrightWhite | NamedColor::BrightForeground => ansi_named_color(15),
        NamedColor::Background => Color::Black,
        NamedColor::Cursor => Color::White,
    }
}

pub fn style_from_flags(
    fg: Color,
    bg: Color,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
) -> Style {
    let mut style = Style::default().fg(fg).bg(bg);
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if strike {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    style
}

pub fn styled_symbol(symbol: String, style: Style) -> Span<'static> {
    Span::styled(symbol, style)
}
