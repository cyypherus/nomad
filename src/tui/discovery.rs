use crate::network::NodeInfo;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Widget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalAction {
    None,
    Connect,
    Save,
    Dismiss,
}

pub struct DiscoveryView {
    nodes: Vec<NodeInfo>,
    selected: usize,
    scroll_offset: usize,
    modal_open: bool,
    modal_selected: usize,
    last_height: usize,
}

impl Default for DiscoveryView {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoveryView {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            modal_open: false,
            modal_selected: 0,
            last_height: 10,
        }
    }

    pub fn add_node(&mut self, node: NodeInfo) {
        if let Some(existing) = self.nodes.iter_mut().find(|n| n.hash == node.hash) {
            existing.name = node.name;
            existing.identity = node.identity;
        } else {
            self.nodes.push(node);
        }
    }

    pub fn nodes(&self) -> &[NodeInfo] {
        &self.nodes
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn selected_node(&self) -> Option<&NodeInfo> {
        self.nodes.get(self.selected)
    }

    pub fn is_modal_open(&self) -> bool {
        self.modal_open
    }

    pub fn select_next(&mut self) {
        if self.modal_open {
            self.modal_selected = (self.modal_selected + 1) % 3;
        } else if !self.nodes.is_empty() {
            self.selected = (self.selected + 1) % self.nodes.len();
            self.adjust_scroll();
        }
    }

    pub fn select_prev(&mut self) {
        if self.modal_open {
            self.modal_selected = if self.modal_selected == 0 {
                2
            } else {
                self.modal_selected - 1
            };
        } else if !self.nodes.is_empty() {
            self.selected = self.selected.checked_sub(1).unwrap_or(self.nodes.len() - 1);
            self.adjust_scroll();
        }
    }

    fn adjust_scroll(&mut self) {
        if self.last_height == 0 {
            return;
        }
        if self.selected < self.scroll_offset {
            self.scroll_offset = self.selected;
        } else if self.selected >= self.scroll_offset + self.last_height {
            self.scroll_offset = self.selected - self.last_height + 1;
        }
    }

    pub fn open_modal(&mut self) {
        if !self.nodes.is_empty() {
            self.modal_open = true;
            self.modal_selected = 0;
        }
    }

    pub fn close_modal(&mut self) {
        self.modal_open = false;
    }

    pub fn modal_action(&self) -> ModalAction {
        if !self.modal_open {
            return ModalAction::None;
        }
        match self.modal_selected {
            0 => ModalAction::Connect,
            1 => ModalAction::Save,
            2 => ModalAction::Dismiss,
            _ => ModalAction::None,
        }
    }

    pub fn click(&mut self, _x: u16, y: u16, area: Rect) -> Option<usize> {
        if self.modal_open {
            return None;
        }

        let inner_y = y.saturating_sub(area.y + 1);
        let idx = self.scroll_offset + inner_y as usize;

        if idx < self.nodes.len() {
            self.selected = idx;
            Some(idx)
        } else {
            None
        }
    }

    pub fn click_modal(&mut self, x: u16, y: u16, area: Rect) -> ModalAction {
        if !self.modal_open {
            return ModalAction::None;
        }

        let modal_width = 50.min(area.width.saturating_sub(4));
        let modal_height = 13.min(area.height.saturating_sub(4));
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;

        let inner_x = modal_x + 1;
        let inner_y = modal_y + 1;
        let inner_height = modal_height.saturating_sub(2);

        let button_y = inner_y + inner_height.saturating_sub(2);
        let inner_width = modal_width.saturating_sub(2);
        let center_x = inner_x + inner_width / 2;

        if y != button_y {
            return ModalAction::None;
        }

        let connect_start = center_x.saturating_sub(22);
        let connect_end = connect_start + 9;
        let save_start = center_x.saturating_sub(6);
        let save_end = save_start + 6;
        let cancel_start = center_x + 8;
        let cancel_end = cancel_start + 9;

        if x >= connect_start && x < connect_end {
            return ModalAction::Connect;
        }
        if x >= save_start && x < save_end {
            return ModalAction::Save;
        }
        if x >= cancel_start && x < cancel_end {
            return ModalAction::Dismiss;
        }

        ModalAction::None
    }

    pub fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(
                    " Discovered Nodes ",
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
                    "Listening for announcements...",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Nodes will appear here as they announce",
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
            .enumerate()
            .skip(self.scroll_offset)
            .take(inner.height as usize)
            .map(|(i, node)| {
                let is_selected = i == self.selected;

                let name_style = if is_selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };

                let hash_short = format!("{}..{}", &node.hash_hex()[..6], &node.hash_hex()[26..]);
                let hash_style = Style::default().fg(Color::DarkGray);

                let prefix = if is_selected { "> " } else { "  " };
                let prefix_style = if is_selected {
                    Style::default().fg(Color::Magenta)
                } else {
                    Style::default()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(&node.name, name_style),
                    Span::styled(format!("  {}", hash_short), hash_style),
                ]))
            })
            .collect();

        let list = List::new(items);
        list.render(inner, buf);
    }

    pub fn render_modal(&self, area: Rect, buf: &mut Buffer) {
        if !self.modal_open {
            return;
        }

        let Some(node) = self.selected_node() else {
            return;
        };

        let modal_width = 50.min(area.width.saturating_sub(4));
        let modal_height = 13.min(area.height.saturating_sub(4));
        let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

        Clear.render(modal_area, buf);

        let block = Block::default()
            .title(Line::from(vec![Span::styled(
                " Node ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        let hash_hex = node.hash_hex();
        let content = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Name: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &node.name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "  Hash: ",
                Style::default().fg(Color::DarkGray),
            )]),
            Line::from(vec![Span::styled(
                format!("  {}", &hash_hex[..16]),
                Style::default().fg(Color::Cyan),
            )]),
            Line::from(vec![Span::styled(
                format!("  {}", &hash_hex[16..]),
                Style::default().fg(Color::Cyan),
            )]),
            Line::from(""),
        ];

        Paragraph::new(content).render(
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(2),
            ),
            buf,
        );

        let button_y = inner.y + inner.height.saturating_sub(2);
        let center_x = inner.x + inner.width / 2;

        let connect_style = if self.modal_selected == 0 {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Magenta)
        };

        let save_style = if self.modal_selected == 1 {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        let cancel_style = if self.modal_selected == 2 {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };

        buf.set_string(
            center_x.saturating_sub(22),
            button_y,
            " Connect ",
            connect_style,
        );
        buf.set_string(center_x.saturating_sub(6), button_y, " Save ", save_style);
        buf.set_string(center_x + 8, button_y, " Cancel ", cancel_style);
    }
}

impl Widget for &mut DiscoveryView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_list(area, buf);
        self.render_modal(area, buf);
    }
}
