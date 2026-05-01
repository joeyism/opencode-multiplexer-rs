#![allow(clippy::if_same_then_else)]
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::diff::LineMeta;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn build_diff_document(
    diff_output: &str,
    width: u16,
) -> (Vec<Line<'static>>, Vec<Option<LineMeta>>) {
    if diff_output.trim().is_empty() {
        return (
            vec![Line::from(Span::styled(
                "No uncommitted changes.",
                Style::default().fg(Color::DarkGray),
            ))],
            vec![None],
        );
    }

    let parsed = parse_unified_diff(diff_output);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut meta: Vec<Option<LineMeta>> = Vec::new();

    // Helper closure to keep lines and meta in sync.
    let mut push = |line: Line<'static>, m: Option<LineMeta>| {
        lines.push(line);
        meta.push(m);
    };

    let divider_w: u16 = 1;
    let left_width = (width.saturating_sub(divider_w)) / 2;
    let right_width = width.saturating_sub(divider_w).saturating_sub(left_width);

    for (file_idx, file) in parsed.files.iter().enumerate() {
        if file_idx > 0 {
            push(Line::from(Span::raw("")), None);
        }

        // File header
        push(render_file_header(file, width), None);

        // Metadata lines (index, new file mode, etc.)
        for file_meta in &file.meta {
            push(
                Line::from(Span::styled(
                    truncate_pad(file_meta, width as usize),
                    Style::default().fg(Color::DarkGray),
                )),
                None,
            );
        }

        // Compute line-number gutter width for this file
        let line_no_width = compute_line_no_width(file);

        for hunk in &file.hunks {
            push(render_hunk_header(&hunk.header, width), None);

            for row in &hunk.rows {
                match row {
                    DiffRow::Pair { left, right } => {
                        let line_meta = LineMeta {
                            filepath: file.new_path.clone(),
                            new_line_no: right.as_ref().map(|c| c.line_no),
                            old_line_no: left.as_ref().map(|c| c.line_no),
                        };
                        push(
                            render_pair_row(
                                left.as_ref(),
                                right.as_ref(),
                                left_width,
                                right_width,
                                line_no_width,
                            ),
                            Some(line_meta),
                        );
                    }
                    DiffRow::Note(text) => {
                        push(
                            Line::from(Span::styled(
                                truncate_pad(text, width as usize),
                                Style::default().fg(Color::DarkGray),
                            )),
                            None,
                        );
                    }
                }
            }
        }
    }

    (lines, meta)
}

/// Apply search-match highlighting to visible lines.
///
/// `lines` are the visible slice from `visible_lines()`. `scroll_offset` is the
/// document row index of the first visible line. `matches` contains all match
/// positions as `(line_idx, byte_start, byte_len)` in absolute document
/// coordinates. `current_match_idx` is the index into `matches` of the focused
/// match (rendered with a distinct colour).
pub fn highlight_search_matches(
    lines: &[Line<'static>],
    scroll_offset: usize,
    matches: &[(usize, usize, usize)],
    current_match_idx: usize,
) -> Vec<Line<'static>> {
    let highlight_style = Style::default().fg(Color::Black).bg(Color::Yellow);
    let current_style = Style::default()
        .fg(Color::Black)
        .bg(Color::Rgb(255, 150, 0));

    lines
        .iter()
        .enumerate()
        .map(|(vis_idx, line)| {
            let abs_idx = scroll_offset + vis_idx;

            // Collect matches for this line.
            let line_matches: Vec<(usize, usize, bool)> = matches
                .iter()
                .enumerate()
                .filter(|(_, (li, _, _))| *li == abs_idx)
                .map(|(mi, (_, start, len))| (*start, *len, mi == current_match_idx))
                .collect();

            if line_matches.is_empty() {
                return line.clone();
            }

            // Flatten all spans into (byte_offset, byte_len, style) segments.
            let mut segments: Vec<(String, Style)> = Vec::new();
            for span in &line.spans {
                segments.push((span.content.to_string(), span.style));
            }

            // Now split segments at match boundaries.
            let flat: String = segments.iter().map(|(t, _)| t.as_str()).collect();

            // Build a style-override map: for each byte position in the flat
            // string, optionally store the highlight style to apply.
            let mut overrides: Vec<Option<Style>> = vec![None; flat.len()];
            for &(start, len, is_current) in &line_matches {
                let style = if is_current {
                    current_style
                } else {
                    highlight_style
                };
                let end = (start + len).min(flat.len());
                for slot in &mut overrides[start..end] {
                    *slot = Some(style);
                }
            }

            // Walk the original segments and split at override boundaries.
            let mut result_spans: Vec<Span<'static>> = Vec::new();
            let mut byte_pos: usize = 0;

            for (text, base_style) in &segments {
                let seg_start = byte_pos;
                let seg_end = byte_pos + text.len();
                let mut cursor = seg_start;

                while cursor < seg_end {
                    let current_override = overrides.get(cursor).copied().flatten();
                    // Find the run length of the same override state.
                    let run_end = (cursor + 1..seg_end)
                        .find(|&i| overrides.get(i).copied().flatten() != current_override)
                        .unwrap_or(seg_end);

                    let slice = &text[(cursor - seg_start)..(run_end - seg_start)];
                    let style = current_override.unwrap_or(*base_style);
                    result_spans.push(Span::styled(slice.to_string(), style));
                    cursor = run_end;
                }

                byte_pos = seg_end;
            }

            Line::from(result_spans)
        })
        .collect()
}

/// Apply cursor and visual-selection highlighting to visible lines.
///
/// `lines` are the visible slice. `scroll_offset` is the document row index of
/// the first visible line. `cursor` is the absolute cursor row. `selection_range`
/// is an inclusive `(start, end)` range when visual mode is active.
pub fn apply_cursor_and_selection(
    lines: Vec<Line<'static>>,
    scroll_offset: usize,
    cursor: usize,
    selection_range: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    let cursor_style = Style::default().bg(Color::Rgb(60, 60, 100));
    let selection_style = Style::default().bg(Color::Rgb(50, 50, 80));

    lines
        .into_iter()
        .enumerate()
        .map(|(vis_idx, line)| {
            let abs_idx = scroll_offset + vis_idx;
            let is_cursor = abs_idx == cursor;
            let is_selected = selection_range
                .map(|(lo, hi)| abs_idx >= lo && abs_idx <= hi)
                .unwrap_or(false);

            if !is_cursor && !is_selected {
                return line;
            }

            let highlight = if is_cursor {
                cursor_style
            } else {
                selection_style
            };

            let new_spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|span| {
                    Span::styled(
                        span.content.as_ref().to_string(),
                        highlight.patch(span.style),
                    )
                })
                .collect();

            Line::from(new_spans)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Parser types
// ---------------------------------------------------------------------------

struct ParsedDiff {
    files: Vec<DiffFile>,
}

struct DiffFile {
    old_path: String,
    new_path: String,
    meta: Vec<String>,
    hunks: Vec<DiffHunk>,
}

struct DiffHunk {
    header: String,
    rows: Vec<DiffRow>,
}

enum DiffRow {
    Pair {
        left: Option<SideCell>,
        right: Option<SideCell>,
    },
    Note(String),
}

struct SideCell {
    line_no: usize,
    text: String,
    kind: CellKind,
}

#[derive(Clone, Copy)]
enum CellKind {
    Context,
    Removed,
    Added,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

fn parse_unified_diff(input: &str) -> ParsedDiff {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk: Option<DiffHunk> = None;
    let mut removed_buf: Vec<(usize, String)> = Vec::new();
    let mut added_buf: Vec<(usize, String)> = Vec::new();
    let mut old_line: usize = 0;
    let mut new_line: usize = 0;

    let flush_change_block = |removed: &mut Vec<(usize, String)>,
                              added: &mut Vec<(usize, String)>,
                              hunk: &mut Option<DiffHunk>| {
        let Some(h) = hunk.as_mut() else { return };
        let max_len = removed.len().max(added.len());
        for i in 0..max_len {
            let left = removed.get(i).map(|(ln, text)| SideCell {
                line_no: *ln,
                text: text.clone(),
                kind: CellKind::Removed,
            });
            let right = added.get(i).map(|(ln, text)| SideCell {
                line_no: *ln,
                text: text.clone(),
                kind: CellKind::Added,
            });
            h.rows.push(DiffRow::Pair { left, right });
        }
        removed.clear();
        added.clear();
    };

    for raw_line in input.lines() {
        if raw_line.starts_with("diff --git ") {
            // Flush any pending change block
            flush_change_block(&mut removed_buf, &mut added_buf, &mut current_hunk);
            // Flush current hunk into current file
            if let (Some(ref mut file), Some(hunk)) = (current_file.as_mut(), current_hunk.take()) {
                file.hunks.push(hunk);
            }
            // Flush current file
            if let Some(file) = current_file.take() {
                files.push(file);
            }
            current_file = Some(DiffFile {
                old_path: String::new(),
                new_path: String::new(),
                meta: Vec::new(),
                hunks: Vec::new(),
            });
        } else if let Some(stripped) = raw_line.strip_prefix("--- ") {
            if let Some(ref mut file) = current_file {
                file.old_path = strip_git_prefix(stripped).to_string();
            }
        } else if let Some(stripped) = raw_line.strip_prefix("+++ ") {
            if let Some(ref mut file) = current_file {
                file.new_path = strip_git_prefix(stripped).to_string();
            }
        } else if raw_line.starts_with("@@ ") {
            flush_change_block(&mut removed_buf, &mut added_buf, &mut current_hunk);
            if let (Some(ref mut file), Some(hunk)) = (current_file.as_mut(), current_hunk.take()) {
                file.hunks.push(hunk);
            }
            let (os, ns) = parse_hunk_header(raw_line);
            old_line = os;
            new_line = ns;
            current_hunk = Some(DiffHunk {
                header: raw_line.to_string(),
                rows: Vec::new(),
            });
        } else if let Some(ref mut _file) = current_file {
            if current_hunk.is_some() {
                if let Some(rest) = raw_line.strip_prefix(' ') {
                    flush_change_block(&mut removed_buf, &mut added_buf, &mut current_hunk);
                    if let Some(ref mut h) = current_hunk {
                        h.rows.push(DiffRow::Pair {
                            left: Some(SideCell {
                                line_no: old_line,
                                text: rest.to_string(),
                                kind: CellKind::Context,
                            }),
                            right: Some(SideCell {
                                line_no: new_line,
                                text: rest.to_string(),
                                kind: CellKind::Context,
                            }),
                        });
                    }
                    old_line += 1;
                    new_line += 1;
                } else if let Some(rest) = raw_line.strip_prefix('-') {
                    removed_buf.push((old_line, rest.to_string()));
                    old_line += 1;
                } else if let Some(rest) = raw_line.strip_prefix('+') {
                    added_buf.push((new_line, rest.to_string()));
                    new_line += 1;
                } else if raw_line.starts_with('\\') {
                    flush_change_block(&mut removed_buf, &mut added_buf, &mut current_hunk);
                    if let Some(ref mut h) = current_hunk {
                        h.rows.push(DiffRow::Note(raw_line.to_string()));
                    }
                }
            } else {
                // Metadata lines before first hunk (index, new file mode, etc.)
                if let Some(ref mut file) = current_file {
                    file.meta.push(raw_line.to_string());
                }
            }
        }
    }

    // Final flush
    flush_change_block(&mut removed_buf, &mut added_buf, &mut current_hunk);
    if let (Some(ref mut file), Some(hunk)) = (current_file.as_mut(), current_hunk.take()) {
        file.hunks.push(hunk);
    }
    if let Some(file) = current_file.take() {
        files.push(file);
    }

    ParsedDiff { files }
}

fn parse_hunk_header(header: &str) -> (usize, usize) {
    // "@@ -OLD_START[,OLD_COUNT] +NEW_START[,NEW_COUNT] @@..."
    let stripped = header.trim_start_matches("@@ ");
    let parts: Vec<&str> = stripped.splitn(3, ' ').collect();

    let old_start = parts
        .first()
        .and_then(|s| s.strip_prefix('-'))
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);

    let new_start = parts
        .get(1)
        .and_then(|s| s.strip_prefix('+'))
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);

    (old_start, new_start)
}

fn strip_git_prefix(path: &str) -> &str {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn compute_line_no_width(file: &DiffFile) -> usize {
    let mut max_line: usize = 1;
    for hunk in &file.hunks {
        for row in &hunk.rows {
            if let DiffRow::Pair { left, right } = row {
                if let Some(cell) = left {
                    max_line = max_line.max(cell.line_no);
                }
                if let Some(cell) = right {
                    max_line = max_line.max(cell.line_no);
                }
            }
        }
    }
    digit_count(max_line)
}

fn digit_count(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    ((n as f64).log10().floor() as usize) + 1
}

fn render_file_header(file: &DiffFile, width: u16) -> Line<'static> {
    let label = if file.new_path.is_empty() && file.old_path.is_empty() {
        String::from("(unknown file)")
    } else if file.old_path == file.new_path || file.old_path.is_empty() {
        file.new_path.clone()
    } else if file.new_path.is_empty() || file.new_path == "/dev/null" {
        format!("{} (deleted)", file.old_path)
    } else {
        format!("{} -> {}", file.old_path, file.new_path)
    };

    let padded = format!(
        " {:<width$}",
        label,
        width = (width as usize).saturating_sub(1)
    );
    let display = truncate_exact(&padded, width as usize);

    Line::from(Span::styled(
        display,
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    ))
}

fn render_hunk_header(header: &str, width: u16) -> Line<'static> {
    Line::from(Span::styled(
        truncate_pad(header, width as usize),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::DIM),
    ))
}

fn render_pair_row(
    left: Option<&SideCell>,
    right: Option<&SideCell>,
    left_width: u16,
    right_width: u16,
    line_no_width: usize,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Left side
    render_side_cell(left, left_width as usize, line_no_width, &mut spans, true);

    // Center divider
    spans.push(Span::styled(
        "\u{2502}",
        Style::default().fg(Color::DarkGray),
    ));

    // Right side
    render_side_cell(
        right,
        right_width as usize,
        line_no_width,
        &mut spans,
        false,
    );

    Line::from(spans)
}

fn render_side_cell(
    cell: Option<&SideCell>,
    side_width: usize,
    line_no_width: usize,
    spans: &mut Vec<Span<'static>>,
    is_left: bool,
) {
    // Layout: "{line_no:>W} {text...}"
    // gutter = line_no_width + 1 (space)
    let gutter = line_no_width + 1;
    let text_width = side_width.saturating_sub(gutter);

    match cell {
        Some(cell) => {
            // Line number
            spans.push(Span::styled(
                format!("{:>width$}", cell.line_no, width = line_no_width),
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::raw(" "));

            // Text content
            let style = match cell.kind {
                CellKind::Context => Style::default(),
                CellKind::Removed => Style::default().fg(Color::Red),
                CellKind::Added => Style::default().fg(Color::Green),
            };
            spans.push(Span::styled(truncate_pad(&cell.text, text_width), style));
        }
        None => {
            // Empty cell — fill with dim background
            let fill = if is_left {
                format!(
                    "{:>width$} {:<tw$}",
                    "~",
                    "",
                    width = line_no_width,
                    tw = text_width
                )
            } else {
                format!(
                    "{:>width$} {:<tw$}",
                    "~",
                    "",
                    width = line_no_width,
                    tw = text_width
                )
            };
            spans.push(Span::styled(fill, Style::default().fg(Color::DarkGray)));
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate_pad(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        format!("{text:<width$}")
    } else if width == 0 {
        String::new()
    } else {
        chars[..width].iter().collect()
    }
}

fn truncate_exact(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        format!("{text:<width$}")
    } else {
        chars[..width].iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_diff_produces_placeholder() {
        let (doc, meta) = build_diff_document("", 80);
        assert_eq!(doc.len(), 1);
        assert_eq!(meta.len(), 1);
        assert!(meta[0].is_none());
    }

    #[test]
    fn single_file_single_hunk() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
index abc..def 100644
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
 line1
-old
+new
 line3
";
        let parsed = parse_unified_diff(diff);
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.files[0].hunks.len(), 1);
        // 3 context/change rows: context, pair(old/new), context
        assert_eq!(parsed.files[0].hunks[0].rows.len(), 3);
    }

    #[test]
    fn additions_only() {
        let diff = "\
diff --git a/bar.rs b/bar.rs
--- a/bar.rs
+++ b/bar.rs
@@ -1,2 +1,4 @@
 line1
+added1
+added2
 line2
";
        let parsed = parse_unified_diff(diff);
        let rows = &parsed.files[0].hunks[0].rows;
        // line1 (context), added1+added2 as pair with None left, line2 (context)
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn deletions_only() {
        let diff = "\
diff --git a/baz.rs b/baz.rs
--- a/baz.rs
+++ b/baz.rs
@@ -1,4 +1,2 @@
 line1
-removed1
-removed2
 line2
";
        let parsed = parse_unified_diff(diff);
        let rows = &parsed.files[0].hunks[0].rows;
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn multiple_files() {
        let diff = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,2 @@
-old_a
+new_a
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1,2 +1,2 @@
-old_b
+new_b
";
        let parsed = parse_unified_diff(diff);
        assert_eq!(parsed.files.len(), 2);
    }

    #[test]
    fn hunk_header_parsing() {
        assert_eq!(parse_hunk_header("@@ -12,3 +15,4 @@ fn foo"), (12, 15));
        assert_eq!(parse_hunk_header("@@ -7 +7 @@"), (7, 7));
        assert_eq!(parse_hunk_header("@@ -1,100 +1,102 @@"), (1, 1));
    }

    #[test]
    fn no_newline_note() {
        let diff = "\
diff --git a/c.rs b/c.rs
--- a/c.rs
+++ b/c.rs
@@ -1,2 +1,2 @@
-old
+new
\\ No newline at end of file
";
        let parsed = parse_unified_diff(diff);
        let rows = &parsed.files[0].hunks[0].rows;
        // pair(old/new), then note
        assert_eq!(rows.len(), 2);
        assert!(matches!(rows[1], DiffRow::Note(_)));
    }

    #[test]
    fn build_document_width_sanity() {
        let diff = "\
diff --git a/foo.rs b/foo.rs
--- a/foo.rs
+++ b/foo.rs
@@ -1,3 +1,3 @@
 line1
-old
+new
 line3
";
        let (doc, _meta) = build_diff_document(diff, 80);
        // file header + metadata + hunk header + 3 rows = at least 5 lines
        assert!(doc.len() >= 5);
    }

    #[test]
    fn strip_prefix_works() {
        assert_eq!(strip_git_prefix("a/src/main.rs"), "src/main.rs");
        assert_eq!(strip_git_prefix("b/src/main.rs"), "src/main.rs");
        assert_eq!(strip_git_prefix("/dev/null"), "/dev/null");
        assert_eq!(strip_git_prefix("plain.txt"), "plain.txt");
    }

    #[test]
    fn digit_count_works() {
        assert_eq!(digit_count(0), 1);
        assert_eq!(digit_count(1), 1);
        assert_eq!(digit_count(9), 1);
        assert_eq!(digit_count(10), 2);
        assert_eq!(digit_count(99), 2);
        assert_eq!(digit_count(100), 3);
        assert_eq!(digit_count(999), 3);
        assert_eq!(digit_count(1000), 4);
    }

    #[test]
    fn new_file_from_dev_null_parses() {
        // This is the format produced by `git diff --no-index /dev/null <file>`.
        let diff = "\
diff --git a/brand_new.rs b/brand_new.rs
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/brand_new.rs
@@ -0,0 +1,3 @@
+fn main() {
+    println!(\"hello\");
+}
";
        let parsed = parse_unified_diff(diff);
        assert_eq!(parsed.files.len(), 1);
        let file = &parsed.files[0];
        assert_eq!(file.old_path, "/dev/null");
        assert_eq!(file.new_path, "brand_new.rs");
        assert!(file.meta.iter().any(|m| m.contains("new file mode")));
        assert_eq!(file.hunks.len(), 1);
        assert_eq!(file.hunks[0].rows.len(), 3);
    }

    #[test]
    fn new_file_document_renders() {
        let diff = "\
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+line one
+line two
";
        let (doc, _meta) = build_diff_document(diff, 80);
        // Should have at least: file header, meta, hunk header, 2 rows.
        assert!(
            doc.len() >= 5,
            "expected at least 5 lines, got {}",
            doc.len()
        );
    }

    // -----------------------------------------------------------------------
    // Search highlight tests
    // -----------------------------------------------------------------------

    #[test]
    fn highlight_splits_spans_correctly() {
        // One span: "hello world foo", match on "world" (bytes 6..11).
        let lines = vec![Line::from(Span::raw("hello world foo".to_string()))];
        let matches = vec![(0usize, 6usize, 5usize)]; // line 0, byte 6, len 5
        let result = highlight_search_matches(&lines, 0, &matches, 0);

        assert_eq!(result.len(), 1);
        let spans = &result[0].spans;
        assert_eq!(spans.len(), 3, "expected 3 spans: before, match, after");
        assert_eq!(spans[0].content.as_ref(), "hello ");
        assert_eq!(spans[1].content.as_ref(), "world");
        assert_eq!(spans[2].content.as_ref(), " foo");
        // The match span should have a highlight bg.
        assert_ne!(spans[1].style, Style::default());
    }

    #[test]
    fn highlight_current_match_uses_different_style() {
        // Two matches on the same line.
        let lines = vec![Line::from(Span::raw("aa bb aa".to_string()))];
        let matches = vec![(0, 0, 2), (0, 6, 2)]; // "aa" at 0 and "aa" at 6

        let result = highlight_search_matches(&lines, 0, &matches, 1); // current = second match
        let spans = &result[0].spans;

        // First "aa" should have yellow, second "aa" should have orange (current).
        let yellow_bg = Color::Yellow;
        let orange_bg = Color::Rgb(255, 150, 0);

        let first_match_span = spans.iter().find(|s| s.content.as_ref() == "aa").unwrap();
        assert_eq!(first_match_span.style.bg.unwrap(), yellow_bg);

        let second_match_span = spans.iter().rfind(|s| s.content.as_ref() == "aa").unwrap();
        assert_eq!(second_match_span.style.bg.unwrap(), orange_bg);
    }

    #[test]
    fn highlight_no_matches_passes_through() {
        let lines = vec![Line::from(Span::raw("hello".to_string()))];
        let result = highlight_search_matches(&lines, 0, &[], 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spans.len(), 1);
        assert_eq!(result[0].spans[0].content.as_ref(), "hello");
    }

    #[test]
    fn highlight_respects_scroll_offset() {
        // Visible lines start at document line 10.
        let lines = vec![
            Line::from(Span::raw("first visible".to_string())),
            Line::from(Span::raw("second visible".to_string())),
        ];
        // Match is on absolute line 11 (second visible line).
        let matches = vec![(11, 0, 6)]; // "second"
        let result = highlight_search_matches(&lines, 10, &matches, 0);

        assert_eq!(result[0].spans.len(), 1); // line 10: no match, unchanged
        assert!(result[1].spans.len() > 1); // line 11: has a highlight split
    }
}
