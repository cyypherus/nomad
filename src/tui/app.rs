use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use lxmf::{ConversationInfo, StoredMessage, DESTINATION_LENGTH};
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    prelude::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};
use tokio::sync::mpsc;

use super::conversations::{ConversationPane, ConversationsView};
use super::network::NetworkView;
use super::tabs::{Tab, TabBar};

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    NodeAnnounce(NodeInfo),
    AnnounceSent,
    Status(String),
    MessageReceived([u8; DESTINATION_LENGTH]),
    ConversationsUpdated(Vec<ConversationInfo>),
    MessagesLoaded(Vec<StoredMessage>),
    PageReceived { url: String, content: String },
    PageFailed { url: String, reason: String },
}

use crate::network::NodeInfo;

#[derive(Debug, Clone)]
pub enum TuiCommand {
    Announce,
    LoadConversations,
    LoadMessages([u8; DESTINATION_LENGTH]),
    SendMessage {
        content: String,
        destination: [u8; DESTINATION_LENGTH],
    },
    MarkConversationRead([u8; DESTINATION_LENGTH]),
    FetchPage {
        node: NodeInfo,
        path: String,
    },
}

pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: bool,
    tab: Tab,
    conversations: ConversationsView,
    network: NetworkView,
    event_rx: mpsc::Receiver<NetworkEvent>,
    cmd_tx: mpsc::Sender<TuiCommand>,
    status_message: Option<String>,
    status_time: Option<Instant>,
    announces_received: usize,
    announces_sent: usize,
    unread_count: usize,
}

impl TuiApp {
    pub fn new(
        dest_hash: [u8; 16],
        initial_nodes: Vec<NodeInfo>,
        event_rx: mpsc::Receiver<NetworkEvent>,
        cmd_tx: mpsc::Sender<TuiCommand>,
    ) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Clear(ClearType::All))?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let _ = cmd_tx.blocking_send(TuiCommand::LoadConversations);

        Ok(Self {
            terminal,
            running: true,
            tab: Tab::default(),
            conversations: ConversationsView::new(),
            network: NetworkView::new(dest_hash, initial_nodes),
            event_rx,
            cmd_tx,
            status_message: None,
            status_time: None,
            announces_received: 0,
            announces_sent: 0,
            unread_count: 0,
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        while self.running {
            self.poll_events();
            self.draw()?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                NetworkEvent::NodeAnnounce(node) => {
                    self.network.add_node(node);
                    self.announces_received += 1;
                    self.set_status("Node announce received");
                }
                NetworkEvent::AnnounceSent => {
                    self.announces_sent += 1;
                    self.network.update_announce_time();
                    self.set_status("Announced");
                }
                NetworkEvent::Status(msg) => {
                    self.set_status(&msg);
                }
                NetworkEvent::PageReceived { url, content } => {
                    self.network.set_page_content(url, content);
                }
                NetworkEvent::PageFailed { url, reason } => {
                    self.network.set_connection_failed(url, reason);
                }
                NetworkEvent::MessageReceived(peer) => {
                    self.unread_count += 1;
                    let peer_str = hex::encode(peer);
                    self.set_status(&format!(
                        "Message from {}..{}",
                        &peer_str[..4],
                        &peer_str[28..]
                    ));
                    let _ = self.cmd_tx.blocking_send(TuiCommand::LoadConversations);
                }
                NetworkEvent::ConversationsUpdated(convos) => {
                    self.unread_count = convos.iter().map(|c| c.unread_count).sum();
                    self.conversations.set_conversations(convos);
                }
                NetworkEvent::MessagesLoaded(messages) => {
                    self.conversations.set_messages(messages);
                }
            }
        }

        if let Some(time) = self.status_time {
            if time.elapsed() > Duration::from_secs(3) {
                self.status_message = None;
                self.status_time = None;
            }
        }
    }

    fn set_status(&mut self, msg: &str) {
        self.status_message = Some(msg.to_string());
        self.status_time = Some(Instant::now());
    }

    fn draw(&mut self) -> io::Result<()> {
        let status_msg = self.status_message.clone();
        let announces_rx = self.announces_received;
        let announces_tx = self.announces_sent;
        let unread = self.unread_count;
        let keybinds = self.keybinds_for_tab();
        let tab = self.tab;
        let conv_pane = self.conversations.pane();
        let uses_textarea = conv_pane == ConversationPane::Compose
            || conv_pane == ConversationPane::NewConversation;

        self.terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

            let top_chunks =
                Layout::horizontal([Constraint::Min(40), Constraint::Length(35)]).split(chunks[0]);

            frame.render_widget(TabBar::new(tab), top_chunks[0]);

            let activity = Line::from(vec![
                if unread > 0 {
                    Span::styled(
                        format!("[{}] ", unread),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::raw("")
                },
                Span::styled("↓", Style::default().fg(Color::Green)),
                Span::raw(format!("{} ", announces_rx)),
                Span::styled("↑", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{} ", announces_tx)),
                if let Some(ref msg) = status_msg {
                    Span::styled(
                        msg,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled("●", Style::default().fg(Color::Green))
                },
            ]);
            let activity_widget = Paragraph::new(activity).alignment(Alignment::Right);
            frame.render_widget(activity_widget, top_chunks[1]);

            match tab {
                Tab::Conversations if uses_textarea => {
                    self.conversations.render_input_pane(frame, chunks[1]);
                }
                Tab::Conversations => frame.render_widget(&self.conversations, chunks[1]),
                Tab::Network => frame.render_widget(&mut self.network, chunks[1]),
            }

            let status = Paragraph::new(keybinds).style(Style::default().bg(Color::DarkGray));
            frame.render_widget(status, chunks[2]);
        })?;
        Ok(())
    }

    fn keybinds_for_tab(&self) -> Line<'static> {
        match self.tab {
            Tab::Conversations => self.conversation_keybinds(),
            Tab::Network => Line::from(vec![
                Span::styled("[C-l]", Style::default().fg(Color::Yellow)),
                Span::raw(" Nodes/Announces  "),
                Span::styled("[a]", Style::default().fg(Color::Yellow)),
                Span::raw(" Announce  "),
                Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
                Span::raw(" Connect  "),
                Span::styled("[q]", Style::default().fg(Color::Yellow)),
                Span::raw(" Quit"),
            ]),
        }
    }

    fn conversation_keybinds(&self) -> Line<'static> {
        match self.conversations.pane() {
            ConversationPane::List => Line::from(vec![
                Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
                Span::raw(" Switch  "),
                Span::styled("[j/k]", Style::default().fg(Color::Yellow)),
                Span::raw(" Navigate  "),
                Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
                Span::raw(" Open  "),
                Span::styled("[n]", Style::default().fg(Color::Yellow)),
                Span::raw(" New  "),
                Span::styled("[q]", Style::default().fg(Color::Yellow)),
                Span::raw(" Quit"),
            ]),
            ConversationPane::Messages => Line::from(vec![
                Span::styled("[j/k]", Style::default().fg(Color::Yellow)),
                Span::raw(" Scroll  "),
                Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
                Span::raw(" Reply  "),
                Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
                Span::raw(" Back  "),
            ]),
            ConversationPane::Compose => Line::from(vec![
                Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
                Span::raw(" Send  "),
                Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
                Span::raw(" Cancel  "),
            ]),
            ConversationPane::NewConversation => Line::from(vec![
                Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
                Span::raw(" Continue  "),
                Span::styled("[Esc]", Style::default().fg(Color::Yellow)),
                Span::raw(" Cancel  "),
            ]),
        }
    }

    fn handle_events(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;

            if let Event::Key(key) = &event {
                if key.kind != KeyEventKind::Press {
                    return Ok(());
                }
            }

            if self.tab == Tab::Conversations {
                use super::conversations::InputResult;
                match self.conversations.handle_event(&event) {
                    InputResult::Consumed => return Ok(()),
                    InputResult::Back => {
                        self.conversations.back();
                        return Ok(());
                    }
                    InputResult::SendMessage(content, dest) => {
                        let _ = self.cmd_tx.blocking_send(TuiCommand::SendMessage {
                            content,
                            destination: dest,
                        });
                        if let Some(peer) = self.conversations.selected_peer() {
                            let _ = self.cmd_tx.blocking_send(TuiCommand::LoadMessages(peer));
                        }
                        return Ok(());
                    }
                    InputResult::NotConsumed => {}
                }
            }

            if let Event::Key(key) = event {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

                match (key.code, ctrl) {
                    (KeyCode::Char('q'), false) => self.running = false,
                    (KeyCode::Char('c'), true) => self.running = false,
                    (KeyCode::Tab, false) => {
                        if self.tab == Tab::Conversations
                            && self.conversations.pane() != ConversationPane::List
                        {
                        } else {
                            self.tab = self.tab.next();
                        }
                    }
                    (KeyCode::BackTab, false) => self.tab = self.tab.prev(),
                    (KeyCode::Down | KeyCode::Char('j'), false) => self.handle_down(),
                    (KeyCode::Up | KeyCode::Char('k'), false) => self.handle_up(),
                    (KeyCode::Enter, false) => self.handle_enter(),
                    (KeyCode::Esc, false) => self.handle_escape(),
                    (KeyCode::Char('l'), true) => self.handle_ctrl_l(),
                    (KeyCode::Char('a'), false) => self.handle_announce(),
                    (KeyCode::Char('n'), false) => self.handle_new(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_down(&mut self) {
        match self.tab {
            Tab::Conversations => self.conversations.select_next(),
            Tab::Network => self.network.select_next(),
        }
    }

    fn handle_up(&mut self) {
        match self.tab {
            Tab::Conversations => self.conversations.select_prev(),
            Tab::Network => self.network.select_prev(),
        }
    }

    fn handle_enter(&mut self) {
        match self.tab {
            Tab::Conversations => {
                let was_list = self.conversations.pane() == ConversationPane::List;
                self.conversations.enter();
                if was_list {
                    if let Some(peer) = self.conversations.selected_peer() {
                        let _ = self.cmd_tx.blocking_send(TuiCommand::LoadMessages(peer));
                        let _ = self
                            .cmd_tx
                            .blocking_send(TuiCommand::MarkConversationRead(peer));
                    }
                }
            }
            Tab::Network => {
                if let Some((node, path)) = self.network.connect_selected() {
                    let _ = self
                        .cmd_tx
                        .blocking_send(TuiCommand::FetchPage { node, path });
                }
            }
        }
    }

    fn handle_escape(&mut self) {
        if self.tab == Tab::Conversations {
            self.conversations.back();
        }
    }

    fn handle_ctrl_l(&mut self) {
        if self.tab == Tab::Network {
            self.network.toggle_left_mode();
        }
    }

    fn handle_announce(&mut self) {
        if self.tab == Tab::Network {
            self.set_status("Sending announce command...");
            match self.cmd_tx.blocking_send(TuiCommand::Announce) {
                Ok(_) => {}
                Err(e) => self.set_status(&format!("Failed to send: {}", e)),
            }
        }
    }

    fn handle_new(&mut self) {
        if self.tab == Tab::Conversations && self.conversations.pane() == ConversationPane::List {
            self.conversations.start_new_conversation();
        }
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}
