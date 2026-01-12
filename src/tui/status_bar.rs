use reticulum::transport::TransportStatsSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::time::Instant;

pub struct StatusBar {
    status_message: Option<String>,
    status_time: Option<Instant>,
    relay_stats: Option<TransportStatsSnapshot>,
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
            status_time: None,
            relay_stats: None,
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_time = Some(Instant::now());
    }

    pub fn set_relay_stats(&mut self, stats: TransportStatsSnapshot) {
        self.relay_stats = Some(stats);
    }

    pub fn tick(&mut self) {
        if let Some(time) = self.status_time {
            if time.elapsed().as_secs() > 3 {
                self.status_message = None;
                self.status_time = None;
            }
        }
    }
}

impl Widget for &StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        for x in area.x..area.x + area.width {
            for y in area.y..area.y + area.height {
                buf[(x, y)].set_bg(Color::Rgb(20, 20, 30));
            }
        }

        if let Some(ref msg) = self.status_message {
            let spans = vec![
                Span::raw(" "),
                Span::styled(
                    msg.clone(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            let line = Line::from(spans);
            Paragraph::new(line).render(Rect::new(area.x, area.y, area.width, 1), buf);
        }

        if let Some(ref stats) = self.relay_stats {
            if stats.packets_relayed > 0 || stats.announces_relayed > 0 {
                let relay_line = Line::from(vec![
                    Span::styled("\u{2191}\u{2193}", Style::default().fg(Color::Magenta)),
                    Span::styled(
                        format!(
                            " {} ",
                            TransportStatsSnapshot::format_bytes(stats.bytes_relayed)
                        ),
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
