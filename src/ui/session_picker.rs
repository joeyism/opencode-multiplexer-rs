use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::{app::session_picker::SessionPickerState, ui::sidebar::relative_time_from_updated};

pub fn render_session_picker(frame: &mut Frame, picker: &mut SessionPickerState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " attach session ",
            Style::default().fg(Color::Cyan),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Vertical layout: search input (1 line) + gap (1 line) + table + footer (1 line)
    let [search_area, table_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Render search input
    let search_line = Line::from(vec![
        Span::styled(" Search: ", Style::default().fg(Color::DarkGray)),
        Span::raw(&picker.query),
        Span::styled("█", Style::default().fg(Color::Cyan)),
    ]);
    frame.render_widget(Paragraph::new(search_line), search_area);

    // Compute page size and ensure selection is visible
    let page_size = table_area.height.saturating_sub(1) as usize; // subtract 1 for header
    picker.ensure_visible(page_size);

    // Get visible entries with match indices
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
            "Repo",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )),
        Cell::from(Span::styled(
            "Title",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )),
        Cell::from(Span::styled(
            "Directory",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )),
        Cell::from(Span::styled(
            "Time",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        )),
    ]);

    let rows: Vec<Row> = visible
        .iter()
        .enumerate()
        .map(|(i, (entry, repo_idx, title_idx, dir_idx))| {
            let is_selected = i + picker.scroll_offset == picker.selected;
            let (normal, highlight) = if is_selected {
                (selected_style, selected_matched_style)
            } else {
                (Style::default(), matched_style)
            };

            let repo_cell = Cell::from(highlight_text(&entry.repo, repo_idx, normal, highlight));
            let title_cell = Cell::from(highlight_text(&entry.title, title_idx, normal, highlight));
            let dir_cell = Cell::from(highlight_text(&entry.directory, dir_idx, normal, highlight));
            let time = relative_time_from_updated(Some(entry.time_updated));
            let time_cell = Cell::from(Span::styled(time, normal));

            let row = Row::new(vec![repo_cell, title_cell, dir_cell, time_cell]);
            if is_selected {
                row.style(selected_style)
            } else {
                row
            }
        })
        .collect();

    let widths = [
        Constraint::Length(18),
        Constraint::Min(20),
        Constraint::Min(24),
        Constraint::Length(8),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, table_area);

    // Render footer hints
    let matched = picker.matched_count();
    let total = picker.total_count();
    let footer = Line::from(vec![
        Span::styled(
            format!(" {matched}/{total} "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "↑↓ move · Enter attach · Esc cancel",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(footer), footer_area);
}

/// Build a Line with matched character indices highlighted.
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
