use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{
    layout::{Constraint, Layout},
    prelude::CrosstermBackend,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};
use tokio::sync::mpsc;

use super::conversations::ConversationsView;
use super::network::NetworkView;
use super::tabs::{Tab, TabBar};

pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: bool,
    tab: Tab,
    dest_hash: [u8; 16],
    conversations: ConversationsView,
    network: NetworkView,
    announce_rx: mpsc::Receiver<[u8; 16]>,
}

impl TuiApp {
    pub fn new(dest_hash: [u8; 16], announce_rx: mpsc::Receiver<[u8; 16]>) -> io::Result<Self> {
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
            dest_hash,
            conversations: ConversationsView::new(),
            network: NetworkView::new(dest_hash),
            announce_rx,
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        while self.running {
            self.poll_announces();
            self.draw()?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn poll_announces(&mut self) {
        while let Ok(hash) = self.announce_rx.try_recv() {
            self.network.add_announce(hash);
        }
    }

    fn draw(&mut self) -> io::Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::vertical([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

            frame.render_widget(TabBar::new(self.tab), chunks[0]);

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
                    Span::styled("[Enter]", Style::default().fg(Color::Yellow)),
                    Span::raw(" Connect  "),
                    Span::styled("[C-r]", Style::default().fg(Color::Yellow)),
                    Span::raw(" Reload  "),
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
            Tab::Network => self.network.connect_selected(),
        }
    }

    fn handle_ctrl_l(&mut self) {
        if self.tab == Tab::Network {
            self.network.toggle_left_mode();
        }
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}
