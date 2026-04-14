use ratatui::{
    layout::Margin,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::{
    app::{conversation::ConversationViewState, focus::AppFocus},
    config::Keybindings,
    terminal::{manager::PtyManager, renderer::TerminalWidget},
    ui::{
        hints::footer_line,
        layout::split_root,
        sidebar::{render_sidebar, SidebarVisibleRow},
    },
};

pub fn render(
    frame: &mut Frame,
    focus: AppFocus,
    selected: usize,
    rows: &[SidebarVisibleRow],
    manager: &PtyManager,
    footer_message: Option<&str>,
    keys: &Keybindings,
    show_help: bool,
    show_files: &[String],
    sidebar_width: u16,
    sidebar_collapsed: bool,
    app_focused: bool,
    conversation: &ConversationViewState,
) {
    let layout = split_root(frame.area(), sidebar_width, 1);
    frame.render_widget(
        render_sidebar(
            rows,
            selected,
            focus,
            sidebar_collapsed,
            sidebar_width,
            app_focused,
        ),
        layout.sidebar,
    );
    frame.render_widget(
        Line::from(footer_line(focus, footer_message, keys)),
        layout.footer,
    );

    let main_border_style = if !app_focused {
        Style::default().fg(Color::DarkGray)
    } else if matches!(focus, AppFocus::Terminal | AppFocus::Conversation) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(main_border_style)
        .title(Span::styled(
            match focus {
                AppFocus::Sidebar => " opencode ",
                AppFocus::Terminal => " opencode [live] ",
                AppFocus::Conversation => " opencode [read-only] ",
            },
            main_border_style,
        ));
    frame.render_widget(block, layout.main);

    let inner = layout.main.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    let viewport_height = inner.height as usize;

    if matches!(focus, AppFocus::Conversation) && conversation.is_active() {
        if let Some(error) = conversation.load_error() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!("Error: {error}"),
                    Style::default().fg(Color::Red),
                )),
                inner,
            );
        } else {
            let visible = conversation.visible_lines(viewport_height);
            frame.render_widget(Paragraph::new(visible), inner);
        }
    } else if let Some(pty) = manager.active_session() {
        frame.render_widget(Paragraph::new(""), inner);
        frame.render_widget(TerminalWidget::new(&pty.surface), inner);
        if matches!(focus, AppFocus::Terminal) {
            let (cursor_row, cursor_col) = pty.surface.cursor();
            let x = inner.x + cursor_col as u16;
            let y = inner.y + cursor_row as u16;
            if x < inner.right() && y < inner.bottom() {
                frame.set_cursor_position(ratatui::layout::Position::new(x, y));
            }
        }
    } else if manager.selected_summary().is_some() {
        frame.render_widget(
            Paragraph::new("Press Enter to attach to this session."),
            inner,
        );
    } else {
        frame.render_widget(
            Paragraph::new("No opencode sessions found. Press n to spawn."),
            inner,
        );
    }

    if show_help {
        let help = Paragraph::new(format!(
            "{} focus\n{} / {} move\nEnter attach/open\n{} view\n{} files\n{} spawn\n{} worktree\n{} kill\n{} help\n{} quit",
            "Ctrl-\\",
            keys.down,
            keys.up,
            keys.view,
            keys.files,
            keys.spawn,
            keys.worktree,
            keys.kill,
            keys.help,
            keys.quit,
        ))
        .block(Block::bordered().title("help"));
        let popup = centered_rect(frame.area(), 50, 40);
        frame.render_widget(Clear, popup);
        frame.render_widget(help, popup);
    }

    if !show_files.is_empty() {
        let content = show_files.join("\n");
        let files_widget = Paragraph::new(content)
            .block(Block::bordered().title(format!("files ({})", show_files.len())));
        let popup = centered_rect(frame.area(), 70, 60);
        frame.render_widget(Clear, popup);
        frame.render_widget(files_widget, popup);
    }
}

fn centered_rect(
    area: ratatui::layout::Rect,
    percent_x: u16,
    percent_y: u16,
) -> ratatui::layout::Rect {
    let width = area.width * percent_x / 100;
    let height = area.height * percent_y / 100;
    ratatui::layout::Rect::new(
        area.x + (area.width.saturating_sub(width)) / 2,
        area.y + (area.height.saturating_sub(height)) / 2,
        width.max(1),
        height.max(1),
    )
}
