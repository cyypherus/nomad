use crossterm::event::{Event, KeyCode, KeyModifiers};
use lxmf::{ConversationInfo, StoredMessage, DESTINATION_LENGTH};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget, Wrap},
    Frame,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum ConversationPane {
    #[default]
    List,
    Messages,
    Compose,
    NewConversation,
}

pub struct ConversationsView {
    conversations: Vec<ConversationInfo>,
    selected: usize,
    messages: Vec<StoredMessage>,
    message_scroll: usize,
    pane: ConversationPane,
    compose_input: Input,
    dest_input: Input,
    new_dest_peer: Option<[u8; DESTINATION_LENGTH]>,
}

impl Default for ConversationsView {
    fn default() -> Self {
        Self {
            conversations: Vec::new(),
            selected: 0,
            messages: Vec::new(),
            message_scroll: 0,
            pane: ConversationPane::List,
            compose_input: Input::default(),
            dest_input: Input::default(),
            new_dest_peer: None,
        }
    }
}

impl ConversationsView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_conversations(&mut self, convos: Vec<ConversationInfo>) {
        self.conversations = convos;
        if self.selected >= self.conversations.len() && !self.conversations.is_empty() {
            self.selected = self.conversations.len() - 1;
        }
    }

    pub fn set_messages(&mut self, messages: Vec<StoredMessage>) {
        self.messages = messages;
        self.message_scroll = 0;
    }

    pub fn select_next(&mut self) {
        match self.pane {
            ConversationPane::List => {
                if !self.conversations.is_empty() {
                    self.selected = (self.selected + 1) % self.conversations.len();
                }
            }
            ConversationPane::Messages => {
                if self.message_scroll + 10 < self.messages.len() {
                    self.message_scroll += 1;
                }
            }
            ConversationPane::Compose | ConversationPane::NewConversation => {}
        }
    }

    pub fn select_prev(&mut self) {
        match self.pane {
            ConversationPane::List => {
                if !self.conversations.is_empty() {
                    self.selected = self
                        .selected
                        .checked_sub(1)
                        .unwrap_or(self.conversations.len() - 1);
                }
            }
            ConversationPane::Messages => {
                self.message_scroll = self.message_scroll.saturating_sub(1);
            }
            ConversationPane::Compose | ConversationPane::NewConversation => {}
        }
    }

    pub fn selected_peer(&self) -> Option<[u8; DESTINATION_LENGTH]> {
        self.new_dest_peer
            .or_else(|| self.conversations.get(self.selected).map(|c| c.peer_hash))
    }

    pub fn pane(&self) -> ConversationPane {
        self.pane
    }

    pub fn enter(&mut self) -> bool {
        match self.pane {
            ConversationPane::List if !self.conversations.is_empty() => {
                self.pane = ConversationPane::Messages;
                true
            }
            ConversationPane::Messages => {
                self.pane = ConversationPane::Compose;
                false
            }
            ConversationPane::NewConversation => {
                if let Some(peer) = self.parse_dest_hash() {
                    self.new_dest_peer = Some(peer);
                    self.dest_input.reset();
                    self.pane = ConversationPane::Compose;
                }
                false
            }
            _ => false,
        }
    }

    fn parse_dest_hash(&self) -> Option<[u8; DESTINATION_LENGTH]> {
        let text = self.dest_input.value();
        let clean: String = text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if clean.len() == 32 {
            let bytes = hex::decode(&clean).ok()?;
            let mut arr = [0u8; DESTINATION_LENGTH];
            arr.copy_from_slice(&bytes);
            Some(arr)
        } else {
            None
        }
    }

    pub fn back(&mut self) -> bool {
        match self.pane {
            ConversationPane::Compose if self.new_dest_peer.is_some() => {
                self.pane = ConversationPane::NewConversation;
                self.compose_input.reset();
                true
            }
            ConversationPane::Compose => {
                self.pane = ConversationPane::Messages;
                true
            }
            ConversationPane::Messages => {
                self.pane = ConversationPane::List;
                self.messages.clear();
                true
            }
            ConversationPane::NewConversation => {
                self.pane = ConversationPane::List;
                self.dest_input.reset();
                self.new_dest_peer = None;
                true
            }
            ConversationPane::List => false,
        }
    }

    pub fn handle_event(&mut self, event: &Event) -> InputResult {
        match self.pane {
            ConversationPane::Compose => {
                if let Event::Key(key) = event {
                    match key.code {
                        KeyCode::Esc => return InputResult::Back,
                        KeyCode::Enter if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                            let content = self.compose_input.value().to_string();
                            if !content.is_empty() {
                                if let Some(peer) = self.selected_peer() {
                                    self.compose_input.reset();
                                    self.new_dest_peer = None;
                                    return InputResult::SendMessage(content, peer);
                                }
                            }
                            return InputResult::Consumed;
                        }
                        _ => {}
                    }
                }
                self.compose_input.handle_event(event);
                InputResult::Consumed
            }
            ConversationPane::NewConversation => {
                if let Event::Key(key) = event {
                    match key.code {
                        KeyCode::Esc => return InputResult::Back,
                        KeyCode::Enter => {
                            if let Some(peer) = self.parse_dest_hash() {
                                self.new_dest_peer = Some(peer);
                                self.dest_input.reset();
                                self.pane = ConversationPane::Compose;
                            }
                            return InputResult::Consumed;
                        }
                        KeyCode::Char(c) => {
                            if c.is_ascii_hexdigit() && self.dest_input.value().len() < 32 {
                                self.dest_input.handle_event(event);
                            }
                            return InputResult::Consumed;
                        }
                        KeyCode::Backspace => {
                            self.dest_input.handle_event(event);
                            return InputResult::Consumed;
                        }
                        _ => return InputResult::Consumed,
                    }
                }
                InputResult::Consumed
            }
            _ => InputResult::NotConsumed,
        }
    }

    pub fn start_new_conversation(&mut self) {
        self.dest_input.reset();
        self.new_dest_peer = None;
        self.pane = ConversationPane::NewConversation;
    }

    pub fn render_input_pane(&self, frame: &mut Frame, area: Rect) {
        match self.pane {
            ConversationPane::Compose => {
                let chunks =
                    Layout::vertical([Constraint::Min(5), Constraint::Length(3)]).split(area);
                self.render_messages_area(frame.buffer_mut(), chunks[0]);

                let width = chunks[1].width.saturating_sub(2) as usize;
                let scroll = self.compose_input.visual_scroll(width);
                let input_widget = Paragraph::new(self.compose_input.value())
                    .scroll((0, scroll as u16))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Compose [Enter to send, Esc to cancel]"),
                    );
                frame.render_widget(input_widget, chunks[1]);

                let cursor_pos = self.compose_input.visual_cursor().saturating_sub(scroll);
                frame.set_cursor_position((chunks[1].x + 1 + cursor_pos as u16, chunks[1].y + 1));
            }
            ConversationPane::NewConversation => {
                let chunks =
                    Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);

                let width = chunks[0].width.saturating_sub(2) as usize;
                let scroll = self.dest_input.visual_scroll(width);
                let input_widget = Paragraph::new(self.dest_input.value())
                    .scroll((0, scroll as u16))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Destination [Enter when valid, Esc to cancel]"),
                    );
                frame.render_widget(input_widget, chunks[0]);

                let cursor_pos = self.dest_input.visual_cursor().saturating_sub(scroll);
                frame.set_cursor_position((chunks[0].x + 1 + cursor_pos as u16, chunks[0].y + 1));

                let valid = self.parse_dest_hash().is_some();
                let text = self.dest_input.value();
                let status = if text.is_empty() {
                    "Enter 32 hex characters (0-9, a-f)".to_string()
                } else if valid {
                    "Valid address - press Enter to continue".to_string()
                } else {
                    format!("{}/32 characters", text.len())
                };
                let help = Paragraph::new(status)
                    .block(Block::default().borders(Borders::ALL).title("Status"));
                frame.render_widget(help, chunks[1]);
            }
            _ => {}
        }
    }

    fn render_messages_area(&self, buf: &mut Buffer, area: Rect) {
        let peer = self.selected_peer().unwrap_or([0u8; 16]);
        let peer_str = hex::encode(peer);
        let title = format!("{}..{}", &peer_str[..4], &peer_str[28..]);

        let lines: Vec<Line> = if self.messages.is_empty() {
            vec![Line::from("No messages yet - start the conversation!")]
        } else {
            self.messages
                .iter()
                .skip(self.message_scroll)
                .flat_map(|msg| {
                    let is_ours = !msg.incoming;
                    let prefix = if is_ours { "You: " } else { "Them: " };
                    let style = if is_ours {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Green)
                    };

                    let content = msg.content_as_string().unwrap_or_default();
                    let time = format_timestamp(msg.timestamp);

                    vec![
                        Line::from(vec![
                            Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                            Span::styled(time, Style::default().fg(Color::DarkGray)),
                        ]),
                        Line::from(content),
                        Line::from(""),
                    ]
                })
                .collect()
        };

        let messages = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false });
        messages.render(area, buf);
    }
}

impl Widget for &ConversationsView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.pane {
            ConversationPane::List => self.render_list(area, buf),
            ConversationPane::Messages => self.render_conversation(area, buf),
            ConversationPane::Compose | ConversationPane::NewConversation => {}
        }
    }
}

impl ConversationsView {
    fn render_list(&self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        if self.conversations.is_empty() {
            let empty =
                Paragraph::new("No conversations yet.\n\nPress [n] to start a new conversation.")
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Conversations"),
                    )
                    .wrap(Wrap { trim: false });
            empty.render(area, buf);
            return;
        }

        let items: Vec<ListItem> = self
            .conversations
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let style = if i == self.selected {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else if c.unread_count > 0 {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };

                let hash_str = hex::encode(c.peer_hash);
                let short_hash = format!("{}..{}", &hash_str[..4], &hash_str[28..]);

                let unread_indicator = if c.unread_count > 0 {
                    format!(" ({})", c.unread_count)
                } else {
                    String::new()
                };

                ListItem::new(Line::from(format!("{}{}", short_hash, unread_indicator)))
                    .style(style)
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Contacts"));
        list.render(chunks[0], buf);

        let preview = if let Some(conv) = self.conversations.get(self.selected) {
            let hash_str = hex::encode(conv.peer_hash);
            let time_str = conv
                .last_timestamp
                .map(format_timestamp)
                .unwrap_or_default();
            let preview_text = conv
                .last_message_preview
                .clone()
                .unwrap_or_else(|| "[no content]".to_string());

            Paragraph::new(vec![
                Line::from(Span::styled(
                    hash_str,
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{} messages", conv.message_count),
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(time_str, Style::default().fg(Color::DarkGray))),
                Line::from(preview_text),
            ])
            .block(Block::default().borders(Borders::ALL).title("Preview"))
            .wrap(Wrap { trim: false })
        } else {
            Paragraph::new("No conversation selected")
                .block(Block::default().borders(Borders::ALL).title("Preview"))
                .wrap(Wrap { trim: false })
        };

        preview.render(chunks[1], buf);
    }

    fn render_conversation(&self, area: Rect, buf: &mut Buffer) {
        let peer = self.selected_peer().unwrap_or([0u8; 16]);
        let peer_str = hex::encode(peer);
        let title = format!("Conversation with {}..{}", &peer_str[..4], &peer_str[28..]);

        let lines: Vec<Line> = if self.messages.is_empty() {
            vec![Line::from("No messages in this conversation")]
        } else {
            self.messages
                .iter()
                .skip(self.message_scroll)
                .flat_map(|msg| {
                    let is_ours = !msg.incoming;
                    let prefix = if is_ours { "You: " } else { "Them: " };
                    let style = if is_ours {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::Green)
                    };

                    let content = msg.content_as_string().unwrap_or_default();
                    let time = format_timestamp(msg.timestamp);

                    vec![
                        Line::from(vec![
                            Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                            Span::styled(time, Style::default().fg(Color::DarkGray)),
                        ]),
                        Line::from(content),
                        Line::from(""),
                    ]
                })
                .collect()
        };

        let messages = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false });
        messages.render(area, buf);
    }
}

pub enum InputResult {
    Consumed,
    NotConsumed,
    Back,
    SendMessage(String, [u8; DESTINATION_LENGTH]),
}

fn format_timestamp(ts: f64) -> String {
    let duration = Duration::from_secs_f64(ts);
    let time = UNIX_EPOCH + duration;
    let now = SystemTime::now();

    if let Ok(elapsed) = now.duration_since(time) {
        if elapsed.as_secs() < 60 {
            "Just now".to_string()
        } else if elapsed.as_secs() < 3600 {
            format!("{}m ago", elapsed.as_secs() / 60)
        } else if elapsed.as_secs() < 86400 {
            format!("{}h ago", elapsed.as_secs() / 3600)
        } else {
            format!("{}d ago", elapsed.as_secs() / 86400)
        }
    } else {
        "Future".to_string()
    }
}
