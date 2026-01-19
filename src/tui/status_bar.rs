use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use rinse::StatsSnapshot;

pub struct StatusBar {
    status_message: Option<String>,
    relay_stats: Option<StatsSnapshot>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            status_message: None,
            relay_stats: None,
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    pub fn set_relay_stats(&mut self, stats: StatsSnapshot) {
        self.relay_stats = Some(stats);
    }

    pub fn tick(&mut self) {}
}

impl Widget for &StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for x in area.x..area.x + area.width {
            for y in area.y..area.y + area.height {
                buf[(x, y)].set_bg(Color::Rgb(20, 20, 30));
            }
        }

        if let Some(ref msg) = self.status_message {
            let max_chars = area.width.saturating_sub(2) as usize;
            let char_count = msg.chars().count();
            let display_msg = if char_count > max_chars {
                let truncated: String = msg.chars().take(max_chars.saturating_sub(3)).collect();
                format!("{}...", truncated)
            } else {
                msg.clone()
            };
            let spans = vec![
                Span::raw(" "),
                Span::styled(
                    display_msg,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            let line = Line::from(spans);
            Paragraph::new(line)
                .alignment(Alignment::Right)
                .render(Rect::new(area.x, area.y, area.width, 1), buf);
        }

        if let Some(ref stats) = self.relay_stats {
            if stats.packets_relayed > 0 || stats.announces_relayed > 0 {
                let relay_line = Line::from(vec![
                    Span::styled("\u{2191}\u{2193}", Style::default().fg(Color::Magenta)),
                    Span::styled(
                        format!(" {} ", StatsSnapshot::format_bytes(stats.bytes_relayed)),
                        Style::default().fg(Color::White),
                    ),
                ]);
                Paragraph::new(relay_line)
                    .alignment(Alignment::Right)
                    .render(Rect::new(area.x, area.y + 1, area.width, 1), buf);
            }
        }
    }
}
