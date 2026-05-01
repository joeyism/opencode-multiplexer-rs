use std::sync::Arc;

use nucleo::{
    Config, Nucleo,
    pattern::{CaseMatching, Normalization},
};

use crate::data::db::{models::DbUserMessage, reader::DbReader};

pub type VisibleEntry = (MessagePickerEntry, Vec<u32>, Vec<u32>);

#[derive(Debug, Clone)]
pub struct MessagePickerEntry {
    pub message_id: String,
    pub session_id: String,
    pub session_title: String,
    pub time_created: i64,
    pub text: String,
    pub compact_text: String,
}

pub struct MessagePickerState {
    pub query: String,
    pub selected: usize,
    pub scroll_offset: usize,
    entries: Vec<MessagePickerEntry>,
    matcher: Nucleo<usize>,
}

impl MessagePickerState {
    pub fn load() -> anyhow::Result<Self> {
        let reader = DbReader::open_default()?;
        let messages = reader.get_all_user_messages()?;
        Ok(Self::from_messages(messages))
    }

    pub fn from_messages(messages: Vec<DbUserMessage>) -> Self {
        let entries: Vec<MessagePickerEntry> = messages.iter().map(entry_from_message).collect();

        let mut matcher = Nucleo::new(Config::DEFAULT, Arc::new(|| {}), Some(1), 1);

        let injector = matcher.injector();
        for (idx, entry) in entries.iter().enumerate() {
            let search_text = format!("{} {}", entry.session_title, entry.compact_text);
            let _ = injector.push(idx, |_, dst| {
                dst[0] = search_text.into();
            });
        }

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

    fn sorted_match_indices(&self) -> Vec<usize> {
        let snapshot = self.matcher.snapshot();
        let count = snapshot.matched_item_count() as usize;
        if count == 0 {
            return Vec::new();
        }

        let pattern = snapshot.pattern().column_pattern(0);
        let mut scorer = nucleo::Matcher::default();

        let mut scored: Vec<(u32, usize)> = Vec::with_capacity(count);
        for item in snapshot.matched_items(0..count as u32) {
            let entry_idx = *item.data;
            let haystack = item.matcher_columns[0].slice(..);
            if let Some(score) = pattern.score(haystack, &mut scorer) {
                scored.push((score, entry_idx));
            }
        }

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| {
                let a_time = self.entries.get(a.1).map(|e| e.time_created).unwrap_or(0);
                let b_time = self.entries.get(b.1).map(|e| e.time_created).unwrap_or(0);
                b_time.cmp(&a_time)
            })
        });

        scored.into_iter().map(|(_, idx)| idx).collect()
    }

    pub fn selected_entry(&self) -> Option<MessagePickerEntry> {
        let snapshot = self.matcher.snapshot();
        let count = snapshot.matched_item_count();
        if count == 0 {
            return None;
        }
        let sel = self.selected.min(count as usize - 1);
        let sorted = self.sorted_match_indices();
        let entry_idx = *sorted.get(sel)?;
        let item = snapshot.get_item(entry_idx as u32)?;
        debug_assert_eq!(*item.data, entry_idx);
        self.entries.get(entry_idx).cloned()
    }

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
        for &entry_idx in &sorted[start..end] {
            let Some(item) = snapshot.get_item(entry_idx as u32) else {
                continue;
            };
            debug_assert_eq!(*item.data, entry_idx);
            let Some(entry) = self.entries.get(entry_idx) else {
                continue;
            };

            indices_buf.clear();
            let haystack = item.matcher_columns[0].slice(..);
            let _ = pattern.indices(haystack, &mut indices_matcher, &mut indices_buf);
            indices_buf.sort_unstable();
            indices_buf.dedup();

            let title_len = entry.session_title.chars().count() as u32;
            let text_start = title_len + 1;

            let mut title_indices = Vec::new();
            let mut text_indices = Vec::new();

            for &i in &indices_buf {
                if i < title_len {
                    title_indices.push(i);
                } else if i >= text_start {
                    text_indices.push(i - text_start);
                }
            }

            result.push((entry.clone(), title_indices, text_indices));
        }

        result
    }

    pub fn ensure_visible(&mut self, page_size: usize) {
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + page_size {
            self.scroll_offset = self.selected.saturating_sub(page_size).saturating_add(1);
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

fn compact_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn entry_from_message(message: &DbUserMessage) -> MessagePickerEntry {
    let session_title = if message.session_title.trim().is_empty() {
        "(untitled)".to_string()
    } else {
        message.session_title.clone()
    };

    MessagePickerEntry {
        message_id: message.id.clone(),
        session_id: message.session_id.clone(),
        session_title,
        time_created: message.time_created,
        text: message.text.clone(),
        compact_text: compact_text(&message.text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_entries_sorted_by_time_when_scores_equal() {
        let messages = vec![
            DbUserMessage {
                id: "1".into(),
                session_id: "s1".into(),
                session_title: "Test A".into(),
                time_created: 1000,
                text: "hello world".into(),
            },
            DbUserMessage {
                id: "2".into(),
                session_id: "s2".into(),
                session_title: "Test B".into(),
                time_created: 2000,
                text: "hello world".into(),
            },
        ];

        let picker = MessagePickerState::from_messages(messages);
        let visible = picker.visible_entries(10);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].0.message_id, "2");
        assert_eq!(visible[1].0.message_id, "1");
    }

    #[test]
    fn test_compact_text() {
        assert_eq!(compact_text("  hello \n \t world  "), "hello world");
    }
}
