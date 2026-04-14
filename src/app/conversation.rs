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
    }

    pub fn close(&mut self) -> AppFocus {
        self.session_id = None;
        self.document.clear();
        self.scroll = 0;
        self.load_error = None;
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
                .map_or(true, |last| now.duration_since(last).as_millis() >= 1000)
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
}
