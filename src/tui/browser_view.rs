use crate::browser::{Browser, BrowserAction, LoadingState};
use crate::network::NodeInfo;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

pub struct BrowserView {
    browser: Browser,
    current_node: Option<NodeInfo>,
    last_content_area: Rect,
}

impl Default for BrowserView {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserView {
    pub fn new() -> Self {
        Self {
            browser: Browser::new(),
            current_node: None,
            last_content_area: Rect::default(),
        }
    }

    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    pub fn browser_mut(&mut self) -> &mut Browser {
        &mut self.browser
    }

    pub fn current_node(&self) -> Option<&NodeInfo> {
        self.current_node.as_ref()
    }

    pub fn set_current_node(&mut self, node: NodeInfo) {
        self.current_node = Some(node);
    }

    pub fn navigate(&mut self, node: &NodeInfo, path: &str) -> BrowserAction {
        let url = format!("{}:{}", node.hash_hex(), path);
        self.current_node = Some(node.clone());
        self.browser.navigate(url)
    }

    pub fn set_page_content(&mut self, url: String, content: String) {
        let width = if self.last_content_area.width > 0 {
            self.last_content_area.width
        } else {
            80
        };
        self.browser.set_content(&url, content, width);
    }

    pub fn set_connection_failed(&mut self, url: String, reason: String) {
        self.browser.set_failed(&url, reason);
    }

    pub fn scroll_up(&mut self) {
        self.browser.scroll_up();
    }

    pub fn scroll_down(&mut self) {
        self.browser.scroll_down();
    }

    pub fn scroll_page_up(&mut self) {
        self.browser.scroll_page_up(self.last_content_area.height);
    }

    pub fn scroll_page_down(&mut self) {
        self.browser.scroll_page_down(self.last_content_area.height);
    }

    pub fn select_next(&mut self) {
        self.browser.select_next();
    }

    pub fn select_prev(&mut self) {
        self.browser.select_prev();
    }

    pub fn activate(&mut self) -> Option<(String, std::collections::HashMap<String, String>)> {
        match self.browser.activate() {
            BrowserAction::Navigate { url, form_data } => Some((url, form_data)),
            BrowserAction::None => None,
        }
    }

    pub fn go_back(&mut self) -> Option<(String, std::collections::HashMap<String, String>)> {
        match self.browser.go_back() {
            BrowserAction::Navigate { url, form_data } => Some((url, form_data)),
            BrowserAction::None => None,
        }
    }

    pub fn click(
        &mut self,
        x: u16,
        y: u16,
    ) -> Option<(String, std::collections::HashMap<String, String>)> {
        match self.browser.click(x, y, self.last_content_area) {
            BrowserAction::Navigate { url, form_data } => Some((url, form_data)),
            BrowserAction::None => None,
        }
    }

    pub fn navigate_to_link(
        &mut self,
        link_url: &str,
        known_nodes: &[NodeInfo],
    ) -> Option<(NodeInfo, String)> {
        if let Some(rest) = link_url.strip_prefix(':') {
            if let Some(ref node) = self.current_node {
                let path = if rest.starts_with('/') {
                    rest.to_string()
                } else {
                    format!("/{}", rest)
                };
                let url = format!("{}:{}", node.hash_hex(), path);
                let node = node.clone();
                self.browser.navigate(url);
                return Some((node, path));
            }
            return None;
        }

        if link_url.contains(':') {
            let parts: Vec<&str> = link_url.splitn(2, ':').collect();
            if parts.len() == 2 && parts[0].len() == 32 {
                let hash_hex = parts[0];
                let path = parts[1].to_string();

                if let Ok(hash_bytes) = hex::decode(hash_hex) {
                    if hash_bytes.len() == 16 {
                        let mut hash = [0u8; 16];
                        hash.copy_from_slice(&hash_bytes);

                        let node = known_nodes
                            .iter()
                            .find(|n| n.hash == hash)
                            .cloned()
                            .or_else(|| {
                                self.current_node
                                    .as_ref()
                                    .filter(|n| n.hash == hash)
                                    .cloned()
                            });

                        if let Some(node) = node {
                            let url = format!("{}:{}", node.hash_hex(), path);
                            self.current_node = Some(node.clone());
                            self.browser.navigate(url);
                            return Some((node, path));
                        }
                    }
                }
            }
        }

        if let Some(ref node) = self.current_node {
            let path = if link_url.starts_with('/') {
                link_url.to_string()
            } else {
                format!("/{}", link_url)
            };
            let url = format!("{}:{}", node.hash_hex(), path);
            let node = node.clone();
            self.browser.navigate(url);
            return Some((node, path));
        }

        None
    }

    pub fn last_content_area(&self) -> Rect {
        self.last_content_area
    }
}

impl Widget for &mut BrowserView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self
            .current_node
            .as_ref()
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Browser".to_string());

        let title_color = if self.browser.error().is_some() {
            Color::Red
        } else {
            match self.browser.loading_state() {
                LoadingState::Idle => {
                    if self.browser.current_url().is_some() {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    }
                }
                LoadingState::Connecting | LoadingState::Retrieving => Color::Yellow,
            }
        };

        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    title,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(title_color));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 {
            return;
        }

        let url_line = if let Some(url) = self.browser.current_url() {
            Line::from(vec![
                Span::styled("\u{2192} ", Style::default().fg(Color::Cyan)),
                Span::styled(url.to_string(), Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from(Span::styled(
                "No page loaded",
                Style::default().fg(Color::DarkGray),
            ))
        };

        let url_area = Rect::new(inner.x, inner.y, inner.width, 1);
        Paragraph::new(url_line).render(url_area, buf);

        let divider_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        let divider = "\u{2500}".repeat(inner.width as usize);
        Paragraph::new(Line::from(Span::styled(
            divider,
            Style::default().fg(Color::DarkGray),
        )))
        .render(divider_area, buf);

        let content_area = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );

        self.last_content_area = content_area;
        self.browser.render_content(content_area, buf);

        if let Some(link_url) = self.browser.selected_link_url() {
            if content_area.height > 1 {
                let status_y = content_area.y + content_area.height - 1;
                let status_area = Rect::new(content_area.x, status_y, content_area.width, 1);
                let status_text = format!("\u{2192} {}", link_url);
                Paragraph::new(status_text)
                    .style(Style::default().fg(Color::DarkGray).bg(Color::Black))
                    .render(status_area, buf);
            }
        }
    }
}
