use crate::network::NodeInfo;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Widget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SavedModalAction {
    None,
    Connect,
    Delete,
    Copy,
    ToggleIdentify,
}

pub struct SavedView {
    nodes: Vec<NodeInfo>,
    list_state: ListState,
    last_height: usize,
    last_list_area: Rect,
    identify_button_area: Option<Rect>,
    connect_button_area: Option<Rect>,
    copy_button_area: Option<Rect>,
    delete_button_area: Option<Rect>,
}

impl Default for SavedView {
    fn default() -> Self {
        Self::new()
    }
}

impl SavedView {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            list_state: ListState::default(),
            last_height: 10,
            last_list_area: Rect::default(),
            identify_button_area: None,
            connect_button_area: None,
            copy_button_area: None,
            delete_button_area: None,
        }
    }

    fn selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn add_node(&mut self, node: NodeInfo) {
        if !self.nodes.iter().any(|n| n.hash == node.hash) {
            let pos = self
                .nodes
                .binary_search_by(|n| n.name.to_lowercase().cmp(&node.name.to_lowercase()))
                .unwrap_or_else(|p| p);
            self.nodes.insert(pos, node);
        }
    }

    pub fn select_by_hash(&mut self, hash: [u8; 16]) {
        if let Some(pos) = self.nodes.iter().position(|n| n.hash == hash) {
            self.list_state.select(Some(pos));
        }
    }

    pub fn nodes(&self) -> &[NodeInfo] {
        &self.nodes
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn selected_node(&self) -> Option<&NodeInfo> {
        self.nodes.get(self.selected())
    }

    pub fn select_next(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => (i + 1) % self.nodes.len(),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn select_prev(&mut self) {
        if self.nodes.is_empty() {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => i.checked_sub(1).unwrap_or(self.nodes.len() - 1),
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn scroll_up(&mut self) {
        self.list_state.scroll_up_by(1);
    }

    pub fn scroll_down(&mut self) {
        self.list_state.scroll_down_by(1);
    }

    pub fn click(&mut self, x: u16, y: u16, _area: Rect) -> Option<usize> {
        let list_inner = Rect::new(
            self.last_list_area.x + 1,
            self.last_list_area.y + 1,
            self.last_list_area.width.saturating_sub(2),
            self.last_list_area.height.saturating_sub(2),
        );

        if !list_inner.contains((x, y).into()) {
            return None;
        }

        let inner_y = y.saturating_sub(list_inner.y);
        let offset = self.list_state.offset();
        let idx = offset + inner_y as usize;

        if idx < self.nodes.len() {
            self.list_state.select(Some(idx));
            Some(idx)
        } else {
            None
        }
    }

    pub fn remove_selected(&mut self) -> Option<NodeInfo> {
        if self.nodes.is_empty() {
            return None;
        }
        let selected = self.selected();
        let removed = self.nodes.remove(selected);
        if selected >= self.nodes.len() && !self.nodes.is_empty() {
            self.list_state.select(Some(self.nodes.len() - 1));
        }
        Some(removed)
    }

    pub fn toggle_identify_selected(&mut self) -> Option<&NodeInfo> {
        let selected = self.selected();
        if let Some(node) = self.nodes.get_mut(selected) {
            node.identify = !node.identify;
            Some(node)
        } else {
            None
        }
    }

    pub fn set_identify(&mut self, hash: [u8; 16], enabled: bool) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.hash == hash) {
            node.identify = enabled;
        }
    }

    pub fn update_node_name(&mut self, hash: [u8; 16], name: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.hash == hash) {
            if node.name != name {
                node.name = name.to_string();
            }
        }
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        self.last_list_area = area;

        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(
                    " Saved Nodes ",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({}) ", self.nodes.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        self.last_height = inner.height as usize;
        block.render(area, buf);

        if self.nodes.is_empty() {
            let empty_lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No saved nodes yet",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Save nodes from Discovery to see them here",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            Paragraph::new(empty_lines)
                .alignment(ratatui::layout::Alignment::Center)
                .render(inner, buf);
            return;
        }

        let items: Vec<ListItem> = self
            .nodes
            .iter()
            .map(|node| {
                let hash_short = format!("{}..{}", &node.hash_hex()[..6], &node.hash_hex()[26..]);
                ListItem::new(Line::from(vec![
                    Span::styled(" \u{2022} ", Style::default().fg(Color::Green)),
                    Span::styled(&node.name, Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("  {}", hash_short),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("");

        ratatui::widgets::StatefulWidget::render(list, inner, buf, &mut self.list_state);
    }

    fn render_detail(&mut self, area: Rect, buf: &mut Buffer) {
        self.identify_button_area = None;
        self.connect_button_area = None;
        self.copy_button_area = None;
        self.delete_button_area = None;

        let block = Block::default()
            .title(Line::from(vec![Span::styled(
                " Node Info ",
                Style::default().fg(Color::White),
            )]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        let Some(node) = self.selected_node() else {
            let empty = Paragraph::new(Line::from(Span::styled(
                "Select a node to view details",
                Style::default().fg(Color::DarkGray),
            )))
            .alignment(ratatui::layout::Alignment::Center);
            empty.render(inner, buf);
            return;
        };

        let hash_hex = node.hash_hex();
        let identify_enabled = node.identify;

        let content = vec![
            Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &node.name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled("Hash:", Style::default().fg(Color::DarkGray))),
            Line::from(Span::styled(
                &hash_hex[..16],
                Style::default().fg(Color::Cyan),
            )),
            Line::from(Span::styled(
                &hash_hex[16..],
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
        ];

        Paragraph::new(content).render(inner, buf);

        // Self-Identify toggle
        let identify_y = inner.y + 7;
        let (identify_text, identify_style) = if identify_enabled {
            (
                " [x] Self-Identify ",
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (
                " [ ] Self-Identify ",
                Style::default().fg(Color::White).bg(Color::DarkGray),
            )
        };
        let identify_width = identify_text.len() as u16;
        buf.set_string(inner.x, identify_y, identify_text, identify_style);
        self.identify_button_area = Some(Rect::new(inner.x, identify_y, identify_width, 1));

        // Action buttons at bottom: Delete | Copy | Connect
        let button_y = inner.y + inner.height.saturating_sub(1);
        let mut x = inner.x;

        let delete_text = " Delete ";
        let delete_style = Style::default()
            .fg(Color::White)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD);
        buf.set_string(x, button_y, delete_text, delete_style);
        self.delete_button_area = Some(Rect::new(x, button_y, delete_text.len() as u16, 1));
        x += delete_text.len() as u16 + 1;

        let copy_text = " Copy ";
        let copy_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        buf.set_string(x, button_y, copy_text, copy_style);
        self.copy_button_area = Some(Rect::new(x, button_y, copy_text.len() as u16, 1));
        x += copy_text.len() as u16 + 1;

        let connect_text = " Connect ";
        let connect_style = Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD);
        buf.set_string(x, button_y, connect_text, connect_style);
        self.connect_button_area = Some(Rect::new(x, button_y, connect_text.len() as u16, 1));
    }

    pub fn click_detail(&mut self, x: u16, y: u16) -> SavedModalAction {
        if self.nodes.is_empty() {
            return SavedModalAction::None;
        }

        let in_button = |area: Option<Rect>| -> bool {
            if let Some(a) = area {
                x >= a.x && x < a.x + a.width && y >= a.y && y < a.y + a.height
            } else {
                false
            }
        };

        if in_button(self.identify_button_area) {
            SavedModalAction::ToggleIdentify
        } else if in_button(self.connect_button_area) {
            SavedModalAction::Connect
        } else if in_button(self.copy_button_area) {
            SavedModalAction::Copy
        } else if in_button(self.delete_button_area) {
            SavedModalAction::Delete
        } else {
            SavedModalAction::None
        }
    }
}

impl Widget for &mut SavedView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chunks = ratatui::layout::Layout::horizontal([
            ratatui::layout::Constraint::Percentage(50),
            ratatui::layout::Constraint::Percentage(50),
        ])
        .split(area);

        self.render_list(chunks[0], buf);
        self.render_detail(chunks[1], buf);
    }
}
