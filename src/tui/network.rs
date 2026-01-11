use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedNode {
    #[serde(with = "hex_bytes_16")]
    pub hash: [u8; 16],
    pub name: String,
    #[serde(with = "hex_bytes_32")]
    pub public_key: [u8; 32],
    #[serde(with = "hex_bytes_32")]
    pub verifying_key: [u8; 32],
}

mod hex_bytes_16 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 16], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 16], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut arr = [0u8; 16];
        if bytes.len() != 16 {
            return Err(serde::de::Error::custom("expected 16 bytes"));
        }
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

mod hex_bytes_32 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        let mut arr = [0u8; 32];
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("expected 32 bytes"));
        }
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NodesFile {
    nodes: Vec<SavedNode>,
}

#[derive(Debug, Clone)]
pub struct AnnounceEntry {
    pub hash: [u8; 16],
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ViewerState {
    Disconnected,
    Connecting,
    Retrieving,
    Connected,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftPanelMode {
    Nodes,
    Announces,
}

pub struct NetworkView {
    saved_nodes: Vec<SavedNode>,
    announces: Vec<AnnounceEntry>,
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
    pub fn new(our_lxmf_addr: [u8; 16]) -> Self {
        let saved_nodes = Self::load_nodes().unwrap_or_default();
        Self {
            saved_nodes,
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

    fn nodes_path() -> std::path::PathBuf {
        Path::new(".nomad").join("nodes.toml")
    }

    fn load_nodes() -> Option<Vec<SavedNode>> {
        let path = Self::nodes_path();
        let contents = fs::read_to_string(&path).ok()?;
        let file: NodesFile = toml::from_str(&contents).ok()?;
        Some(file.nodes)
    }

    fn save_nodes(&self) {
        let file = NodesFile {
            nodes: self.saved_nodes.clone(),
        };
        if let Ok(contents) = toml::to_string_pretty(&file) {
            let _ = fs::write(Self::nodes_path(), contents);
        }
    }

    pub fn add_announce(
        &mut self,
        hash: [u8; 16],
        name: Option<String>,
        public_key: [u8; 32],
        verifying_key: [u8; 32],
    ) {
        if let Some(existing) = self.announces.iter_mut().find(|a| a.hash == hash) {
            if name.is_some() {
                existing.name = name.clone();
            }
        } else {
            self.announces.push(AnnounceEntry {
                hash,
                name: name.clone(),
            });
        }

        if let Some(node_name) = name {
            if let Some(existing) = self.saved_nodes.iter_mut().find(|n| n.hash == hash) {
                existing.name = node_name;
                existing.public_key = public_key;
                existing.verifying_key = verifying_key;
            } else {
                self.saved_nodes.push(SavedNode {
                    hash,
                    name: node_name,
                    public_key,
                    verifying_key,
                });
            }
            self.save_nodes();
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
            LeftPanelMode::Nodes => self.saved_nodes.len(),
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
        self.last_announce_secs = 0;
    }

    pub fn connect_selected(
        &mut self,
    ) -> Option<([u8; 16], String, Option<[u8; 32]>, Option<[u8; 32]>)> {
        let (hash, name, public_key, verifying_key) = match self.left_mode {
            LeftPanelMode::Nodes => {
                let node = self.saved_nodes.get(self.selected)?;
                (
                    node.hash,
                    Some(node.name.clone()),
                    Some(node.public_key),
                    Some(node.verifying_key),
                )
            }
            LeftPanelMode::Announces => {
                let entry = self.announces.get(self.selected)?;
                (entry.hash, entry.name.clone(), None, None)
            }
        };

        let path = "/page/index.mu".to_string();
        self.current_node_name = name;
        self.current_url = Some(format!("{}:{}", hex::encode(hash), path));
        self.viewer_state = ViewerState::Connecting;
        self.status_message = Some("Connecting...".to_string());
        self.page_content = None;

        Some((hash, path, public_key, verifying_key))
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
        if self.viewer_state == ViewerState::Connecting {
            self.viewer_state = ViewerState::Retrieving;
            self.status_message = Some("Retrieving page...".to_string());
        }
    }

    fn render_saved_nodes(&self, area: Rect, buf: &mut Buffer) {
        let title = match self.left_mode {
            LeftPanelMode::Nodes => "Saved Nodes",
            LeftPanelMode::Announces => "Announces",
        };

        let items: Vec<ListItem> = match self.left_mode {
            LeftPanelMode::Nodes => self
                .saved_nodes
                .iter()
                .enumerate()
                .map(|(i, node)| {
                    let style = if i == self.selected {
                        Style::default()
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled("Ⓝ  ", Style::default().fg(Color::Cyan)),
                        Span::raw(&node.name),
                    ]))
                    .style(style)
                })
                .collect(),
            LeftPanelMode::Announces => self
                .announces
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    let style = if i == self.selected {
                        Style::default()
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    let display = match &entry.name {
                        Some(name) => name.clone(),
                        None => {
                            let hash_str = hex::encode(entry.hash);
                            hash_str[..16].to_string()
                        }
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled("Ⓝ  ", Style::default().fg(Color::Yellow)),
                        Span::raw(display),
                    ]))
                    .style(style)
                })
                .collect(),
        };

        let list = if items.is_empty() {
            let msg = match self.left_mode {
                LeftPanelMode::Nodes => "No saved nodes",
                LeftPanelMode::Announces => "No announces yet...",
            };
            List::new(vec![ListItem::new(msg)])
                .block(Block::default().borders(Borders::ALL).title(title))
                .style(Style::default().fg(Color::DarkGray))
        } else {
            List::new(items).block(Block::default().borders(Borders::ALL).title(title))
        };

        list.render(area, buf);
    }

    fn render_local_info(&self, area: Rect, buf: &mut Buffer) {
        let announce_ago = if self.last_announce_secs == 0 {
            "never".to_string()
        } else if self.last_announce_secs < 60 {
            format!("{} seconds ago", self.last_announce_secs)
        } else {
            format!("{} minutes ago", self.last_announce_secs / 60)
        };

        let text = vec![
            Line::from(vec![
                Span::raw("LXMF Addr : <"),
                Span::styled(
                    hex::encode(self.our_lxmf_addr),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(">"),
            ]),
            Line::from(vec![Span::raw("Name      : "), Span::raw(&self.our_name)]),
            Line::from("┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄"),
            Line::from(vec![Span::raw("Announced : "), Span::raw(announce_ago)]),
            Line::from(vec![
                Span::styled(
                    "< Announce Now",
                    Style::default().fg(Color::White).bg(Color::DarkGray),
                ),
                Span::raw(" >"),
            ]),
        ];

        let para = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Local Peer Info"),
            )
            .wrap(Wrap { trim: false });

        para.render(area, buf);
    }

    fn render_viewer(&self, area: Rect, buf: &mut Buffer) {
        let title = self.current_node_name.as_deref().unwrap_or("Remote Node");

        let block = Block::default().borders(Borders::ALL).title(title);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 {
            return;
        }

        let url_line = if let Some(url) = &self.current_url {
            Line::from(vec![
                Span::styled("Ⓝ  ", Style::default().fg(Color::Cyan)),
                Span::raw(url.as_str()),
            ])
        } else {
            Line::from("")
        };

        let url_para = Paragraph::new(url_line);
        let url_area = Rect::new(inner.x, inner.y, inner.width, 1);
        url_para.render(url_area, buf);

        let divider = "┄".repeat(inner.width as usize);
        let divider_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        Paragraph::new(divider).render(divider_area, buf);

        let content_area = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );

        let content: Vec<Line> = match self.viewer_state {
            ViewerState::Disconnected => {
                vec![
                    Line::from(""),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Disconnected",
                        Style::default().fg(Color::DarkGray),
                    )),
                    Line::from(Span::styled("←  →", Style::default().fg(Color::DarkGray))),
                ]
            }
            ViewerState::Connecting => {
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "Connecting...",
                        Style::default().fg(Color::Yellow),
                    )),
                ]
            }
            ViewerState::Retrieving => {
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "Retrieving",
                        Style::default().fg(Color::Yellow),
                    )),
                    Line::from(format!("[{}]", self.current_url.as_deref().unwrap_or(""))),
                ]
            }
            ViewerState::Connected => {
                if let Some(content) = &self.page_content {
                    content.lines().map(|l| Line::from(l.to_string())).collect()
                } else {
                    vec![Line::from("(empty page)")]
                }
            }
            ViewerState::Failed => {
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "!",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Request failed",
                        Style::default().fg(Color::Red),
                    )),
                ]
            }
            ViewerState::TimedOut => {
                vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "!",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Request timed out",
                        Style::default().fg(Color::Red),
                    )),
                ]
            }
        };

        let para = Paragraph::new(content).alignment(ratatui::layout::Alignment::Center);
        para.render(content_area, buf);

        if let Some(msg) = &self.status_message {
            if content_area.height > 0 {
                let status_y = content_area.y + content_area.height - 1;
                let status_area = Rect::new(content_area.x, status_y, content_area.width, 1);
                let divider = "┄".repeat(content_area.width as usize);
                Paragraph::new(divider).render(status_area, buf);
                if status_y > content_area.y {
                    let msg_area = Rect::new(content_area.x, status_y, content_area.width, 1);
                    Paragraph::new(msg.as_str()).render(msg_area, buf);
                }
            }
        }
    }
}

impl Widget for &NetworkView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);

        let left_chunks =
            Layout::vertical([Constraint::Min(8), Constraint::Length(9)]).split(chunks[0]);

        self.render_saved_nodes(left_chunks[0], buf);
        self.render_local_info(left_chunks[1], buf);
        self.render_viewer(chunks[1], buf);
    }
}
