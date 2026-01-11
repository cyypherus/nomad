use crate::network::NodeInfo;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ViewerState {
    Disconnected,
    Connecting,
    Retrieving,
    Connected,
    Failed,
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
    left_mode: LeftPanelMode,

    viewer_state: ViewerState,
    current_url: Option<String>,
    current_node_name: Option<String>,
    page_content: Option<String>,
    status_message: Option<String>,

    our_lxmf_addr: [u8; 16],
    our_name: String,
    last_announce_secs: u64,
}

impl NetworkView {
    pub fn new(our_lxmf_addr: [u8; 16], nodes: Vec<NodeInfo>) -> Self {
        Self {
            nodes,
            announces: Vec::new(),
            selected: 0,
            left_mode: LeftPanelMode::Nodes,
            viewer_state: ViewerState::Disconnected,
            current_url: None,
            current_node_name: None,
            page_content: None,
            status_message: None,
            our_lxmf_addr,
            our_name: "Anonymous Peer".to_string(),
            last_announce_secs: 0,
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
    }

    pub fn select_next(&mut self) {
        let len = self.current_list_len();
        if len > 0 {
            self.selected = (self.selected + 1) % len;
        }
    }

    pub fn select_prev(&mut self) {
        let len = self.current_list_len();
        if len > 0 {
            self.selected = self.selected.checked_sub(1).unwrap_or(len - 1);
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

    pub fn connect_selected(&mut self) -> Option<(NodeInfo, String)> {
        let node = match self.left_mode {
            LeftPanelMode::Nodes => self.nodes.get(self.selected)?.clone(),
            LeftPanelMode::Announces => self.announces.get(self.selected)?.clone(),
        };

        let path = "/page/index.mu".to_string();
        self.current_node_name = Some(node.name.clone());
        self.current_url = Some(format!("{}:{}", node.hash_hex(), path));
        self.viewer_state = ViewerState::Connecting;
        self.status_message = Some("Connecting...".to_string());
        self.page_content = None;

        Some((node, path))
    }

    pub fn set_page_content(&mut self, url: String, content: String) {
        if self.current_url.as_ref() == Some(&url) {
            self.page_content = Some(content);
            self.viewer_state = ViewerState::Connected;
            self.status_message = None;
        }
    }

    pub fn set_connection_failed(&mut self, url: String, reason: String) {
        if self.current_url.as_ref() == Some(&url) {
            self.viewer_state = ViewerState::Failed;
            self.status_message = Some(reason);
        }
    }

    pub fn set_retrieving(&mut self) {
        self.viewer_state = ViewerState::Retrieving;
        self.status_message = Some("Retrieving page...".to_string());
    }

    fn render_left_panel(&self, area: Rect, buf: &mut Buffer) {
        let title = match self.left_mode {
            LeftPanelMode::Nodes => "Saved Nodes",
            LeftPanelMode::Announces => "Announces",
        };

        let items: Vec<ListItem> = match self.left_mode {
            LeftPanelMode::Nodes => self
                .nodes
                .iter()
                .enumerate()
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

        let list = List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );

        Widget::render(list, area, buf);
    }

    fn render_viewer(&self, area: Rect, buf: &mut Buffer) {
        let title = self
            .current_node_name
            .clone()
            .unwrap_or_else(|| "Remote Node".to_string());

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        Widget::render(block, area, buf);

        if inner.height < 3 {
            return;
        }

        let url_line = if let Some(url) = &self.current_url {
            Line::from(vec![
                Span::styled("\u{24c3}  ", Style::default().fg(Color::Cyan)),
                Span::raw(url),
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

        match self.viewer_state {
            ViewerState::Disconnected => {
                let msg = Paragraph::new(vec![
                    Line::raw(""),
                    Line::raw(""),
                    Line::styled(
                        "Disconnected",
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Line::styled(
                        "  \u{2190}  \u{2192}  ",
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
                .alignment(ratatui::layout::Alignment::Center);
                Widget::render(msg, content_area, buf);
            }
            ViewerState::Connecting | ViewerState::Retrieving => {
                let status = self
                    .status_message
                    .clone()
                    .unwrap_or_else(|| "Connecting...".to_string());
                let msg = Paragraph::new(vec![
                    Line::raw(""),
                    Line::raw(""),
                    Line::styled(
                        "\u{25cf}",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::SLOW_BLINK),
                    ),
                    Line::raw(""),
                    Line::styled(status, Style::default().fg(Color::Yellow)),
                ])
                .alignment(ratatui::layout::Alignment::Center);
                Widget::render(msg, content_area, buf);
            }
            ViewerState::Connected => {
                if let Some(content) = &self.page_content {
                    let doc = micron::parse(content);
                    let rendered = micron::render(&doc, &Default::default());
                    let paragraph = Paragraph::new(rendered).wrap(Wrap { trim: false });
                    Widget::render(paragraph, content_area, buf);
                }
            }
            ViewerState::Failed => {
                let reason = self
                    .status_message
                    .clone()
                    .unwrap_or_else(|| "Unknown error".to_string());
                let msg = Paragraph::new(vec![
                    Line::raw(""),
                    Line::raw(""),
                    Line::styled("!", Style::default().fg(Color::Red)),
                    Line::raw(""),
                    Line::styled("Request failed", Style::default().fg(Color::Red)),
                    Line::raw(""),
                    Line::styled(reason, Style::default().fg(Color::DarkGray)),
                ])
                .alignment(ratatui::layout::Alignment::Center);
                Widget::render(msg, content_area, buf);
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
            Line::raw("\u{2504}".repeat(12)),
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

impl Widget for &NetworkView {
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
