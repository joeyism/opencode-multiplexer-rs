use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

use crate::{app::focus::AppFocus, config::Keybindings};

pub fn footer_line(focus: AppFocus, message: Option<&str>, keys: &Keybindings) -> Line<'static> {
    if let Some(message) = message {
        return Line::from(vec![Span::styled(
            message.to_string(),
            Style::default().fg(Color::Yellow),
        )]);
    }

    let focus_label = match focus {
        AppFocus::Sidebar => "sidebar",
        AppFocus::Terminal => "terminal",
    };

    Line::from(vec![
        Span::styled("Ctrl-\\", Style::default().fg(Color::Cyan)),
        Span::raw(" focus  "),
        Span::styled(keys.quit.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(" quit  "),
        Span::styled(keys.spawn.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(" new  "),
        Span::styled(keys.worktree.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(" worktree  "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(" refresh  "),
        Span::styled("!", Style::default().fg(Color::Cyan)),
        Span::raw(" bash  "),
        Span::styled("/", Style::default().fg(Color::Cyan)),
        Span::raw(" search  "),
        Span::styled(keys.kill.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(" kill  "),
        Span::styled(
            format!("{}/{}", keys.down, keys.up),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" move  "),
        Span::styled(keys.help.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(" help  "),
        Span::styled(
            format!("[{focus_label}]"),
            Style::default().fg(Color::Yellow),
        ),
    ])
}
