use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::agents::{AgentGraph, AgentNode, AgentStatus};

pub fn render_agents(frame: &mut Frame, area: Rect, graph: Option<&AgentGraph>, selected: usize) {
    let Some(graph) = graph else {
        frame.render_widget(Paragraph::new("No agent graph available."), area);
        return;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(4)])
        .split(area);
    let lines = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| render_node(node, index == selected, area.width))
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::TOP)
                .title(graph.header.clone()),
        ),
        chunks[0],
    );
    let detail = graph
        .nodes
        .get(selected)
        .map(detail_lines)
        .unwrap_or_else(|| vec![Line::from("No node selected")]);
    frame.render_widget(
        Paragraph::new(detail).block(Block::default().borders(Borders::TOP).title("selected")),
        chunks[1],
    );
}

fn render_node(node: &AgentNode, selected: bool, width: u16) -> Line<'static> {
    let prefix = format!("{}{} ", "  ".repeat(node.depth), node.status.glyph());
    let title_limit = (width as usize).saturating_sub(prefix.len() + 12);
    let title = truncate(&node.title, title_limit);
    let status = format_status(node.status);
    let style = if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color(node.status))
    };
    Line::from(vec![Span::styled(
        format!("{prefix}{title:<width$} {status}", width = title_limit),
        style,
    )])
}

fn detail_lines(node: &AgentNode) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!(
        "{} · {}",
        node.title,
        format_status(node.status)
    ))];
    if !node.depends_on.is_empty() {
        lines.push(Line::from(format!("deps: {}", node.depends_on.join(", "))));
    }
    if let Some(session_id) = &node.session_id {
        lines.push(Line::from(format!(
            "session: {session_id}   [Enter] attach  [v] convo  [d] diff"
        )));
    } else {
        lines.push(Line::from(
            "no session assigned   [j/k] move  [a/q/Esc] close",
        ));
    }
    if let Some(detail) = &node.detail {
        lines.push(Line::from(truncate(detail, 120)));
    }
    lines
}

fn format_status(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Working => "working",
        AgentStatus::SubagentsWorking => "subagents",
        AgentStatus::NeedsInput => "needs input",
        AgentStatus::Error => "error",
        AgentStatus::Succeeded => "succeeded",
        AgentStatus::Pending => "pending",
        AgentStatus::Idle => "idle",
        AgentStatus::Unknown => "unknown",
        AgentStatus::Cancelled => "cancelled",
    }
}

fn status_color(status: AgentStatus) -> Color {
    match status {
        AgentStatus::Working => Color::Green,
        AgentStatus::SubagentsWorking => Color::Cyan,
        AgentStatus::NeedsInput => Color::Yellow,
        AgentStatus::Error => Color::Red,
        AgentStatus::Succeeded => Color::DarkGray,
        AgentStatus::Pending
        | AgentStatus::Idle
        | AgentStatus::Unknown
        | AgentStatus::Cancelled => Color::Gray,
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    value
        .chars()
        .take(max.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_glyphs_distinguish_live_and_pending_nodes() {
        assert_eq!(AgentStatus::Working.glyph(), "●");
        assert_eq!(AgentStatus::Pending.glyph(), "○");
        assert_eq!(format_status(AgentStatus::Succeeded), "succeeded");
    }
}
