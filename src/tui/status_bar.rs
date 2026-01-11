use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::time::Instant;

pub struct StatusBar {
    announces_received: usize,
    announces_sent: usize,
    status_message: Option<String>,
    status_time: Option<Instant>,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            announces_received: 0,
            announces_sent: 0,
            status_message: None,
            status_time: None,
        }
    }

    pub fn increment_received(&mut self) {
        self.announces_received += 1;
    }

    pub fn increment_sent(&mut self) {
        self.announces_sent += 1;
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_time = Some(Instant::now());
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
        let mut spans = vec![Span::raw(" ")];

        spans.push(Span::styled("\u{2193}", Style::default().fg(Color::Green)));
        spans.push(Span::styled(
            format!("{} ", self.announces_received),
            Style::default().fg(Color::White),
        ));

        spans.push(Span::styled("\u{2191}", Style::default().fg(Color::Cyan)));
        spans.push(Span::styled(
            format!("{}", self.announces_sent),
            Style::default().fg(Color::White),
        ));

        if let Some(ref msg) = self.status_message {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                msg.clone(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let line = Line::from(spans);
        Paragraph::new(line)
            .style(Style::default().bg(Color::Rgb(20, 20, 30)))
            .render(area, buf);
    }
}
