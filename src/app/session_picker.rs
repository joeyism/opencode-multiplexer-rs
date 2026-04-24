use std::path::PathBuf;
use std::sync::Arc;

use nucleo::{
    pattern::{CaseMatching, Normalization},
    Config, Nucleo,
};

use crate::data::db::{models::DbSessionSummary, reader::DbReader};

/// A visible entry with per-field match indices (repo, title, directory).
pub type VisibleEntry = (SessionPickerEntry, Vec<u32>, Vec<u32>, Vec<u32>);

#[derive(Debug, Clone)]
pub struct SessionPickerEntry {
    pub session_id: String,
    pub repo: String,
    pub title: String,
    pub directory: String,
    pub dir_path: PathBuf,
    pub time_updated: i64,
}

pub struct SessionPickerState {
    pub query: String,
    pub selected: usize,
    pub scroll_offset: usize,
    entries: Vec<SessionPickerEntry>,
    matcher: Nucleo<usize>,
}

impl SessionPickerState {
    pub fn load() -> anyhow::Result<Self> {
        let reader = DbReader::open_default()?;
        let summaries = reader.get_all_sessions()?;
        Ok(Self::from_summaries(summaries))
    }

    pub fn from_summaries(summaries: Vec<DbSessionSummary>) -> Self {
        let entries: Vec<SessionPickerEntry> = summaries.iter().map(entry_from_summary).collect();

        let mut matcher = Nucleo::new(Config::DEFAULT, Arc::new(|| {}), Some(1), 1);

        let injector = matcher.injector();
        for (idx, entry) in entries.iter().enumerate() {
            let search_text = format!("{} {} {}", entry.repo, entry.title, entry.directory);
            let _ = injector.push(idx, |_, dst| {
                dst[0] = search_text.into();
            });
        }

        // Let the matcher process all items before we return.
        matcher.tick(10);

        Self {
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
            entries,
            matcher,
        }
    }

    pub fn tick(&mut self) {
        self.matcher.tick(10);
    }

    pub fn insert_char(&mut self, ch: char) {
        self.query.push(ch);
        self.refresh_pattern();
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.refresh_pattern();
        self.selected = 0;
        self.scroll_offset = 0;
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let count = self.matched_count();
        if count > 0 && self.selected < count - 1 {
            self.selected += 1;
        }
    }

    pub fn matched_count(&self) -> usize {
        self.matcher.snapshot().matched_item_count() as usize
    }

    pub fn total_count(&self) -> usize {
        self.entries.len()
    }

    pub fn selected_entry(&self) -> Option<SessionPickerEntry> {
        let snapshot = self.matcher.snapshot();
        let count = snapshot.matched_item_count();
        if count == 0 {
            return None;
        }
        let sel = self.selected.min(count as usize - 1);
        let item = snapshot.matched_items(sel as u32..(sel as u32 + 1)).next()?;
        let idx = *item.data;
        self.entries.get(idx).cloned()
    }

    /// Returns visible entries in rank order with match char indices for highlighting.
    /// Each entry gets separate index lists for repo, title, and directory.
    pub fn visible_entries(
        &self,
        page_size: usize,
    ) -> Vec<VisibleEntry> {
        let snapshot = self.matcher.snapshot();
        let count = snapshot.matched_item_count() as usize;
        if count == 0 {
            return Vec::new();
        }

        let start = self.scroll_offset;
        let end = (start + page_size).min(count);

        let pattern = snapshot.pattern().column_pattern(0);
        let mut indices_matcher = nucleo::Matcher::default();
        let mut indices_buf = Vec::new();

        let mut result = Vec::new();
        for item in snapshot.matched_items(start as u32..end as u32) {
            let idx = *item.data;
            let Some(entry) = self.entries.get(idx) else {
                continue;
            };

            // Compute match indices on the combined search text, then split
            // them into per-field ranges.
            indices_buf.clear();
            let haystack = item.matcher_columns[0].slice(..);
            let _ = pattern.indices(haystack, &mut indices_matcher, &mut indices_buf);
            indices_buf.sort_unstable();
            indices_buf.dedup();

            // The combined search text is "{repo} {title} {directory}"
            // repo chars: 0..repo_len
            // separator: repo_len (the space)
            // title chars: repo_len+1..repo_len+1+title_len
            // separator: repo_len+1+title_len (the space)
            // directory chars: repo_len+1+title_len+1..
            let repo_len = entry.repo.chars().count() as u32;
            let title_len = entry.title.chars().count() as u32;
            let title_start = repo_len + 1;
            let dir_start = title_start + title_len + 1;

            let mut repo_indices = Vec::new();
            let mut title_indices = Vec::new();
            let mut dir_indices = Vec::new();

            for &i in &indices_buf {
                if i < repo_len {
                    repo_indices.push(i);
                } else if i >= title_start && i < title_start + title_len {
                    title_indices.push(i - title_start);
                } else if i >= dir_start {
                    dir_indices.push(i - dir_start);
                }
            }

            result.push((entry.clone(), repo_indices, title_indices, dir_indices));
        }

        result
    }

    /// Ensure scroll_offset keeps the selected item visible.
    pub fn ensure_visible(&mut self, page_size: usize) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + page_size {
            self.scroll_offset = self.selected - page_size + 1;
        }
    }

    fn refresh_pattern(&mut self) {
        self.matcher.pattern.reparse(
            0,
            &self.query,
            CaseMatching::Smart,
            Normalization::Smart,
            false,
        );
    }
}

fn entry_from_summary(summary: &DbSessionSummary) -> SessionPickerEntry {
    let repo = summary
        .worktree
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();

    let (directory, dir_path) = if summary.directory.as_os_str().is_empty() {
        (
            summary
                .worktree
                .to_string_lossy()
                .to_string(),
            summary.worktree.clone(),
        )
    } else {
        (
            summary.directory.to_string_lossy().to_string(),
            summary.directory.clone(),
        )
    };

    SessionPickerEntry {
        session_id: summary.id.clone(),
        repo,
        title: summary.title.clone(),
        directory,
        dir_path,
        time_updated: summary.time_updated,
    }
}
