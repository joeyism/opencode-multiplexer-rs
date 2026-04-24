use ratatui::text::Line;

use crate::app::focus::AppFocus;

pub struct DiffViewState {
    session_id: Option<String>,
    session_title: String,
    raw_diff: String,
    return_focus: AppFocus,
    document: Vec<Line<'static>>,
    scroll: usize,
    // Search state
    search_query: String,
    search_active: bool,
    match_positions: Vec<(usize, usize, usize)>, // (line_idx, byte_start, byte_len)
    current_match: usize,
}

impl Default for DiffViewState {
    fn default() -> Self {
        Self {
            session_id: None,
            session_title: String::new(),
            raw_diff: String::new(),
            return_focus: AppFocus::Sidebar,
            document: Vec::new(),
            scroll: 0,
            search_query: String::new(),
            search_active: false,
            match_positions: Vec::new(),
            current_match: 0,
        }
    }
}

impl DiffViewState {
    pub fn open(
        &mut self,
        session_id: String,
        session_title: String,
        raw_diff: String,
        return_focus: AppFocus,
    ) {
        self.session_id = Some(session_id);
        self.session_title = session_title;
        self.raw_diff = raw_diff;
        self.return_focus = return_focus;
        self.document.clear();
        self.scroll = 0;
        self.search_query.clear();
        self.search_active = false;
        self.match_positions.clear();
        self.current_match = 0;
    }

    pub fn close(&mut self) -> AppFocus {
        self.session_id = None;
        self.raw_diff.clear();
        self.document.clear();
        self.scroll = 0;
        self.search_query.clear();
        self.search_active = false;
        self.match_positions.clear();
        self.current_match = 0;
        self.return_focus
    }

    pub fn is_active(&self) -> bool {
        self.session_id.is_some()
    }

    pub fn raw_diff(&self) -> &str {
        &self.raw_diff
    }

    pub fn replace_document(&mut self, lines: Vec<Line<'static>>, viewport_height: usize) {
        self.document = lines;
        if self.scroll >= self.document.len() {
            let max = self.document.len().saturating_sub(viewport_height);
            self.scroll = max;
        }
    }

    pub fn visible_lines(&self, viewport_height: usize) -> Vec<Line<'static>> {
        let end = (self.scroll + viewport_height).min(self.document.len());
        self.document[self.scroll..end].to_vec()
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: usize, viewport_height: usize) {
        let max_scroll = self.document.len().saturating_sub(viewport_height);
        self.scroll = (self.scroll + amount).min(max_scroll);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    pub fn scroll_to_end(&mut self, viewport_height: usize) {
        let max_scroll = self.document.len().saturating_sub(viewport_height);
        self.scroll = max_scroll;
    }

    pub fn clamp_scroll(&mut self, viewport_height: usize) {
        let max_scroll = self.document.len().saturating_sub(viewport_height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll
    }

    // -----------------------------------------------------------------------
    // Search
    // -----------------------------------------------------------------------

    /// Enter search input mode.
    pub fn start_search(&mut self) {
        self.search_active = true;
    }

    /// Exit search input mode without clearing the query.
    pub fn confirm_search(&mut self) {
        self.search_active = false;
    }

    /// Clear the search query and exit input mode.
    pub fn cancel_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.match_positions.clear();
        self.current_match = 0;
    }

    /// Insert a character into the search query and refresh matches.
    pub fn search_insert(&mut self, ch: char, viewport_height: usize) {
        self.search_query.push(ch);
        self.refresh_matches(viewport_height);
    }

    /// Insert a string (e.g. from paste) into the search query and refresh once.
    pub fn search_insert_str(&mut self, text: &str, viewport_height: usize) {
        self.search_query.push_str(text);
        self.refresh_matches(viewport_height);
    }

    /// Delete last character from the search query and refresh matches.
    pub fn search_backspace(&mut self, viewport_height: usize) {
        self.search_query.pop();
        self.refresh_matches(viewport_height);
    }

    /// Whether the search input bar is active (for key routing).
    pub fn is_searching(&self) -> bool {
        self.search_active
    }

    /// The current search query string.
    pub fn search_query(&self) -> &str {
        &self.search_query
    }

    /// Current match index and total count, if any matches exist.
    pub fn match_status(&self) -> Option<(usize, usize)> {
        if self.match_positions.is_empty() {
            None
        } else {
            Some((self.current_match + 1, self.match_positions.len()))
        }
    }

    /// All match positions: `(line_idx, byte_start, byte_len)`.
    pub fn matches(&self) -> &[(usize, usize, usize)] {
        &self.match_positions
    }

    /// Index of the currently focused match.
    pub fn current_match_index(&self) -> usize {
        self.current_match
    }

    /// Jump to the next match, wrapping around.
    pub fn next_match(&mut self, viewport_height: usize) {
        if self.match_positions.is_empty() {
            return;
        }
        self.current_match = (self.current_match + 1) % self.match_positions.len();
        self.scroll_to_current_match(viewport_height);
    }

    /// Jump to the previous match, wrapping around.
    pub fn prev_match(&mut self, viewport_height: usize) {
        if self.match_positions.is_empty() {
            return;
        }
        if self.current_match == 0 {
            self.current_match = self.match_positions.len() - 1;
        } else {
            self.current_match -= 1;
        }
        self.scroll_to_current_match(viewport_height);
    }

    /// Recalculate match positions from the document and current query.
    /// Scrolls to the first match (or nearest match to current scroll).
    fn refresh_matches(&mut self, viewport_height: usize) {
        self.match_positions.clear();
        self.current_match = 0;

        if self.search_query.is_empty() {
            return;
        }

        let query_lower = self.search_query.to_lowercase();

        for (line_idx, line) in self.document.iter().enumerate() {
            // Flatten all span text into a single string for searching.
            let flat: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let flat_lower = flat.to_lowercase();

            let mut start = 0;
            while let Some(pos) = flat_lower[start..].find(&query_lower) {
                let byte_start = start + pos;
                self.match_positions
                    .push((line_idx, byte_start, query_lower.len()));
                start = byte_start + query_lower.len();
            }
        }

        // Jump to the first match at or after the current scroll position.
        if !self.match_positions.is_empty() {
            self.current_match = self
                .match_positions
                .iter()
                .position(|(line_idx, _, _)| *line_idx >= self.scroll)
                .unwrap_or(0);
            self.scroll_to_current_match(viewport_height);
        }
    }

    /// Scroll so the current match is visible in the viewport.
    fn scroll_to_current_match(&mut self, viewport_height: usize) {
        if let Some(&(line_idx, _, _)) = self.match_positions.get(self.current_match) {
            let max_scroll = self.document.len().saturating_sub(viewport_height);
            if line_idx < self.scroll {
                // Match is above viewport — scroll up to show it.
                self.scroll = line_idx;
            } else if line_idx >= self.scroll + viewport_height {
                // Match is below viewport — scroll down.
                self.scroll = line_idx.saturating_sub(viewport_height / 2).min(max_scroll);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    fn make_document(texts: &[&str]) -> Vec<Line<'static>> {
        texts
            .iter()
            .map(|t| Line::from(Span::raw(t.to_string())))
            .collect()
    }

    #[test]
    fn search_finds_matches_in_document() {
        let mut state = DiffViewState::default();
        state.session_id = Some("test".into());
        state.document = make_document(&[
            "hello world",
            "foo bar",
            "hello again",
            "nothing here",
        ]);
        state.start_search();
        state.search_insert('h', 100);
        state.search_insert('e', 100);
        state.search_insert('l', 100);
        state.search_insert('l', 100);
        state.search_insert('o', 100);

        assert_eq!(state.match_positions.len(), 2);
        assert_eq!(state.match_positions[0], (0, 0, 5)); // "hello" in line 0
        assert_eq!(state.match_positions[1], (2, 0, 5)); // "hello" in line 2
    }

    #[test]
    fn search_is_case_insensitive() {
        let mut state = DiffViewState::default();
        state.session_id = Some("test".into());
        state.document = make_document(&["Hello World", "HELLO again"]);
        state.start_search();
        state.search_insert('h', 100);
        state.search_insert('e', 100);
        state.search_insert('l', 100);
        state.search_insert('l', 100);
        state.search_insert('o', 100);

        assert_eq!(state.match_positions.len(), 2);
    }

    #[test]
    fn next_match_wraps_around() {
        let mut state = DiffViewState::default();
        state.session_id = Some("test".into());
        state.document = make_document(&["aaa", "bbb", "aaa"]);
        state.start_search();
        state.search_insert('a', 100);
        state.search_insert('a', 100);
        state.search_insert('a', 100);

        assert_eq!(state.match_positions.len(), 2);
        assert_eq!(state.current_match, 0);

        state.next_match(100);
        assert_eq!(state.current_match, 1);

        state.next_match(100);
        assert_eq!(state.current_match, 0); // wrapped
    }

    #[test]
    fn prev_match_wraps_around() {
        let mut state = DiffViewState::default();
        state.session_id = Some("test".into());
        state.document = make_document(&["aaa", "bbb", "aaa"]);
        state.start_search();
        state.search_insert('a', 100);
        state.search_insert('a', 100);
        state.search_insert('a', 100);

        assert_eq!(state.current_match, 0);

        state.prev_match(100);
        assert_eq!(state.current_match, 1); // wrapped to last

        state.prev_match(100);
        assert_eq!(state.current_match, 0);
    }

    #[test]
    fn cancel_search_clears_query() {
        let mut state = DiffViewState::default();
        state.session_id = Some("test".into());
        state.document = make_document(&["hello"]);
        state.start_search();
        state.search_insert('h', 100);
        assert!(!state.match_positions.is_empty());

        state.cancel_search();
        assert!(!state.search_active);
        assert!(state.search_query.is_empty());
        assert!(state.match_positions.is_empty());
    }

    #[test]
    fn search_scrolls_to_first_match() {
        let mut state = DiffViewState::default();
        state.session_id = Some("test".into());
        // 100 lines, match only on line 50.
        let mut texts: Vec<&str> = vec!["no match"; 50];
        texts.push("found it");
        texts.extend(vec!["no match"; 49]);
        state.document = make_document(&texts);
        state.scroll = 0;

        state.start_search();
        // Type "found"
        for ch in "found".chars() {
            state.search_insert(ch, 20);
        }

        assert_eq!(state.match_positions.len(), 1);
        assert_eq!(state.match_positions[0].0, 50); // line 50
        // Scroll should have moved to make line 50 visible.
        assert!(state.scroll <= 50);
        assert!(state.scroll + 20 > 50);
    }
}
