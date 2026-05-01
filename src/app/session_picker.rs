use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use nucleo::{
    Config, Nucleo,
    pattern::{CaseMatching, Normalization},
};

use crate::data::db::{models::DbSessionSummary, reader::DbReader};

/// A visible entry with per-field match indices (repo, title, directory) and live status.
pub type VisibleEntry = (SessionPickerEntry, Vec<u32>, Vec<u32>, Vec<u32>, bool);

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
    pub live_session_ids: HashSet<String>,
    entries: Vec<SessionPickerEntry>,
    matcher: Nucleo<usize>,
}

impl SessionPickerState {
    pub fn load(live_ids: HashSet<String>) -> anyhow::Result<Self> {
        let reader = DbReader::open_default()?;
        let summaries = reader.get_all_sessions()?;
        Ok(Self::from_summaries(summaries, live_ids))
    }

    pub fn from_summaries(summaries: Vec<DbSessionSummary>, live_ids: HashSet<String>) -> Self {
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
            live_session_ids: live_ids,
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

    fn sorted_match_indices(&self) -> Vec<usize> {
        let snapshot = self.matcher.snapshot();
        let count = snapshot.matched_item_count() as usize;
        if count == 0 {
            return Vec::new();
        }

        let pattern = snapshot.pattern().column_pattern(0);
        let mut scorer = nucleo::Matcher::default();

        let mut scored: Vec<(bool, u32, i64, usize)> = Vec::with_capacity(count);
        for item in snapshot.matched_items(0..count as u32) {
            let entry_idx = *item.data;
            let haystack = item.matcher_columns[0].slice(..);
            if let Some(score) = pattern.score(haystack, &mut scorer) {
                let entry = &self.entries[entry_idx];
                scored.push((
                    self.live_session_ids.contains(&entry.session_id),
                    score,
                    entry.time_updated,
                    entry_idx,
                ));
            }
        }

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| b.2.cmp(&a.2))
        });

        scored.into_iter().map(|(_, _, _, idx)| idx).collect()
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
        let sorted = self.sorted_match_indices();
        let idx = *sorted.get(sel)?;
        let item = snapshot.get_item(idx as u32)?;
        debug_assert_eq!(*item.data, idx);
        self.entries.get(idx).cloned()
    }

    /// Returns visible entries with match char indices for highlighting.
    /// Each entry gets separate index lists for repo, title, and directory.
    pub fn visible_entries(&self, page_size: usize) -> Vec<VisibleEntry> {
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
        let sorted = self.sorted_match_indices();

        for idx in sorted.into_iter().skip(start).take(end - start) {
            let Some(item) = snapshot.get_item(idx as u32) else {
                continue;
            };
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
            let is_live = self.live_session_ids.contains(&entry.session_id);

            for &i in &indices_buf {
                if i < repo_len {
                    repo_indices.push(i);
                } else if i >= title_start && i < title_start + title_len {
                    title_indices.push(i - title_start);
                } else if i >= dir_start {
                    dir_indices.push(i - dir_start);
                }
            }

            result.push((entry.clone(), repo_indices, title_indices, dir_indices, is_live));
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
            summary.worktree.to_string_lossy().to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_entries_sorted_by_time_when_scores_equal() {
        let live_ids = HashSet::new();
        let summaries = vec![
            DbSessionSummary {
                id: "old".into(),
                title: "Test".into(),
                directory: PathBuf::from("/tmp/a"),
                time_updated: 1000,
                archived: false,
                worktree: PathBuf::from("/tmp/project"),
            },
            DbSessionSummary {
                id: "new".into(),
                title: "Test".into(),
                directory: PathBuf::from("/tmp/b"),
                time_updated: 2000,
                archived: false,
                worktree: PathBuf::from("/tmp/project"),
            },
        ];

        let mut picker = SessionPickerState::from_summaries(summaries, live_ids.clone());
        picker.insert_char('T');
        picker.insert_char('e');
        picker.insert_char('s');
        picker.insert_char('t');
        picker.tick();

        assert_eq!(picker.live_session_ids, live_ids);
        assert_eq!(picker.total_count(), 2);
        assert_eq!(picker.selected_entry().as_ref().map(|e| e.session_id.as_str()), Some("new"));

        let visible = picker.visible_entries(10);
        let (entry0, repo0, title0, dir0, live0) = &visible[0];
        assert_eq!(entry0.session_id, "new");
        assert_eq!(repo0, &Vec::<u32>::new());
        assert_eq!(title0, &vec![0, 1, 2, 3]);
        assert_eq!(dir0, &Vec::<u32>::new());
        assert!(!live0);

        let (entry1, _, _, _, live1) = &visible[1];
        assert_eq!(entry1.session_id, "old");
        assert!(!live1);
    }

    #[test]
    fn live_entry_comes_first_when_scores_and_times_match() {
        let mut live_ids = HashSet::new();
        live_ids.insert("live".into());
        let summaries = vec![
            DbSessionSummary {
                id: "live".into(),
                title: "Test".into(),
                directory: PathBuf::from("/tmp/shared"),
                time_updated: 1000,
                archived: false,
                worktree: PathBuf::from("/tmp/project"),
            },
            DbSessionSummary {
                id: "other".into(),
                title: "Test".into(),
                directory: PathBuf::from("/tmp/shared"),
                time_updated: 1000,
                archived: false,
                worktree: PathBuf::from("/tmp/project"),
            },
        ];

        let mut picker = SessionPickerState::from_summaries(summaries, live_ids);
        picker.insert_char('T');
        picker.insert_char('e');
        picker.insert_char('s');
        picker.insert_char('t');
        picker.tick();

        let visible = picker.visible_entries(10);
        assert_eq!(visible[0].0.session_id, "live");
        assert!(visible[0].4);
        assert_eq!(visible[1].0.session_id, "other");
        assert!(!visible[1].4);
    }
}
