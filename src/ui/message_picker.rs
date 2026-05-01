use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap},
    Frame,
};

use crate::{
    app::message_picker::MessagePickerState,
    ui::sidebar::relative_time_from_updated,
};

pub fn render_message_picker(frame: &mut Frame, picker: &mut MessagePickerState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " message history ",
            Style::default().fg(Color::Cyan),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [search_area, spacer_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1), // Spacer
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let [table_area, preview_area] = Layout::vertical([
        Constraint::Percentage(55),
        Constraint::Percentage(45),
    ])
    .areas(body_area);

    let search_line = Line::from(vec![
        Span::styled(" Search: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(&picker.query),
        Span::styled("█", Style::default().fg(Color::Cyan)),
    ]);
    frame.render_widget(Paragraph::new(search_line), search_area);
    
    // Render empty spacer
    frame.render_widget(Paragraph::new(""), spacer_area);

    let page_size = table_area.height.saturating_sub(1).max(1) as usize;
    picker.ensure_visible(page_size);

    let visible = picker.visible_entries(page_size);

    let matched_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let selected_style = Style::default().bg(Color::DarkGray);
    let selected_matched_style = Style::default()
        .fg(Color::Yellow)
        .bg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);

    let header = Row::new(vec![
        Cell::from(Span::styled(
            "Session",
            Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan),
        )),
        Cell::from(Span::styled(
            "Message",
            Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan),
        )),
        Cell::from(Span::styled(
            "Time",
            Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan),
        )),
    ]);

    let rows: Vec<Row> = visible
        .iter()
        .enumerate()
        .map(|(i, (entry, title_idx, text_idx))| {
            let is_selected = i + picker.scroll_offset == picker.selected;
            
            // Session Title Styling
            let (title_normal, title_highlight) = if is_selected {
                (Style::default().fg(Color::White).bg(Color::DarkGray).add_modifier(Modifier::BOLD), selected_matched_style)
            } else {
                (Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD), matched_style)
            };

            // Message Preview Styling (dimmer)
            let (text_normal, text_highlight) = if is_selected {
                (Style::default().fg(Color::White).bg(Color::DarkGray), selected_matched_style)
            } else {
                (Style::default().fg(Color::DarkGray), matched_style)
            };
            
            // Time Styling
            let time_style = if is_selected {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let title_cell = Cell::from(highlight_text(&entry.session_title, title_idx, title_normal, title_highlight));
            
            // Truncate text for the table view so it doesn't wrap awkwardly
            let max_text_len = 100; // Arbitrary max length for the preview column
            let display_text = if entry.compact_text.chars().count() > max_text_len {
                let mut truncated: String = entry.compact_text.chars().take(max_text_len).collect();
                truncated.push('…');
                truncated
            } else {
                entry.compact_text.clone()
            };
            
            let text_cell = Cell::from(highlight_text(&display_text, text_idx, text_normal, text_highlight));
            
            let time = relative_time_from_updated(Some(entry.time_created));
            let time_cell = Cell::from(Span::styled(time, time_style));

            let row = Row::new(vec![title_cell, text_cell, time_cell]);
            if is_selected {
                row.style(selected_style)
            } else {
                row
            }
        })
        .collect();

    let widths = [
        Constraint::Length(24),
        Constraint::Min(20),
        Constraint::Length(8),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, table_area);

    let preview_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " preview ",
            Style::default().fg(Color::DarkGray),
        ));

    let preview_inner = preview_block.inner(preview_area);
    frame.render_widget(preview_block, preview_area);

    if let Some(entry) = picker.selected_entry() {
        frame.render_widget(
            Paragraph::new(entry.text).wrap(Wrap { trim: false }),
            preview_inner,
        );
    } else {
        frame.render_widget(
            Paragraph::new(Span::styled("No matching messages.", Style::default().fg(Color::DarkGray))),
            preview_inner,
        );
    }

    let matched = picker.matched_count();
    let total = picker.total_count();
    let footer = Line::from(vec![
        Span::styled(
            format!(" {matched}/{total} "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "↑↓ move · Enter paste · Esc cancel",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(footer), footer_area);
}

fn highlight_text(
    text: &str,
    indices: &[u32],
    normal_style: Style,
    highlight_style: Style,
) -> Line<'static> {
    if indices.is_empty() {
        return Line::from(Span::styled(text.to_string(), normal_style));
    }

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut in_highlight = false;

    for (i, ch) in text.chars().enumerate() {
        let is_match = indices.contains(&(i as u32));
        if is_match != in_highlight {
            if !current.is_empty() {
                let style = if in_highlight {
                    highlight_style
                } else {
                    normal_style
                };
                spans.push(Span::styled(std::mem::take(&mut current), style));
            }
            in_highlight = is_match;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        let style = if in_highlight {
            highlight_style
        } else {
            normal_style
        };
        spans.push(Span::styled(current, style));
    }

    Line::from(spans)
}
