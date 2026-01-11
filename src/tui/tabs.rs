use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Discovery,
    Saved,
    MyNode,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Discovery, Tab::Saved, Tab::MyNode];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Discovery => "Discovery",
            Tab::Saved => "Saved",
            Tab::MyNode => "My Node",
        }
    }

    pub fn next(&self) -> Tab {
        match self {
            Tab::Discovery => Tab::Saved,
            Tab::Saved => Tab::MyNode,
            Tab::MyNode => Tab::Discovery,
        }
    }

    pub fn prev(&self) -> Tab {
        match self {
            Tab::Discovery => Tab::MyNode,
            Tab::Saved => Tab::Discovery,
            Tab::MyNode => Tab::Saved,
        }
    }
}

pub struct TabBar {
    selected: Tab,
    tab_areas: Vec<(Tab, u16, u16)>,
}

impl TabBar {
    pub fn new(selected: Tab) -> Self {
        Self {
            selected,
            tab_areas: Vec::new(),
        }
    }

    pub fn hit_test(&self, x: u16) -> Option<Tab> {
        for (tab, start, end) in &self.tab_areas {
            if x >= *start && x < *end {
                return Some(*tab);
            }
        }
        None
    }
}

impl Widget for &mut TabBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.tab_areas.clear();
        let mut x = area.x + 1;

        for tab in Tab::ALL {
            let is_selected = tab == self.selected;

            let start_x = x;

            if is_selected {
                buf.set_string(x, area.y, " ", Style::default().bg(Color::Magenta));
                x += 1;

                let title = tab.title();
                buf.set_string(
                    x,
                    area.y,
                    title,
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                );
                x += title.len() as u16;

                buf.set_string(x, area.y, " ", Style::default().bg(Color::Magenta));
                x += 1;
            } else {
                buf.set_string(x, area.y, " ", Style::default());
                x += 1;

                let title = tab.title();
                buf.set_string(x, area.y, title, Style::default().fg(Color::DarkGray));
                x += title.len() as u16;

                buf.set_string(x, area.y, " ", Style::default());
                x += 1;
            }

            buf.set_string(x, area.y, " ", Style::default());
            x += 1;

            self.tab_areas.push((tab, start_x, x));
        }
    }
}
