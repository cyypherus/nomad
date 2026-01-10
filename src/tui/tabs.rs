use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Tabs as RatTabs, Widget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Conversations,
    Network,
    Guide,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Conversations, Tab::Network, Tab::Guide];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Conversations => "Conversations",
            Tab::Network => "Network",
            Tab::Guide => "Guide",
        }
    }

    pub fn next(&self) -> Tab {
        match self {
            Tab::Conversations => Tab::Network,
            Tab::Network => Tab::Guide,
            Tab::Guide => Tab::Conversations,
        }
    }

    pub fn prev(&self) -> Tab {
        match self {
            Tab::Conversations => Tab::Guide,
            Tab::Network => Tab::Conversations,
            Tab::Guide => Tab::Network,
        }
    }

    fn index(&self) -> usize {
        match self {
            Tab::Conversations => 0,
            Tab::Network => 1,
            Tab::Guide => 2,
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
        let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();

        let tabs = RatTabs::new(titles)
            .block(Block::default().borders(Borders::BOTTOM))
            .select(self.selected.index())
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        tabs.render(area, buf);
    }
}
