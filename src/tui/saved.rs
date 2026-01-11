use crate::network::NodeInfo;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

pub struct SavedView {
    nodes: Vec<NodeInfo>,
    selected: usize,
    scroll_offset: usize,
    last_height: usize,
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
            selected: 0,
            scroll_offset: 0,
            last_height: 10,
        }
    }

    pub fn add_node(&mut self, node: NodeInfo) {
        if !self.nodes.iter().any(|n| n.hash == node.hash) {
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

    pub fn select_next(&mut self) {
        if !self.nodes.is_empty() {
            self.selected = (self.selected + 1) % self.nodes.len();
            self.adjust_scroll();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.nodes.is_empty() {
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

    pub fn click(&mut self, _x: u16, y: u16, area: Rect) -> Option<usize> {
        let inner_y = y.saturating_sub(area.y + 1);
        let idx = self.scroll_offset + inner_y as usize;

        if idx < self.nodes.len() {
            self.selected = idx;
            Some(idx)
        } else {
            None
        }
    }

    pub fn remove_selected(&mut self) -> Option<NodeInfo> {
        if self.nodes.is_empty() {
            return None;
        }
        let removed = self.nodes.remove(self.selected);
        if self.selected >= self.nodes.len() && !self.nodes.is_empty() {
            self.selected = self.nodes.len() - 1;
        }
        Some(removed)
    }

    fn render_list(&mut self, area: Rect, buf: &mut Buffer) {
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

    fn render_detail(&self, area: Rect, buf: &mut Buffer) {
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
            Line::from(""),
            Line::from(vec![
                Span::styled("[Enter]", Style::default().fg(Color::Magenta)),
                Span::raw(" Connect  "),
                Span::styled("[d]", Style::default().fg(Color::Red)),
                Span::raw(" Remove"),
            ]),
        ];

        Paragraph::new(content).render(inner, buf);
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
