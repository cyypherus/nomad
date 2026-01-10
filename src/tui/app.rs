use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Layout},
    prelude::CrosstermBackend,
    style::{Color, Style},
    widgets::Paragraph,
    Terminal,
};

use super::browser::BrowserView;
use super::conversations::ConversationsView;
use super::directory::DirectoryView;
use super::tabs::{Tab, TabBar};

pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: bool,
    tab: Tab,
    dest_hash: [u8; 16],
    conversations: ConversationsView,
    browser: BrowserView,
    directory: DirectoryView,
}

impl TuiApp {
    pub fn new(dest_hash: [u8; 16]) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            running: true,
            tab: Tab::default(),
            dest_hash,
            conversations: ConversationsView::new(),
            browser: BrowserView::new(),
            directory: DirectoryView::new(),
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        while self.running {
            self.draw()?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&mut self) -> io::Result<()> {
        self.terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

            frame.render_widget(TabBar::new(self.tab), chunks[0]);

            match self.tab {
                Tab::Conversations => frame.render_widget(&self.conversations, chunks[1]),
                Tab::Network => frame.render_widget(&self.browser, chunks[1]),
                Tab::Guide => frame.render_widget(&self.directory, chunks[1]),
            }

            let status = Paragraph::new(format!(" Address: {}", hex::encode(self.dest_hash)))
                .style(Style::default().fg(Color::Cyan).bg(Color::DarkGray));
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
                match key.code {
                    KeyCode::Char('q') => self.running = false,
                    KeyCode::Tab => self.tab = self.tab.next(),
                    KeyCode::BackTab => self.tab = self.tab.prev(),
                    KeyCode::Down | KeyCode::Char('j') => self.handle_down(),
                    KeyCode::Up | KeyCode::Char('k') => self.handle_up(),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_down(&mut self) {
        match self.tab {
            Tab::Conversations => self.conversations.select_next(),
            Tab::Network => self.browser.scroll_down(),
            Tab::Guide => self.directory.select_next(),
        }
    }

    fn handle_up(&mut self) {
        match self.tab {
            Tab::Conversations => self.conversations.select_prev(),
            Tab::Network => self.browser.scroll_up(),
            Tab::Guide => self.directory.select_prev(),
        }
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}
