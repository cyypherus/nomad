use micron::{
    parse as parse_micron, render as render_micron, Document, Element, Link, RenderConfig,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Widget},
};

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
    selected_link: usize,
    status: Option<String>,
    cached_doc: Option<Document>,
    cached_links: Vec<LinkInfo>,
}

#[derive(Debug, Clone)]
struct LinkInfo {
    url: String,
    line: usize,
    col_start: usize,
    col_end: usize,
}

pub enum NavigateRequest {
    Fetch { url: String },
    None,
}

impl Browser {
    pub fn new() -> Self {
        Self {
            state: BrowserState::Empty,
            current: None,
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            selected_link: 0,
            status: None,
            cached_doc: None,
            cached_links: Vec::new(),
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

    pub fn navigate(&mut self, url: String) -> NavigateRequest {
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
        self.cached_doc = None;
        self.cached_links.clear();
        self.selected_link = 0;
        NavigateRequest::Fetch { url }
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
        self.cached_links = extract_links(&doc);
        self.cached_doc = Some(doc);
        self.selected_link = 0;
    }

    pub fn go_back(&mut self) -> NavigateRequest {
        let Some(prev) = self.back_stack.pop() else {
            return NavigateRequest::None;
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
            NavigateRequest::None
        } else {
            self.state = BrowserState::Connecting;
            self.status = Some("Connecting...".into());
            NavigateRequest::Fetch { url }
        }
    }

    pub fn go_forward(&mut self) -> NavigateRequest {
        let Some(next) = self.forward_stack.pop() else {
            return NavigateRequest::None;
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
            NavigateRequest::None
        } else {
            self.state = BrowserState::Connecting;
            self.status = Some("Connecting...".into());
            NavigateRequest::Fetch { url }
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

    pub fn select_next_link(&mut self) {
        if !self.cached_links.is_empty() {
            self.selected_link = (self.selected_link + 1) % self.cached_links.len();
            self.scroll_to_selected_link();
        }
    }

    pub fn select_prev_link(&mut self) {
        if !self.cached_links.is_empty() {
            self.selected_link = self
                .selected_link
                .checked_sub(1)
                .unwrap_or(self.cached_links.len() - 1);
            self.scroll_to_selected_link();
        }
    }

    fn scroll_to_selected_link(&mut self) {
        if let Some(link) = self.cached_links.get(self.selected_link) {
            if let Some(ref mut entry) = self.current {
                entry.scroll = link.line.saturating_sub(2) as u16;
            }
        }
    }

    pub fn activate_link(&mut self) -> NavigateRequest {
        let Some(link) = self.cached_links.get(self.selected_link) else {
            return NavigateRequest::None;
        };
        let url = link.url.clone();
        self.navigate(url)
    }

    pub fn selected_link_url(&self) -> Option<&str> {
        self.cached_links
            .get(self.selected_link)
            .map(|l| l.url.as_str())
    }

    pub fn link_count(&self) -> usize {
        self.cached_links.len()
    }

    pub fn click(&mut self, x: u16, y: u16, content_area: Rect) -> NavigateRequest {
        if self.state != BrowserState::Loaded {
            return NavigateRequest::None;
        }

        let scroll = self.current.as_ref().map(|e| e.scroll).unwrap_or(0);

        if x < content_area.x || x >= content_area.x + content_area.width {
            return NavigateRequest::None;
        }
        if y < content_area.y || y >= content_area.y + content_area.height {
            return NavigateRequest::None;
        }

        let rel_y = y - content_area.y;
        let rel_x = x - content_area.x;
        let doc_line = (rel_y + scroll) as usize;
        let doc_col = rel_x as usize;

        for (idx, link) in self.cached_links.iter().enumerate() {
            if link.line == doc_line && doc_col >= link.col_start && doc_col < link.col_end {
                self.selected_link = idx;
                return self.activate_link();
            }
        }

        NavigateRequest::None
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

fn extract_links(doc: &Document) -> Vec<LinkInfo> {
    let mut links = Vec::new();
    for (line_idx, line) in doc.lines.iter().enumerate() {
        let mut col = 0usize;
        for element in &line.elements {
            match element {
                Element::Link(Link { url, label, .. }) => {
                    let len = label.chars().count();
                    links.push(LinkInfo {
                        url: url.clone(),
                        line: line_idx,
                        col_start: col,
                        col_end: col + len,
                    });
                    col += len;
                }
                Element::Text(t) => {
                    col += t.text.chars().count();
                }
                Element::Field(f) => {
                    col += f.width.unwrap_or(24) as usize;
                }
                Element::Partial(_) => {}
            }
        }
    }
    links
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
        assert!(matches!(req, NavigateRequest::Fetch { .. }));
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

        assert_eq!(browser.link_count(), 2);
    }

    #[test]
    fn click_on_link() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "`[Click Me`http://target]".into());

        assert_eq!(browser.link_count(), 1);

        let area = Rect::new(0, 0, 80, 24);
        let req = browser.click(3, 0, area);

        assert!(matches!(req, NavigateRequest::Fetch { url } if url == "http://target"));
    }

    #[test]
    fn click_outside_link() {
        let mut browser = Browser::new();
        browser.navigate("abc:page.mu".into());
        browser.set_content("abc:page.mu", "Some text `[Link`http://x]".into());

        let area = Rect::new(0, 0, 80, 24);
        let req = browser.click(0, 0, area);

        assert!(matches!(req, NavigateRequest::None));
    }
}
