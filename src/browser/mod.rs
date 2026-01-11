use micron::{
    parse as parse_micron, render as render_micron, Document, Element, Field, FieldKind, Link,
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

#[derive(Debug, Clone)]
enum InteractiveKind {
    Link { url: String },
    TextField { name: String, masked: bool },
    Checkbox { name: String },
    Radio { name: String, value: String },
}

#[derive(Debug, Clone)]
struct Interactive {
    kind: InteractiveKind,
    line: usize,
    col_start: usize,
    col_end: usize,
}

pub struct Browser {
    state: BrowserState,
    current: Option<PageEntry>,
    back_stack: Vec<PageEntry>,
    forward_stack: Vec<PageEntry>,
    selected: usize,
    status: Option<String>,
    cached_doc: Option<Document>,
    interactives: Vec<Interactive>,
    field_values: HashMap<String, String>,
    checkbox_states: HashMap<String, bool>,
    radio_states: HashMap<String, String>,
    editing_field: Option<usize>,
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
            interactives: Vec::new(),
            field_values: HashMap::new(),
            checkbox_states: HashMap::new(),
            radio_states: HashMap::new(),
            editing_field: None,
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
        self.interactives.clear();
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

    pub fn set_content(&mut self, url: &str, content: String) {
        if self.current.as_ref().map(|e| e.url.as_str()) == Some(url) {
            if let Some(ref mut entry) = self.current {
                entry.content = content.clone();
                entry.scroll = 0;
            }
            self.state = BrowserState::Loaded;
            self.status = None;
            self.rebuild_cache(&content);
        }
    }

    pub fn set_failed(&mut self, url: &str, reason: String) {
        if self.current.as_ref().map(|e| e.url.as_str()) == Some(url) {
            self.state = BrowserState::Failed;
            self.status = Some(reason);
        }
    }

    fn rebuild_cache(&mut self, content: &str) {
        let doc = parse_micron(content);
        self.interactives = extract_interactives(&doc);

        for interactive in &self.interactives {
            match &interactive.kind {
                InteractiveKind::TextField { name, .. } => {
                    self.field_values.entry(name.clone()).or_default();
                }
                InteractiveKind::Checkbox { name } => {
                    self.checkbox_states.entry(name.clone()).or_insert(false);
                }
                InteractiveKind::Radio { name, value } => {
                    self.radio_states
                        .entry(name.clone())
                        .or_insert_with(|| value.clone());
                }
                InteractiveKind::Link { .. } => {}
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
                self.rebuild_cache(&entry.content.clone());
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
                self.rebuild_cache(&entry.content.clone());
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
        if !self.interactives.is_empty() {
            self.selected = (self.selected + 1) % self.interactives.len();
            self.scroll_to_selected();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.interactives.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.interactives.len() - 1);
            self.scroll_to_selected();
        }
    }

    fn scroll_to_selected(&mut self) {
        if let Some(interactive) = self.interactives.get(self.selected) {
            if let Some(ref mut entry) = self.current {
                entry.scroll = interactive.line.saturating_sub(2) as u16;
            }
        }
    }

    pub fn activate(&mut self) -> BrowserAction {
        let Some(interactive) = self.interactives.get(self.selected) else {
            return BrowserAction::None;
        };

        match &interactive.kind {
            InteractiveKind::Link { url } => {
                let url = url.clone();
                self.navigate(url)
            }
            InteractiveKind::TextField { name, .. } => {
                self.editing_field = Some(self.selected);
                BrowserAction::None
            }
            InteractiveKind::Checkbox { name } => {
                let current = self.checkbox_states.get(name).copied().unwrap_or(false);
                self.checkbox_states.insert(name.clone(), !current);
                BrowserAction::None
            }
            InteractiveKind::Radio { name, value } => {
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
        let Some(interactive) = self.interactives.get(idx) else {
            return InputResult::NotConsumed;
        };
        if let InteractiveKind::TextField { name, .. } = &interactive.kind {
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
        let Some(interactive) = self.interactives.get(idx) else {
            return InputResult::NotConsumed;
        };
        if let InteractiveKind::TextField { name, .. } = &interactive.kind {
            if let Some(val) = self.field_values.get_mut(name) {
                val.pop();
            }
            InputResult::Consumed
        } else {
            InputResult::NotConsumed
        }
    }

    pub fn click(&mut self, x: u16, y: u16, content_area: Rect) -> BrowserAction {
        if self.state != BrowserState::Loaded {
            return BrowserAction::None;
        }

        let scroll = self.current.as_ref().map(|e| e.scroll).unwrap_or(0);

        if x < content_area.x || x >= content_area.x + content_area.width {
            return BrowserAction::None;
        }
        if y < content_area.y || y >= content_area.y + content_area.height {
            return BrowserAction::None;
        }

        let rel_y = y - content_area.y;
        let rel_x = x - content_area.x;
        let doc_line = (rel_y + scroll) as usize;
        let doc_col = rel_x as usize;

        for (idx, interactive) in self.interactives.iter().enumerate() {
            if interactive.line == doc_line
                && doc_col >= interactive.col_start
                && doc_col < interactive.col_end
            {
                self.selected = idx;
                return self.activate();
            }
        }

        BrowserAction::None
    }

    pub fn selected_info(&self) -> Option<&str> {
        let interactive = self.interactives.get(self.selected)?;
        match &interactive.kind {
            InteractiveKind::Link { url } => Some(url),
            InteractiveKind::TextField { name, .. } => Some(name),
            InteractiveKind::Checkbox { name } => Some(name),
            InteractiveKind::Radio { name, .. } => Some(name),
        }
    }

    pub fn interactive_count(&self) -> usize {
        self.interactives.len()
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

    pub fn form_data(&self) -> FormData {
        FormData {
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
                    let config = RenderConfig {
                        width,
                        ..Default::default()
                    };
                    render_micron(doc, &config)
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

#[derive(Debug, Clone, Default)]
pub struct FormData {
    pub fields: HashMap<String, String>,
    pub checkboxes: HashMap<String, bool>,
    pub radios: HashMap<String, String>,
}

fn extract_interactives(doc: &Document) -> Vec<Interactive> {
    let mut result = Vec::new();
    for (line_idx, line) in doc.lines.iter().enumerate() {
        let mut col = 0usize;
        for element in &line.elements {
            match element {
                Element::Link(Link { url, label, .. }) => {
                    let len = label.chars().count();
                    result.push(Interactive {
                        kind: InteractiveKind::Link { url: url.clone() },
                        line: line_idx,
                        col_start: col,
                        col_end: col + len,
                    });
                    col += len;
                }
                Element::Field(Field {
                    name,
                    width,
                    masked,
                    kind,
                    ..
                }) => {
                    let w = width.unwrap_or(24) as usize;
                    let interactive_kind = match kind {
                        FieldKind::Text => InteractiveKind::TextField {
                            name: name.clone(),
                            masked: *masked,
                        },
                        FieldKind::Checkbox { .. } => {
                            InteractiveKind::Checkbox { name: name.clone() }
                        }
                        FieldKind::Radio { value, .. } => InteractiveKind::Radio {
                            name: name.clone(),
                            value: value.clone(),
                        },
                    };
                    result.push(Interactive {
                        kind: interactive_kind,
                        line: line_idx,
                        col_start: col,
                        col_end: col + w,
                    });
                    col += w;
                }
                Element::Text(t) => {
                    col += t.text.chars().count();
                }
                Element::Partial(_) => {}
            }
        }
    }
    result
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
        browser.set_content("abc123:page.mu", "Hello".into());
        assert_eq!(browser.state(), BrowserState::Loaded);
    }

    #[test]
    fn back_forward_navigation() {
        let mut browser = Browser::new();
        browser.navigate("hash1:page1.mu".into());
        browser.set_content("hash1:page1.mu", "Page 1".into());

        browser.navigate("hash2:page2.mu".into());
        browser.set_content("hash2:page2.mu", "Page 2".into());

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
        browser.set_content("abc:page.mu", "content".into());

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
        );

        assert_eq!(browser.interactive_count(), 2);
    }

    #[test]
    fn click_on_link() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "`[Click Me`http://target]".into());

        assert_eq!(browser.interactive_count(), 1);

        let area = Rect::new(0, 0, 80, 24);
        let req = browser.click(3, 0, area);

        assert!(matches!(req, BrowserAction::Navigate { url } if url == "http://target"));
    }

    #[test]
    fn click_outside_link() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "Some text `[Link`http://x]".into());

        let area = Rect::new(0, 0, 80, 24);
        let req = browser.click(0, 0, area);

        assert!(matches!(req, BrowserAction::None));
    }

    #[test]
    fn checkbox_toggle() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "`<?|agree|yes`I agree>".into());

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
        browser.set_content("abc:page.mu", "`<|name`Enter name>".into());

        browser.activate();
        assert!(browser.is_editing());

        browser.handle_text_input('H');
        browser.handle_text_input('i');

        assert_eq!(browser.field_value("name"), Some("Hi"));

        browser.handle_backspace();
        assert_eq!(browser.field_value("name"), Some("H"));
    }
}
