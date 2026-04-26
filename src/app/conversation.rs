use std::time::Instant;

use ratatui::text::Line;

use crate::app::focus::AppFocus;

pub struct ConversationViewState {
    session_id: Option<String>,
    session_title: String,
    return_focus: AppFocus,
    document: Vec<Line<'static>>,
    scroll: usize,
    follow_tail: bool,
    last_poll: Option<Instant>,
    load_error: Option<String>,
    // Search state
    search_query: String,
    search_active: bool,
    match_positions: Vec<(usize, usize, usize)>, // (line_idx, byte_start, byte_len)
    current_match: usize,
}

impl Default for ConversationViewState {
    fn default() -> Self {
        Self {
            session_id: None,
            session_title: String::new(),
            return_focus: AppFocus::Sidebar,
            document: Vec::new(),
            scroll: 0,
            follow_tail: true,
            last_poll: None,
            load_error: None,
            search_query: String::new(),
            search_active: false,
            match_positions: Vec::new(),
            current_match: 0,
        }
    }
}

impl ConversationViewState {
    pub fn open(&mut self, session_id: String, session_title: String, return_focus: AppFocus) {
        self.session_id = Some(session_id);
        self.session_title = session_title;
        self.return_focus = return_focus;
        self.document.clear();
        self.scroll = 0;
        self.follow_tail = true;
        self.last_poll = None;
        self.load_error = None;
        self.search_query.clear();
        self.search_active = false;
        self.match_positions.clear();
        self.current_match = 0;
    }

    pub fn close(&mut self) -> AppFocus {
        self.session_id = None;
        self.document.clear();
        self.scroll = 0;
        self.load_error = None;
        self.search_query.clear();
        self.search_active = false;
        self.match_positions.clear();
        self.current_match = 0;
        self.return_focus
    }

    pub fn is_active(&self) -> bool {
        self.session_id.is_some()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn session_title(&self) -> &str {
        &self.session_title
    }

    pub fn load_error(&self) -> Option<&str> {
        self.load_error.as_deref()
    }

    pub fn should_poll(&self, now: Instant) -> bool {
        self.session_id.is_some()
            && self
                .last_poll
                .is_none_or(|last| now.duration_since(last).as_millis() >= 1000)
    }

    pub fn mark_polled(&mut self, now: Instant) {
        self.last_poll = Some(now);
    }

    pub fn replace_document(&mut self, lines: Vec<Line<'static>>, viewport_height: usize) {
        let was_at_tail = self.follow_tail;
        self.document = lines;
        if was_at_tail {
            self.scroll_to_end(viewport_height);
        } else if self.scroll >= self.document.len() {
            self.scroll = self.document.len().saturating_sub(1);
        }
        if !self.search_query.is_empty() {
            self.refresh_matches(viewport_height);
        }
    }

    pub fn set_error(&mut self, error: String) {
        self.load_error = Some(error);
    }

    pub fn clear_error(&mut self) {
        self.load_error = None;
    }

    pub fn visible_lines(&self, viewport_height: usize) -> Vec<Line<'static>> {
        let end = (self.scroll + viewport_height).min(self.document.len());
        self.document[self.scroll..end].to_vec()
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll
    }

    pub fn document_len(&self) -> usize {
        self.document.len()
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll = self.scroll.saturating_sub(amount);
        self.follow_tail = false;
    }

    pub fn scroll_down(&mut self, amount: usize, viewport_height: usize) {
        let max_scroll = self.document.len().saturating_sub(viewport_height);
        self.scroll = (self.scroll + amount).min(max_scroll);
        if self.scroll >= max_scroll {
            self.follow_tail = true;
        }
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
        self.follow_tail = false;
    }

    pub fn scroll_to_end(&mut self, viewport_height: usize) {
        let max_scroll = self.document.len().saturating_sub(viewport_height);
        self.scroll = max_scroll;
        self.follow_tail = true;
    }

    pub fn clamp_scroll(&mut self, viewport_height: usize) {
        let max_scroll = self.document.len().saturating_sub(viewport_height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
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
    fn refresh_matches(&mut self, viewport_height: usize) {
        self.match_positions.clear();
        self.current_match = 0;

        if self.search_query.is_empty() {
            return;
        }

        let query_lower = self.search_query.to_lowercase();

        for (line_idx, line) in self.document.iter().enumerate() {
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
                self.scroll = line_idx;
            } else if line_idx >= self.scroll + viewport_height {
                self.scroll = line_idx.saturating_sub(viewport_height / 2).min(max_scroll);
            }
            self.follow_tail = false;
        }
    }
}
