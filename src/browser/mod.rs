use micron::{
    parse as parse_micron, render_with_hitboxes, Document, FormState, Hitbox, HitboxTarget,
    RenderConfig,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget},
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserState {
    Empty,
    Connecting,
    Retrieving,
    Loaded,
    Failed,
}

#[derive(Debug, Clone)]
struct PageEntry {
    url: String,
    content: String,
    scroll: u16,
}

pub struct Browser {
    state: BrowserState,
    current: Option<PageEntry>,
    back_stack: Vec<PageEntry>,
    forward_stack: Vec<PageEntry>,
    selected: usize,
    status: Option<String>,
    cached_doc: Option<Document>,
    hitboxes: Vec<Hitbox>,
    field_values: HashMap<String, String>,
    checkbox_states: HashMap<String, bool>,
    radio_states: HashMap<String, String>,
    editing_field: Option<usize>,
    render_width: u16,
}

pub enum BrowserAction {
    Navigate { url: String },
    None,
}

pub enum InputResult {
    Consumed,
    NotConsumed,
}

impl Browser {
    pub fn new() -> Self {
        Self {
            state: BrowserState::Empty,
            current: None,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            selected: 0,
            status: None,
            cached_doc: None,
            hitboxes: Vec::new(),
            field_values: HashMap::new(),
            checkbox_states: HashMap::new(),
            radio_states: HashMap::new(),
            editing_field: None,
            render_width: 80,
        }
    }

    pub fn state(&self) -> BrowserState {
        self.state
    }

    pub fn current_url(&self) -> Option<&str> {
        self.current.as_ref().map(|e| e.url.as_str())
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    pub fn is_editing(&self) -> bool {
        self.editing_field.is_some()
    }

    pub fn navigate(&mut self, url: String) -> BrowserAction {
        if let Some(current) = self.current.take() {
            self.back_stack.push(current);
        }
        self.forward_stack.clear();
        self.state = BrowserState::Connecting;
        self.status = Some("Connecting...".into());
        self.current = Some(PageEntry {
            url: url.clone(),
            content: String::new(),
            scroll: 0,
        });
        self.clear_page_state();
        BrowserAction::Navigate { url }
    }

    fn clear_page_state(&mut self) {
        self.cached_doc = None;
        self.hitboxes.clear();
        self.field_values.clear();
        self.checkbox_states.clear();
        self.radio_states.clear();
        self.selected = 0;
        self.editing_field = None;
    }

    pub fn set_retrieving(&mut self) {
        if self.state == BrowserState::Connecting {
            self.state = BrowserState::Retrieving;
            self.status = Some("Retrieving...".into());
        }
    }

    pub fn set_content(&mut self, url: &str, content: String, width: u16) {
        if self.current.as_ref().map(|e| e.url.as_str()) == Some(url) {
            if let Some(ref mut entry) = self.current {
                entry.content = content.clone();
                entry.scroll = 0;
            }
            self.state = BrowserState::Loaded;
            self.status = None;
            self.rebuild_cache(&content, width);
        }
    }

    pub fn set_failed(&mut self, url: &str, reason: String) {
        if self.current.as_ref().map(|e| e.url.as_str()) == Some(url) {
            self.state = BrowserState::Failed;
            self.status = Some(reason);
        }
    }

    fn rebuild_cache(&mut self, content: &str, width: u16) {
        let doc = parse_micron(content);
        let form_state = FormState {
            fields: self.field_values.clone(),
            checkboxes: self.checkbox_states.clone(),
            radios: self.radio_states.clone(),
        };
        let config = RenderConfig {
            width,
            form_state: Some(&form_state),
            ..Default::default()
        };
        let output = render_with_hitboxes(&doc, &config);
        self.hitboxes = output.hitboxes;
        self.render_width = width;

        log::debug!(
            "rebuild_cache: width={}, found {} hitboxes",
            width,
            self.hitboxes.len()
        );
        for (i, hb) in self.hitboxes.iter().enumerate() {
            log::debug!(
                "  hitbox {}: line={} col={}..{} {:?}",
                i,
                hb.line,
                hb.col_start,
                hb.col_end,
                hb.target
            );
        }

        for hitbox in &self.hitboxes {
            match &hitbox.target {
                HitboxTarget::TextField { name, .. } => {
                    self.field_values.entry(name.clone()).or_default();
                }
                HitboxTarget::Checkbox { name } => {
                    self.checkbox_states.entry(name.clone()).or_insert(false);
                }
                HitboxTarget::Radio { name, value } => {
                    self.radio_states
                        .entry(name.clone())
                        .or_insert_with(|| value.clone());
                }
                HitboxTarget::Link { .. } => {}
            }
        }

        self.cached_doc = Some(doc);
        self.selected = 0;
        self.editing_field = None;
    }

    pub fn go_back(&mut self) -> BrowserAction {
        let Some(prev) = self.back_stack.pop() else {
            return BrowserAction::None;
        };
        if let Some(current) = self.current.take() {
            self.forward_stack.push(current);
        }
        let url = prev.url.clone();
        let has_content = !prev.content.is_empty();
        self.current = Some(prev);
        if has_content {
            if let Some(ref entry) = self.current {
                self.rebuild_cache(&entry.content.clone(), self.render_width);
            }
            self.state = BrowserState::Loaded;
            self.status = None;
            BrowserAction::None
        } else {
            self.state = BrowserState::Connecting;
            self.status = Some("Connecting...".into());
            BrowserAction::Navigate { url }
        }
    }

    pub fn go_forward(&mut self) -> BrowserAction {
        let Some(next) = self.forward_stack.pop() else {
            return BrowserAction::None;
        };
        if let Some(current) = self.current.take() {
            self.back_stack.push(current);
        }
        let url = next.url.clone();
        let has_content = !next.content.is_empty();
        self.current = Some(next);
        if has_content {
            if let Some(ref entry) = self.current {
                self.rebuild_cache(&entry.content.clone(), self.render_width);
            }
            self.state = BrowserState::Loaded;
            self.status = None;
            BrowserAction::None
        } else {
            self.state = BrowserState::Connecting;
            self.status = Some("Connecting...".into());
            BrowserAction::Navigate { url }
        }
    }

    pub fn scroll_up(&mut self) {
        if let Some(ref mut entry) = self.current {
            entry.scroll = entry.scroll.saturating_sub(1);
        }
    }

    pub fn scroll_down(&mut self) {
        if let Some(ref mut entry) = self.current {
            entry.scroll = entry.scroll.saturating_add(1);
        }
    }

    pub fn scroll_page_up(&mut self, page_height: u16) {
        if let Some(ref mut entry) = self.current {
            entry.scroll = entry.scroll.saturating_sub(page_height.saturating_sub(2));
        }
    }

    pub fn scroll_page_down(&mut self, page_height: u16) {
        if let Some(ref mut entry) = self.current {
            entry.scroll = entry.scroll.saturating_add(page_height.saturating_sub(2));
        }
    }

    pub fn select_next(&mut self) {
        if !self.hitboxes.is_empty() {
            self.selected = (self.selected + 1) % self.hitboxes.len();
            self.scroll_to_selected();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.hitboxes.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.hitboxes.len() - 1);
            self.scroll_to_selected();
        }
    }

    fn scroll_to_selected(&mut self) {
        if let Some(hitbox) = self.hitboxes.get(self.selected) {
            if let Some(ref mut entry) = self.current {
                entry.scroll = hitbox.line.saturating_sub(2) as u16;
            }
        }
    }

    pub fn activate(&mut self) -> BrowserAction {
        let Some(hitbox) = self.hitboxes.get(self.selected) else {
            return BrowserAction::None;
        };

        match &hitbox.target {
            HitboxTarget::Link { url } => BrowserAction::Navigate { url: url.clone() },
            HitboxTarget::TextField { .. } => {
                self.editing_field = Some(self.selected);
                BrowserAction::None
            }
            HitboxTarget::Checkbox { name } => {
                let current = self.checkbox_states.get(name).copied().unwrap_or(false);
                self.checkbox_states.insert(name.clone(), !current);
                BrowserAction::None
            }
            HitboxTarget::Radio { name, value } => {
                self.radio_states.insert(name.clone(), value.clone());
                BrowserAction::None
            }
        }
    }

    pub fn cancel_edit(&mut self) {
        self.editing_field = None;
    }

    pub fn handle_text_input(&mut self, c: char) -> InputResult {
        let Some(idx) = self.editing_field else {
            return InputResult::NotConsumed;
        };
        let Some(hitbox) = self.hitboxes.get(idx) else {
            return InputResult::NotConsumed;
        };
        if let HitboxTarget::TextField { name, .. } = &hitbox.target {
            self.field_values.entry(name.clone()).or_default().push(c);
            InputResult::Consumed
        } else {
            InputResult::NotConsumed
        }
    }

    pub fn handle_backspace(&mut self) -> InputResult {
        let Some(idx) = self.editing_field else {
            return InputResult::NotConsumed;
        };
        let Some(hitbox) = self.hitboxes.get(idx) else {
            return InputResult::NotConsumed;
        };
        if let HitboxTarget::TextField { name, .. } = &hitbox.target {
            if let Some(val) = self.field_values.get_mut(name) {
                val.pop();
            }
            InputResult::Consumed
        } else {
            InputResult::NotConsumed
        }
    }

    pub fn click(&mut self, x: u16, y: u16, content_area: Rect) -> BrowserAction {
        log::debug!(
            "click: x={} y={} content_area={:?} state={:?}",
            x,
            y,
            content_area,
            self.state
        );

        if self.state != BrowserState::Loaded {
            log::debug!("click: not loaded, ignoring");
            return BrowserAction::None;
        }

        let scroll = self.current.as_ref().map(|e| e.scroll).unwrap_or(0);

        if x < content_area.x || x >= content_area.x + content_area.width {
            log::debug!("click: outside content area (x)");
            return BrowserAction::None;
        }
        if y < content_area.y || y >= content_area.y + content_area.height {
            log::debug!("click: outside content area (y)");
            return BrowserAction::None;
        }

        let rel_y = y - content_area.y;
        let rel_x = x - content_area.x;
        let doc_line = (rel_y + scroll) as usize;
        let doc_col = rel_x as usize;

        log::debug!(
            "click: doc_line={} doc_col={} scroll={} hitboxes={}",
            doc_line,
            doc_col,
            scroll,
            self.hitboxes.len()
        );

        for (idx, hitbox) in self.hitboxes.iter().enumerate() {
            if hitbox.line == doc_line && doc_col >= hitbox.col_start && doc_col < hitbox.col_end {
                log::debug!("click: hit hitbox {}", idx);
                self.selected = idx;
                return self.activate();
            }
        }

        log::debug!("click: no hitbox at position");
        BrowserAction::None
    }

    pub fn selected_info(&self) -> Option<&str> {
        let hitbox = self.hitboxes.get(self.selected)?;
        match &hitbox.target {
            HitboxTarget::Link { url } => Some(url),
            HitboxTarget::TextField { name, .. } => Some(name),
            HitboxTarget::Checkbox { name } => Some(name),
            HitboxTarget::Radio { name, .. } => Some(name),
        }
    }

    pub fn selected_link_url(&self) -> Option<&str> {
        let hitbox = self.hitboxes.get(self.selected)?;
        match &hitbox.target {
            HitboxTarget::Link { url } => Some(url),
            _ => None,
        }
    }

    pub fn hitbox_count(&self) -> usize {
        self.hitboxes.len()
    }

    pub fn field_value(&self, name: &str) -> Option<&str> {
        self.field_values.get(name).map(|s| s.as_str())
    }

    pub fn checkbox_checked(&self, name: &str) -> bool {
        self.checkbox_states.get(name).copied().unwrap_or(false)
    }

    pub fn radio_selected(&self, name: &str) -> Option<&str> {
        self.radio_states.get(name).map(|s| s.as_str())
    }

    pub fn form_state(&self) -> FormState {
        FormState {
            fields: self.field_values.clone(),
            checkboxes: self.checkbox_states.clone(),
            radios: self.radio_states.clone(),
        }
    }

    pub fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let content = self.build_content(area.width);
        let scroll = self.current.as_ref().map(|e| e.scroll).unwrap_or(0);

        let is_loaded = self.state == BrowserState::Loaded;
        let para = if is_loaded {
            Paragraph::new(content).scroll((scroll, 0))
        } else {
            Paragraph::new(content).alignment(ratatui::layout::Alignment::Center)
        };
        para.render(area, buf);
    }

    fn build_content(&self, width: u16) -> Text<'static> {
        match self.state {
            BrowserState::Empty => Text::from(vec![
                Line::from(""),
                Line::from(""),
                Line::from(Span::styled(
                    "No page loaded",
                    Style::default().fg(Color::DarkGray),
                )),
            ]),
            BrowserState::Connecting => Text::from(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Connecting...",
                    Style::default().fg(Color::Yellow),
                )),
            ]),
            BrowserState::Retrieving => Text::from(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Retrieving...",
                    Style::default().fg(Color::Yellow),
                )),
            ]),
            BrowserState::Loaded => {
                if let Some(ref doc) = self.cached_doc {
                    let form_state = FormState {
                        fields: self.field_values.clone(),
                        checkboxes: self.checkbox_states.clone(),
                        radios: self.radio_states.clone(),
                    };
                    let config = RenderConfig {
                        width,
                        form_state: Some(&form_state),
                        ..Default::default()
                    };
                    render_with_hitboxes(doc, &config).text
                } else {
                    Text::from(Line::from("(empty page)"))
                }
            }
            BrowserState::Failed => {
                let msg = self
                    .status
                    .clone()
                    .unwrap_or_else(|| "Request failed".into());
                Text::from(vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "!",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(msg, Style::default().fg(Color::Red))),
                ])
            }
        }
    }
}

impl Default for Browser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_state() {
        let browser = Browser::new();
        assert_eq!(browser.state(), BrowserState::Empty);
        assert!(browser.current_url().is_none());
        assert!(!browser.can_go_back());
        assert!(!browser.can_go_forward());
    }

    #[test]
    fn navigate_sets_connecting() {
        let mut browser = Browser::new();
        let req = browser.navigate("abc123:page.mu".into());
        assert!(matches!(req, BrowserAction::Navigate { .. }));
        assert_eq!(browser.state(), BrowserState::Connecting);
        assert_eq!(browser.current_url(), Some("abc123:page.mu"));
    }

    #[test]
    fn set_content_transitions_to_loaded() {
        let mut browser = Browser::new();
        browser.navigate("abc123:page.mu".into());
        browser.set_content("abc123:page.mu", "Hello".into(), 80);
        assert_eq!(browser.state(), BrowserState::Loaded);
    }

    #[test]
    fn back_forward_navigation() {
        let mut browser = Browser::new();
        browser.navigate("hash1:page1.mu".into());
        browser.set_content("hash1:page1.mu", "Page 1".into(), 80);

        browser.navigate("hash2:page2.mu".into());
        browser.set_content("hash2:page2.mu", "Page 2".into(), 80);

        assert!(browser.can_go_back());
        assert!(!browser.can_go_forward());

        browser.go_back();
        assert_eq!(browser.current_url(), Some("hash1:page1.mu"));
        assert!(browser.can_go_forward());

        browser.go_forward();
        assert_eq!(browser.current_url(), Some("hash2:page2.mu"));
    }

    #[test]
    fn scroll() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "content".into(), 80);

        browser.scroll_down();
        browser.scroll_down();
        assert_eq!(browser.current.as_ref().unwrap().scroll, 2);

        browser.scroll_up();
        assert_eq!(browser.current.as_ref().unwrap().scroll, 1);
    }

    #[test]
    fn extract_links_from_content() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content(
            "abc:page.mu",
            "`[Link 1`http://a]\n`[Link 2`http://b]".into(),
            80,
        );

        assert_eq!(browser.hitbox_count(), 2);
    }

    #[test]
    fn click_on_link() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "`[Click Me`http://target]".into(), 80);

        assert_eq!(browser.hitbox_count(), 1);

        let area = Rect::new(0, 0, 80, 24);
        let req = browser.click(3, 0, area);

        assert!(matches!(req, BrowserAction::Navigate { url } if url == "http://target"));
    }

    #[test]
    fn click_outside_link() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "Some text `[Link`http://x]".into(), 80);

        let area = Rect::new(0, 0, 80, 24);
        let req = browser.click(0, 0, area);

        assert!(matches!(req, BrowserAction::None));
    }

    #[test]
    fn checkbox_toggle() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "`<?|agree|yes`I agree>".into(), 80);

        assert!(!browser.checkbox_checked("agree"));

        browser.activate();
        assert!(browser.checkbox_checked("agree"));

        browser.activate();
        assert!(!browser.checkbox_checked("agree"));
    }

    #[test]
    fn text_field_input() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "`<|name`Enter name>".into(), 80);

        browser.activate();
        assert!(browser.is_editing());

        browser.handle_text_input('H');
        browser.handle_text_input('i');

        assert_eq!(browser.field_value("name"), Some("Hi"));

        browser.handle_backspace();
        assert_eq!(browser.field_value("name"), Some("H"));
    }
}
