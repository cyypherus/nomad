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
pub enum LoadingState {
    Idle,
    Connecting,
    Retrieving,
}

#[derive(Debug, Clone)]
struct PageEntry {
    url: String,
    content: String,
    scroll: u16,
}

pub struct Browser {
    current: Option<PageEntry>,
    back_stack: Vec<PageEntry>,
    forward_stack: Vec<PageEntry>,
    pending_url: Option<String>,
    loading: LoadingState,
    error: Option<String>,
    selected: usize,
    cached_doc: Option<Document>,
    hitboxes: Vec<Hitbox>,
    field_values: HashMap<String, String>,
    checkbox_states: HashMap<String, bool>,
    radio_states: HashMap<String, String>,
    editing_field: Option<usize>,
    render_width: u16,
    debug_hitboxes: bool,
}

pub enum BrowserAction {
    Navigate {
        url: String,
        form_data: HashMap<String, String>,
    },
    None,
}

pub enum InputResult {
    Consumed,
    NotConsumed,
}

impl Browser {
    pub fn new() -> Self {
        Self {
            current: None,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            pending_url: None,
            loading: LoadingState::Idle,
            error: None,
            selected: 0,
            cached_doc: None,
            hitboxes: Vec::new(),
            field_values: HashMap::new(),
            checkbox_states: HashMap::new(),
            radio_states: HashMap::new(),
            editing_field: None,
            render_width: 80,
            debug_hitboxes: false,
        }
    }

    pub fn toggle_debug_hitboxes(&mut self) {
        self.debug_hitboxes = !self.debug_hitboxes;
    }

    pub fn debug_hitboxes(&self) -> bool {
        self.debug_hitboxes
    }

    pub fn loading_state(&self) -> LoadingState {
        self.loading
    }

    pub fn is_loading(&self) -> bool {
        self.loading != LoadingState::Idle
    }

    pub fn current_url(&self) -> Option<&str> {
        self.current.as_ref().map(|e| e.url.as_str())
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
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
        self.pending_url = Some(url.clone());
        self.loading = LoadingState::Connecting;
        self.error = None;
        BrowserAction::Navigate {
            url,
            form_data: HashMap::new(),
        }
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

    pub fn set_retrieving(&mut self, url: &str) {
        if self.pending_url.as_deref() == Some(url) && self.loading == LoadingState::Connecting {
            self.loading = LoadingState::Retrieving;
        }
    }

    pub fn set_content(&mut self, url: &str, content: String, width: u16) {
        if self.pending_url.as_deref() != Some(url) {
            return;
        }
        if let Some(current) = self.current.take() {
            self.back_stack.push(current);
        }
        self.forward_stack.clear();
        self.current = Some(PageEntry {
            url: url.to_string(),
            content: content.clone(),
            scroll: 0,
        });
        self.pending_url = None;
        self.loading = LoadingState::Idle;
        self.error = None;
        self.clear_page_state();
        self.rebuild_cache(&content, width);
    }

    pub fn set_failed(&mut self, url: &str, reason: String) {
        if self.pending_url.as_deref() == Some(url) {
            self.pending_url = None;
            self.loading = LoadingState::Idle;
            self.error = Some(reason);
        }
    }

    fn rebuild_cache(&mut self, content: &str, width: u16) {
        let doc = parse_micron(content);
        let form_state = FormState {
            fields: self.field_values.clone(),
            checkboxes: self.checkbox_states.clone(),
            radios: self.radio_states.clone(),
            editing_field: None,
        };
        let config = RenderConfig {
            width,
            form_state: Some(&form_state),
            ..Default::default()
        };
        let output = render_with_hitboxes(&doc, &config);
        self.hitboxes = output.hitboxes;
        self.render_width = width;

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
        if self.is_loading() {
            return BrowserAction::None;
        }
        let Some(prev) = self.back_stack.pop() else {
            return BrowserAction::None;
        };
        if let Some(current) = self.current.take() {
            self.forward_stack.push(current);
        }
        let url = prev.url.clone();
        let has_content = !prev.content.is_empty();
        self.current = Some(prev);
        self.error = None;
        if has_content {
            if let Some(ref entry) = self.current {
                self.rebuild_cache(&entry.content.clone(), self.render_width);
            }
            BrowserAction::None
        } else {
            self.pending_url = Some(url.clone());
            self.loading = LoadingState::Connecting;
            BrowserAction::Navigate {
                url,
                form_data: HashMap::new(),
            }
        }
    }

    pub fn go_forward(&mut self) -> BrowserAction {
        if self.is_loading() {
            return BrowserAction::None;
        }
        let Some(next) = self.forward_stack.pop() else {
            return BrowserAction::None;
        };
        if let Some(current) = self.current.take() {
            self.back_stack.push(current);
        }
        let url = next.url.clone();
        let has_content = !next.content.is_empty();
        self.current = Some(next);
        self.error = None;
        if has_content {
            if let Some(ref entry) = self.current {
                self.rebuild_cache(&entry.content.clone(), self.render_width);
            }
            BrowserAction::None
        } else {
            self.pending_url = Some(url.clone());
            self.loading = LoadingState::Connecting;
            BrowserAction::Navigate {
                url,
                form_data: HashMap::new(),
            }
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
            HitboxTarget::Link { url, fields } => {
                let form_data = self.collect_form_data(fields);
                BrowserAction::Navigate {
                    url: url.clone(),
                    form_data,
                }
            }
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

    fn collect_form_data(&self, field_specs: &[String]) -> HashMap<String, String> {
        let mut data = HashMap::new();

        if field_specs.is_empty() {
            return data;
        }

        let include_all = field_specs.iter().any(|f| f == "*");
        let mut requested_fields: Vec<&str> = Vec::new();

        for spec in field_specs {
            if let Some((key, value)) = spec.split_once('=') {
                data.insert(format!("var_{}", key), value.to_string());
            } else if spec != "*" {
                requested_fields.push(spec);
            }
        }

        for (name, value) in &self.field_values {
            if include_all || requested_fields.iter().any(|f| f == name) {
                data.insert(format!("field_{}", name), value.clone());
            }
        }

        for (name, checked) in &self.checkbox_states {
            if *checked && (include_all || requested_fields.iter().any(|f| f == name)) {
                let value = self.get_checkbox_value(name);
                let key = format!("field_{}", name);
                if let Some(existing) = data.get(&key) {
                    data.insert(key, format!("{},{}", existing, value));
                } else {
                    data.insert(key, value);
                }
            }
        }

        for (name, value) in &self.radio_states {
            if include_all || requested_fields.iter().any(|f| f == name) {
                data.insert(format!("field_{}", name), value.clone());
            }
        }

        data
    }

    fn get_checkbox_value(&self, name: &str) -> String {
        for hitbox in &self.hitboxes {
            if let HitboxTarget::Checkbox {
                name: field_name, ..
            } = &hitbox.target
            {
                if field_name == name {
                    return "1".to_string();
                }
            }
        }
        "1".to_string()
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
        if self.current.is_none() {
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

        for (idx, hitbox) in self.hitboxes.iter().enumerate() {
            if hitbox.line == doc_line && doc_col >= hitbox.col_start && doc_col < hitbox.col_end {
                self.selected = idx;
                return self.activate();
            }
        }

        BrowserAction::None
    }

    pub fn selected_info(&self) -> Option<&str> {
        let hitbox = self.hitboxes.get(self.selected)?;
        match &hitbox.target {
            HitboxTarget::Link { url, .. } => Some(url),
            HitboxTarget::TextField { name, .. } => Some(name),
            HitboxTarget::Checkbox { name } => Some(name),
            HitboxTarget::Radio { name, .. } => Some(name),
        }
    }

    pub fn selected_link_url(&self) -> Option<&str> {
        let hitbox = self.hitboxes.get(self.selected)?;
        match &hitbox.target {
            HitboxTarget::Link { url, .. } => Some(url),
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
            editing_field: None,
        }
    }

    pub fn render_content(&mut self, area: Rect, buf: &mut Buffer) {
        if self.current.is_some() {
            if area.width != self.render_width {
                if let Some(ref entry) = self.current {
                    let content = entry.content.clone();
                    self.rebuild_cache(&content, area.width);
                }
            }

            let content = self.build_page_content(area.width);
            let scroll = self.current.as_ref().map(|e| e.scroll).unwrap_or(0);
            Paragraph::new(content)
                .scroll((scroll, 0))
                .render(area, buf);

            if self.debug_hitboxes {
                self.render_hitbox_debug(area, buf, scroll);
            }

            if self.is_loading() {
                self.render_loading_overlay(area, buf);
            } else if let Some(ref err) = self.error {
                self.render_error_overlay(area, buf, err);
            }
        } else {
            let content = self.build_empty_content();
            Paragraph::new(content)
                .alignment(ratatui::layout::Alignment::Center)
                .render(area, buf);
        }
    }

    fn render_hitbox_debug(&self, area: Rect, buf: &mut Buffer, scroll: u16) {
        let scroll = scroll as usize;
        let height = area.height as usize;

        for (idx, hitbox) in self.hitboxes.iter().enumerate() {
            if hitbox.line < scroll || hitbox.line >= scroll + height {
                continue;
            }

            let screen_y = area.y + (hitbox.line - scroll) as u16;
            let is_selected = idx == self.selected;

            let bg = if is_selected {
                Color::Yellow
            } else {
                match &hitbox.target {
                    HitboxTarget::Link { .. } => Color::Red,
                    HitboxTarget::TextField { .. } => Color::Green,
                    HitboxTarget::Checkbox { .. } => Color::Blue,
                    HitboxTarget::Radio { .. } => Color::Magenta,
                }
            };

            for col in hitbox.col_start..hitbox.col_end {
                let screen_x = area.x + col as u16;
                if screen_x < area.x + area.width {
                    if let Some(cell) = buf.cell_mut((screen_x, screen_y)) {
                        cell.set_bg(bg);
                    }
                }
            }
        }
    }

    fn build_page_content(&self, width: u16) -> Text<'static> {
        if let Some(ref doc) = self.cached_doc {
            let editing_field = self.editing_field.and_then(|idx| {
                self.hitboxes.get(idx).and_then(|hb| match &hb.target {
                    HitboxTarget::TextField { name, .. } => Some(name.clone()),
                    _ => None,
                })
            });

            let form_state = FormState {
                fields: self.field_values.clone(),
                checkboxes: self.checkbox_states.clone(),
                radios: self.radio_states.clone(),
                editing_field,
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

    fn build_empty_content(&self) -> Text<'static> {
        if self.is_loading() {
            let msg = match self.loading {
                LoadingState::Connecting => "Connecting...",
                LoadingState::Retrieving => "Retrieving...",
                LoadingState::Idle => unreachable!(),
            };
            Text::from(vec![
                Line::from(""),
                Line::from(Span::styled(msg, Style::default().fg(Color::Yellow))),
            ])
        } else if let Some(ref err) = self.error {
            Text::from(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "!",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(err.clone(), Style::default().fg(Color::Red))),
            ])
        } else {
            Text::from(vec![
                Line::from(""),
                Line::from(""),
                Line::from(Span::styled(
                    "No page loaded",
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        }
    }

    fn render_loading_overlay(&self, area: Rect, buf: &mut Buffer) {
        let msg = match self.loading {
            LoadingState::Connecting => " Connecting... ",
            LoadingState::Retrieving => " Retrieving... ",
            LoadingState::Idle => return,
        };
        let x = area.x + area.width.saturating_sub(msg.len() as u16 + 1);
        let y = area.y;
        buf.set_string(
            x,
            y,
            msg,
            Style::default().fg(Color::Black).bg(Color::Yellow),
        );
    }

    fn render_error_overlay(&self, area: Rect, buf: &mut Buffer, err: &str) {
        let msg = format!(" {} ", err);
        let max_len = area.width.saturating_sub(2) as usize;
        let msg = if msg.len() > max_len {
            format!("{}...", &msg[..max_len.saturating_sub(3)])
        } else {
            msg
        };
        let x = area.x + area.width.saturating_sub(msg.len() as u16 + 1);
        let y = area.y;
        buf.set_string(x, y, &msg, Style::default().fg(Color::White).bg(Color::Red));
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
        assert_eq!(browser.loading_state(), LoadingState::Idle);
        assert!(browser.current_url().is_none());
        assert!(!browser.can_go_back());
        assert!(!browser.can_go_forward());
    }

    #[test]
    fn navigate_sets_connecting() {
        let mut browser = Browser::new();
        let req = browser.navigate("abc123:page.mu".into());
        assert!(matches!(req, BrowserAction::Navigate { .. }));
        assert_eq!(browser.loading_state(), LoadingState::Connecting);
        assert!(browser.current_url().is_none());
    }

    #[test]
    fn set_content_transitions_to_loaded() {
        let mut browser = Browser::new();
        browser.navigate("abc123:page.mu".into());
        browser.set_content("abc123:page.mu", "Hello".into(), 80);
        assert_eq!(browser.loading_state(), LoadingState::Idle);
        assert_eq!(browser.current_url(), Some("abc123:page.mu"));
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

        assert!(matches!(req, BrowserAction::Navigate { url, .. } if url == "http://target"));
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

    #[test]
    fn keeps_current_page_during_load() {
        let mut browser = Browser::new();
        browser.navigate("abc:page1.mu".into());
        browser.set_content("abc:page1.mu", "Page 1 content".into(), 80);
        assert_eq!(browser.current_url(), Some("abc:page1.mu"));

        browser.navigate("abc:page2.mu".into());
        assert!(browser.is_loading());
        assert_eq!(browser.current_url(), Some("abc:page1.mu"));

        browser.set_content("abc:page2.mu", "Page 2 content".into(), 80);
        assert!(!browser.is_loading());
        assert_eq!(browser.current_url(), Some("abc:page2.mu"));
    }

    #[test]
    fn failed_load_keeps_current_page() {
        let mut browser = Browser::new();
        browser.navigate("abc:page1.mu".into());
        browser.set_content("abc:page1.mu", "Page 1 content".into(), 80);

        browser.navigate("abc:page2.mu".into());
        browser.set_failed("abc:page2.mu", "Connection failed".into());

        assert!(!browser.is_loading());
        assert_eq!(browser.current_url(), Some("abc:page1.mu"));
        assert_eq!(browser.error(), Some("Connection failed"));
    }

    #[test]
    fn form_data_collection() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content(
            "abc:page.mu",
            "`<|username`Enter name>\n`<|message`Enter message>\n`[Submit`:/submit`username|message|action=send]".into(),
            80,
        );

        browser.activate();
        browser.handle_text_input('J');
        browser.handle_text_input('o');
        browser.handle_text_input('e');
        browser.cancel_edit();

        browser.select_next();
        browser.activate();
        browser.handle_text_input('H');
        browser.handle_text_input('i');
        browser.cancel_edit();

        browser.select_next();
        let action = browser.activate();

        if let BrowserAction::Navigate { url, form_data } = action {
            assert_eq!(url, ":/submit");
            assert_eq!(form_data.get("field_username"), Some(&"Joe".to_string()));
            assert_eq!(form_data.get("field_message"), Some(&"Hi".to_string()));
            assert_eq!(form_data.get("var_action"), Some(&"send".to_string()));
        } else {
            panic!("Expected Navigate action");
        }
    }

    #[test]
    fn form_data_wildcard() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content(
            "abc:page.mu",
            "`<|name`Name>\n`<|email`Email>\n`[Submit`:/submit`*]".into(),
            80,
        );

        browser.activate();
        browser.handle_text_input('A');
        browser.cancel_edit();

        browser.select_next();
        browser.activate();
        browser.handle_text_input('B');
        browser.cancel_edit();

        browser.select_next();
        let action = browser.activate();

        if let BrowserAction::Navigate { form_data, .. } = action {
            assert_eq!(form_data.get("field_name"), Some(&"A".to_string()));
            assert_eq!(form_data.get("field_email"), Some(&"B".to_string()));
        } else {
            panic!("Expected Navigate action");
        }
    }
}
