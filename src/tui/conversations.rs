use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
};

pub struct Conversation {
    pub name: String,
    pub last_message: String,
    pub unread: bool,
}

pub struct ConversationsView {
    conversations: Vec<Conversation>,
    selected: usize,
}

impl Default for ConversationsView {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationsView {
    pub fn new() -> Self {
        Self {
            conversations: vec![
                Conversation {
                    name: "Alice".into(),
                    last_message: "Hey, how are you?".into(),
                    unread: true,
                },
                Conversation {
                    name: "Bob".into(),
                    last_message: "See you tomorrow".into(),
                    unread: false,
                },
            ],
            selected: 0,
        }
    }

    pub fn select_next(&mut self) {
        if !self.conversations.is_empty() {
            self.selected = (self.selected + 1) % self.conversations.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.conversations.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.conversations.len() - 1);
        }
    }
}

impl Widget for &ConversationsView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::horizontal([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(area);

        let items: Vec<ListItem> = self
            .conversations
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let style = if i == self.selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else if c.unread {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(c.name.clone())).style(style)
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Contacts"));
        list.render(chunks[0], buf);

        let messages = if self.conversations.is_empty() {
            Paragraph::new("No conversations")
        } else {
            let conv = &self.conversations[self.selected];
            Paragraph::new(vec![
                Line::from(Span::styled(
                    &conv.name,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(&*conv.last_message),
            ])
        }
        .block(Block::default().borders(Borders::ALL).title("Messages"));

        messages.render(chunks[1], buf);
    }
}
