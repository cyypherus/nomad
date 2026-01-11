use crate::browser::{Browser, BrowserAction, InputResult};
use crate::network::NodeInfo;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    NodeList,
    BrowserView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftPanelMode {
    Nodes,
    Announces,
}

pub struct NetworkView {
    nodes: Vec<NodeInfo>,
    announces: Vec<NodeInfo>,
    selected: usize,
    list_offset: usize,
    left_mode: LeftPanelMode,
    focus: FocusArea,

    browser: Browser,
    current_node: Option<NodeInfo>,

    our_lxmf_addr: [u8; 16],
    our_name: String,
    last_announce_secs: u64,

    last_content_area: Rect,
    last_list_height: usize,
}

impl NetworkView {
    pub fn new(our_lxmf_addr: [u8; 16], nodes: Vec<NodeInfo>) -> Self {
        Self {
            nodes,
            announces: Vec::new(),
            selected: 0,
            list_offset: 0,
            left_mode: LeftPanelMode::Nodes,
            focus: FocusArea::NodeList,
            browser: Browser::new(),
            current_node: None,
            our_lxmf_addr,
            our_name: "Anonymous Peer".to_string(),
            last_announce_secs: 0,
            last_content_area: Rect::default(),
            last_list_height: 0,
        }
    }

    pub fn add_node(&mut self, node: NodeInfo) {
        if let Some(existing) = self.announces.iter_mut().find(|n| n.hash == node.hash) {
            existing.name = node.name.clone();
            existing.identity = node.identity.clone();
        } else {
            self.announces.push(node.clone());
        }

        if let Some(existing) = self.nodes.iter_mut().find(|n| n.hash == node.hash) {
            existing.name = node.name;
            existing.identity = node.identity;
        } else {
            self.nodes.push(node);
        }
    }

    pub fn toggle_left_mode(&mut self) {
        self.left_mode = match self.left_mode {
            LeftPanelMode::Nodes => LeftPanelMode::Announces,
            LeftPanelMode::Announces => LeftPanelMode::Nodes,
        };
        self.selected = 0;
        self.list_offset = 0;
    }

    pub fn select_next(&mut self) {
        let len = self.current_list_len();
        if len > 0 {
            self.selected = (self.selected + 1) % len;
            self.adjust_list_scroll();
        }
    }

    pub fn select_prev(&mut self) {
        let len = self.current_list_len();
        if len > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(len - 1);
            self.adjust_list_scroll();
        }
    }

    fn adjust_list_scroll(&mut self) {
        if self.last_list_height == 0 {
            return;
        }
        if self.selected < self.list_offset {
            self.list_offset = self.selected;
        } else if self.selected >= self.list_offset + self.last_list_height {
            self.list_offset = self.selected - self.last_list_height + 1;
        }
    }

    fn current_list_len(&self) -> usize {
        match self.left_mode {
            LeftPanelMode::Nodes => self.nodes.len(),
            LeftPanelMode::Announces => self.announces.len(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.announces.len()
    }

    pub fn set_last_announce(&mut self, secs: u64) {
        self.last_announce_secs = secs;
    }

    pub fn update_announce_time(&mut self) {
        self.last_announce_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }

    pub fn focus(&self) -> FocusArea {
        self.focus
    }

    pub fn set_focus(&mut self, focus: FocusArea) {
        self.focus = focus;
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            FocusArea::NodeList => FocusArea::BrowserView,
            FocusArea::BrowserView => FocusArea::NodeList,
        };
    }

    pub fn connect_selected(&mut self) -> Option<(NodeInfo, String)> {
        let node = match self.left_mode {
            LeftPanelMode::Nodes => self.nodes.get(self.selected)?.clone(),
            LeftPanelMode::Announces => self.announces.get(self.selected)?.clone(),
        };

        let path = "/page/index.mu".to_string();
        let url = format!("{}:{}", node.hash_hex(), path);
        self.current_node = Some(node.clone());
        self.browser.navigate(url);
        self.focus = FocusArea::BrowserView;

        Some((node, path))
    }

    pub fn navigate_to_link(
        &mut self,
        link_url: &str,
        form_data: HashMap<String, String>,
    ) -> Option<(NodeInfo, String, HashMap<String, String>)> {
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
                return Some((node, path, form_data));
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

                        let node = self
                            .nodes
                            .iter()
                            .find(|n| n.hash == hash)
                            .cloned()
                            .or_else(|| self.announces.iter().find(|n| n.hash == hash).cloned())
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
                            return Some((node, path, form_data));
                        } else {
                            log::warn!("Cannot navigate to unknown node: {}", hash_hex);
                            return None;
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
            return Some((node, path, form_data));
        }

        None
    }

    pub fn current_node(&self) -> Option<&NodeInfo> {
        self.current_node.as_ref()
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

    pub fn browser(&self) -> &Browser {
        &self.browser
    }

    pub fn browser_mut(&mut self) -> &mut Browser {
        &mut self.browser
    }

    pub fn handle_browser_action(
        &mut self,
        action: BrowserAction,
    ) -> Option<(String, HashMap<String, String>)> {
        match action {
            BrowserAction::Navigate { url, form_data } => Some((url, form_data)),
            BrowserAction::None => None,
        }
    }

    pub fn browser_scroll_up(&mut self) {
        self.browser.scroll_up();
    }

    pub fn browser_scroll_down(&mut self) {
        self.browser.scroll_down();
    }

    pub fn browser_scroll_page_up(&mut self) {
        self.browser.scroll_page_up(self.last_content_area.height);
    }

    pub fn browser_scroll_page_down(&mut self) {
        self.browser.scroll_page_down(self.last_content_area.height);
    }

    pub fn browser_select_next(&mut self) {
        self.browser.select_next();
    }

    pub fn browser_select_prev(&mut self) {
        self.browser.select_prev();
    }

    pub fn browser_activate(&mut self) -> Option<(String, HashMap<String, String>)> {
        let action = self.browser.activate();
        self.handle_browser_action(action)
    }

    pub fn browser_go_back(&mut self) -> Option<(String, HashMap<String, String>)> {
        let action = self.browser.go_back();
        self.handle_browser_action(action)
    }

    pub fn browser_go_forward(&mut self) -> Option<(String, HashMap<String, String>)> {
        let action = self.browser.go_forward();
        self.handle_browser_action(action)
    }

    pub fn browser_click(&mut self, x: u16, y: u16) -> Option<(String, HashMap<String, String>)> {
        let action = self.browser.click(x, y, self.last_content_area);
        self.handle_browser_action(action)
    }

    pub fn browser_handle_char(&mut self, c: char) -> InputResult {
        self.browser.handle_text_input(c)
    }

    pub fn browser_handle_backspace(&mut self) -> InputResult {
        self.browser.handle_backspace()
    }

    pub fn browser_cancel_edit(&mut self) {
        self.browser.cancel_edit();
    }

    pub fn browser_is_editing(&self) -> bool {
        self.browser.is_editing()
    }

    pub fn browser_toggle_debug(&mut self) {
        self.browser.toggle_debug_hitboxes();
    }

    fn render_left_panel(&mut self, area: Rect, buf: &mut Buffer) {
        let title = match self.left_mode {
            LeftPanelMode::Nodes => "Saved Nodes",
            LeftPanelMode::Announces => "Announces",
        };

        let border_style = if self.focus == FocusArea::NodeList {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        self.last_list_height = inner.height as usize;

        let items: Vec<ListItem> = match self.left_mode {
            LeftPanelMode::Nodes => self
                .nodes
                .iter()
                .enumerate()
                .skip(self.list_offset)
                .take(inner.height as usize)
                .map(|(i, node)| {
                    let style = if i == self.selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("\u{24c3}  {}", node.name)).style(style)
                })
                .collect(),
            LeftPanelMode::Announces => self
                .announces
                .iter()
                .enumerate()
                .skip(self.list_offset)
                .take(inner.height as usize)
                .map(|(i, node)| {
                    let style = if i == self.selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(format!("\u{24c3}  {}", node.name)).style(style)
                })
                .collect(),
        };

        let list = List::new(items).block(block);
        Widget::render(list, area, buf);
    }

    fn render_viewer(&mut self, area: Rect, buf: &mut Buffer) {
        let title = self
            .current_node
            .as_ref()
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Remote Node".to_string());

        let border_style = if self.focus == FocusArea::BrowserView {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        Widget::render(block, area, buf);

        if inner.height < 3 {
            return;
        }

        let url_line = if let Some(url) = self.browser.current_url() {
            Line::from(vec![
                Span::styled("\u{24c3}  ", Style::default().fg(Color::Cyan)),
                Span::raw(url.to_string()),
            ])
        } else {
            Line::raw("")
        };

        let url_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };
        Paragraph::new(url_line).render(url_area, buf);

        let divider_area = Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: 1,
        };
        Paragraph::new("\u{2504}".repeat(inner.width as usize)).render(divider_area, buf);

        let content_area = Rect {
            x: inner.x,
            y: inner.y + 2,
            width: inner.width,
            height: inner.height.saturating_sub(2),
        };

        self.last_content_area = content_area;
        self.browser.render_content(content_area, buf);

        if let Some(link_url) = self.browser.selected_link_url() {
            if content_area.height > 1 {
                let status_y = content_area.y + content_area.height - 1;
                let status_area = Rect::new(content_area.x, status_y, content_area.width, 1);
                let status_text = format!("\u{2192} {}", link_url);
                Paragraph::new(status_text)
                    .style(Style::default().fg(Color::DarkGray))
                    .render(status_area, buf);
            }
        }
    }

    fn render_local_info(&self, area: Rect, buf: &mut Buffer) {
        let addr_hex = hex::encode(self.our_lxmf_addr);

        let announce_status = if self.last_announce_secs > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let elapsed = now.saturating_sub(self.last_announce_secs);
            if elapsed < 60 {
                format!("{}s ago", elapsed)
            } else if elapsed < 3600 {
                format!("{}m ago", elapsed / 60)
            } else {
                format!("{}h ago", elapsed / 3600)
            }
        } else {
            "never".to_string()
        };

        let info = vec![
            Line::from(vec![Span::styled(
                "LXMF Addr : ",
                Style::default().fg(Color::DarkGray),
            )]),
            Line::from(vec![Span::styled(
                format!("<{}>", addr_hex),
                Style::default().fg(Color::Cyan),
            )]),
            Line::from(vec![
                Span::styled("Name      : ", Style::default().fg(Color::DarkGray)),
                Span::raw(&self.our_name),
            ]),
            Line::raw("\u{2504}".repeat(34)),
            Line::from(vec![
                Span::styled("Announced : ", Style::default().fg(Color::DarkGray)),
                Span::raw(announce_status),
            ]),
            Line::styled(
                "< Announce Now >",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        let paragraph = Paragraph::new(info).block(
            Block::default()
                .title("Local Peer Info")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        Widget::render(paragraph, area, buf);
    }
}

impl Widget for &mut NetworkView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([Constraint::Length(36), Constraint::Min(20)])
            .split(area);

        let left_chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(10)])
            .split(chunks[0]);

        self.render_left_panel(left_chunks[0], buf);
        self.render_local_info(left_chunks[1], buf);
        self.render_viewer(chunks[1], buf);
    }
}
