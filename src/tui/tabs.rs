use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Widget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Conversations,
    Network,
}

impl Tab {
    pub const ALL: [Tab; 2] = [Tab::Conversations, Tab::Network];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Conversations => "Conversations",
            Tab::Network => "Network",
        }
    }

    pub fn next(&self) -> Tab {
        match self {
            Tab::Conversations => Tab::Network,
            Tab::Network => Tab::Conversations,
        }
    }

    pub fn prev(&self) -> Tab {
        match self {
            Tab::Conversations => Tab::Network,
            Tab::Network => Tab::Conversations,
        }
    }
}

pub struct TabBar {
    selected: Tab,
}

impl TabBar {
    pub fn new(selected: Tab) -> Self {
        Self { selected }
    }
}

impl Widget for TabBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![Span::raw(" + ")];

        for tab in Tab::ALL {
            let style = if tab == self.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled("[ ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(tab.title(), style));
            spans.push(Span::styled(" ]", Style::default().fg(Color::DarkGray)));
            spans.push(Span::raw(" "));
        }

        let line = Line::from(spans);
        let para = ratatui::widgets::Paragraph::new(line).block(Block::default());
        para.render(area, buf);
    }
}
