use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct MyNodeView {
    node_hash: [u8; 16],
    node_name: String,
    last_announce_secs: u64,
    announce_button_area: Option<Rect>,
}

impl MyNodeView {
    pub fn new(node_hash: [u8; 16]) -> Self {
        Self {
            node_hash,
            node_name: "Anonymous Peer".to_string(),
            last_announce_secs: 0,
            announce_button_area: None,
        }
    }

    pub fn set_name(&mut self, name: String) {
        self.node_name = name;
    }

    pub fn update_announce_time(&mut self) {
        self.last_announce_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }

    pub fn click(&self, x: u16, y: u16) -> bool {
        if let Some(area) = self.announce_button_area {
            x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
        } else {
            false
        }
    }

    fn format_announce_time(&self) -> String {
        if self.last_announce_secs == 0 {
            return "Never".to_string();
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let elapsed = now.saturating_sub(self.last_announce_secs);

        if elapsed < 60 {
            format!("{}s ago", elapsed)
        } else if elapsed < 3600 {
            format!("{}m ago", elapsed / 60)
        } else if elapsed < 86400 {
            format!("{}h ago", elapsed / 3600)
        } else {
            format!("{}d ago", elapsed / 86400)
        }
    }

    fn render_identity_card(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Line::from(vec![Span::styled(
                " My Identity ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        block.render(area, buf);

        let hash_hex = hex::encode(self.node_hash);

        let content = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("   Name: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &self.node_name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "   Hash:",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(vec![
                Span::raw("   "),
                Span::styled(&hash_hex[..16], Style::default().fg(Color::Magenta)),
            ]),
            Line::from(vec![
                Span::raw("   "),
                Span::styled(&hash_hex[16..], Style::default().fg(Color::Magenta)),
            ]),
            Line::from(""),
        ];

        Paragraph::new(content).render(inner, buf);
    }

    fn render_announce_section(&mut self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Line::from(vec![Span::styled(
                " Announce ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        let announce_time = self.format_announce_time();
        let status_color = if self.last_announce_secs == 0 {
            Color::Yellow
        } else {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let elapsed = now.saturating_sub(self.last_announce_secs);
            if elapsed < 300 {
                Color::Green
            } else if elapsed < 1800 {
                Color::Yellow
            } else {
                Color::Red
            }
        };

        let content = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("   Last Announced: ", Style::default().fg(Color::DarkGray)),
                Span::styled(announce_time, Style::default().fg(status_color)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "   Broadcasting allows other nodes to",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "   discover and connect to you.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
        ];

        let content_height = content.len() as u16;
        Paragraph::new(content).render(
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                content_height.min(inner.height),
            ),
            buf,
        );

        if inner.height > content_height + 1 {
            let button_y = inner.y + content_height;
            let button_text = " Announce Now ";
            let button_width = button_text.len() as u16;
            let button_x = inner.x + (inner.width.saturating_sub(button_width)) / 2;

            let button_style = Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD);

            buf.set_string(button_x, button_y, button_text, button_style);

            self.announce_button_area = Some(Rect::new(button_x, button_y, button_width, 1));
        }
    }

    fn render_stats(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(Line::from(vec![Span::styled(
                " Network Stats ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        let content = vec![
            Line::from(""),
            Line::from(Span::styled(
                "   Connection statistics will",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "   appear here in a future update.",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(""),
        ];

        Paragraph::new(content).render(inner, buf);
    }
}

impl Widget for &mut MyNodeView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::vertical([
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(4),
        ])
        .split(area);

        self.render_identity_card(chunks[0], buf);
        self.render_announce_section(chunks[1], buf);
        self.render_stats(chunks[2], buf);
    }
}
