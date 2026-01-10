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
use ratatui::{
    layout::{Alignment, Constraint, Layout},
    prelude::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};
use tokio::sync::mpsc;

use super::conversations::ConversationsView;
use super::network::NetworkView;
use super::tabs::{Tab, TabBar};

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    AnnounceReceived([u8; 16]),
    AnnounceSent,
    Status(String),
    PageReceived { url: String, content: String },
    ConnectionFailed { url: String, reason: String },
}

#[derive(Debug, Clone)]
pub enum TuiCommand {
    Announce,
    ConnectToNode { hash: [u8; 16], path: String },
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
}

impl TuiApp {
    pub fn new(
        dest_hash: [u8; 16],
        event_rx: mpsc::Receiver<NetworkEvent>,
        cmd_tx: mpsc::Sender<TuiCommand>,
    ) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, Clear(ClearType::All))?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        Ok(Self {
            terminal,
            running: true,
            tab: Tab::default(),
            conversations: ConversationsView::new(),
            network: NetworkView::new(dest_hash),
            event_rx,
            cmd_tx,
            status_message: None,
            status_time: None,
            announces_received: 0,
            announces_sent: 0,
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
                NetworkEvent::AnnounceReceived(hash) => {
                    self.network.add_announce(hash);
                    self.announces_received += 1;
                    self.set_status("Announce received");
                }
                NetworkEvent::AnnounceSent => {
                    self.announces_sent += 1;
                    self.network.update_announce_time();
                }
                NetworkEvent::Status(msg) => {
                    self.set_status(&msg);
                }
                NetworkEvent::PageReceived { url, content } => {
                    self.network.set_page_content(url, content);
                }
                NetworkEvent::ConnectionFailed { url, reason } => {
                    self.network.set_connection_failed(url, reason);
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

        self.terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

            let top_chunks =
                Layout::horizontal([Constraint::Min(40), Constraint::Length(30)]).split(chunks[0]);

            frame.render_widget(TabBar::new(self.tab), top_chunks[0]);

            let activity = Line::from(vec![
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

            match self.tab {
                Tab::Conversations => frame.render_widget(&self.conversations, chunks[1]),
                Tab::Network => frame.render_widget(&self.network, chunks[1]),
            }

            let keybinds = match self.tab {
                Tab::Conversations => Line::from(vec![
                    Span::styled("[Tab]", Style::default().fg(Color::Yellow)),
                    Span::raw(" Switch  "),
                    Span::styled("[j/k]", Style::default().fg(Color::Yellow)),
                    Span::raw(" Navigate  "),
                    Span::styled("[q]", Style::default().fg(Color::Yellow)),
                    Span::raw(" Quit"),
                ]),
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
            };

            let status = Paragraph::new(keybinds).style(Style::default().bg(Color::DarkGray));
            frame.render_widget(status, chunks[2]);
        })?;
        Ok(())
    }

    fn handle_events(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    return Ok(());
                }

                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

                match (key.code, ctrl) {
                    (KeyCode::Char('q'), false) => self.running = false,
                    (KeyCode::Tab, false) => self.tab = self.tab.next(),
                    (KeyCode::BackTab, false) => self.tab = self.tab.prev(),
                    (KeyCode::Down | KeyCode::Char('j'), false) => self.handle_down(),
                    (KeyCode::Up | KeyCode::Char('k'), false) => self.handle_up(),
                    (KeyCode::Enter, false) => self.handle_enter(),
                    (KeyCode::Char('l'), true) => self.handle_ctrl_l(),
                    (KeyCode::Char('a'), false) => self.handle_announce(),
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
            Tab::Conversations => {}
            Tab::Network => {
                if let Some((hash, path)) = self.network.connect_selected() {
                    let _ = self
                        .cmd_tx
                        .blocking_send(TuiCommand::ConnectToNode { hash, path });
                }
            }
        }
    }

    fn handle_ctrl_l(&mut self) {
        if self.tab == Tab::Network {
            self.network.toggle_left_mode();
        }
    }

    fn handle_announce(&mut self) {
        if self.tab == Tab::Network {
            let _ = self.cmd_tx.blocking_send(TuiCommand::Announce);
        }
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}
