use std::io::{self, Stdout};
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use lxmf::DESTINATION_LENGTH;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    prelude::{CrosstermBackend, Widget},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Terminal,
};
use tokio::sync::mpsc;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use super::browser_view::BrowserView;
use super::discovery::{DiscoveryView, ModalAction};
use super::modal::{Modal, ModalButton};
use super::mynode::MyNodeView;
use super::saved::{SavedModalAction, SavedView};
use super::status_bar::StatusBar;
use super::tabs::{Tab, TabBar};

use crate::network::NodeInfo;

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    NodeAnnounce(NodeInfo),
    AnnounceSent,
    Status(String),
    MessageReceived([u8; DESTINATION_LENGTH]),
    ConversationsUpdated(Vec<lxmf::ConversationInfo>),
    MessagesLoaded(Vec<lxmf::StoredMessage>),
    PageReceived { url: String, content: String },
    PageFailed { url: String, reason: String },
    RelayStats(reticulum::transport::TransportStatsSnapshot),
}

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
        form_data: std::collections::HashMap<String, String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppMode {
    Normal,
    Browser,
    Editing { field_name: String, masked: bool },
}

pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: bool,
    tab: Tab,
    tab_bar: TabBar,
    mode: AppMode,

    discovery: DiscoveryView,
    saved: SavedView,
    mynode: MyNodeView,
    browser: BrowserView,
    status_bar: StatusBar,
    input: Input,
    last_edit_popup_area: Rect,

    event_rx: mpsc::Receiver<NetworkEvent>,
    cmd_tx: mpsc::Sender<TuiCommand>,

    last_main_area: Rect,
}

impl TuiApp {
    pub fn new(
        dest_hash: [u8; 16],
        initial_nodes: Vec<NodeInfo>,
        relay_enabled: bool,
        event_rx: mpsc::Receiver<NetworkEvent>,
        cmd_tx: mpsc::Sender<TuiCommand>,
    ) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            Clear(ClearType::All)
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let discovery = DiscoveryView::new();
        let mut saved = SavedView::new();

        for node in initial_nodes {
            saved.add_node(node);
        }

        Ok(Self {
            terminal,
            running: true,
            tab: Tab::default(),
            tab_bar: TabBar::new(Tab::default()),
            mode: AppMode::Normal,
            discovery,
            saved,
            mynode: MyNodeView::new(dest_hash, relay_enabled),
            browser: BrowserView::new(),
            status_bar: StatusBar::new(),
            input: Input::default(),
            last_edit_popup_area: Rect::default(),
            event_rx,
            cmd_tx,
            last_main_area: Rect::default(),
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        while self.running {
            self.poll_events();
            self.status_bar.tick();
            self.draw()?;
            self.handle_input()?;
        }
        Ok(())
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                NetworkEvent::NodeAnnounce(node) => {
                    self.discovery.add_node(node);
                    self.mynode.increment_announces_received();
                    self.status_bar.set_status("Node discovered".into());
                }
                NetworkEvent::AnnounceSent => {
                    self.mynode.increment_announces_sent();
                    self.mynode.update_announce_time();
                    self.status_bar.set_status("Announced".into());
                }
                NetworkEvent::Status(msg) => {
                    self.status_bar.set_status(msg);
                }
                NetworkEvent::PageReceived { url, content } => {
                    self.browser.set_page_content(&url, &content);
                }
                NetworkEvent::PageFailed { url, reason } => {
                    self.browser.clear_loading();
                    self.status_bar
                        .set_status(format!("Failed to load {}: {}", url, reason));
                }
                NetworkEvent::MessageReceived(_)
                | NetworkEvent::ConversationsUpdated(_)
                | NetworkEvent::MessagesLoaded(_) => {}
                NetworkEvent::RelayStats(stats) => {
                    self.mynode.set_stats(stats.clone());
                    self.status_bar.set_relay_stats(stats);
                }
            }
        }
    }

    fn draw(&mut self) -> io::Result<()> {
        let tab = self.tab;
        let mode = self.mode.clone();
        let keybinds = self.keybinds_for_mode();
        let input_value = self.input.value().to_string();
        let input_cursor = self.input.visual_cursor();

        let mut main_area = Rect::default();
        let mut last_edit_popup_area = Rect::default();

        self.terminal.draw(|frame| {
            let area = frame.area();

            let chunks = Layout::vertical([
                Constraint::Length(2),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(area);

            let header_chunks =
                Layout::horizontal([Constraint::Min(40), Constraint::Length(30)]).split(chunks[0]);

            frame.render_widget(
                &mut self.tab_bar,
                Rect::new(
                    header_chunks[0].x,
                    header_chunks[0].y + 1,
                    header_chunks[0].width,
                    1,
                ),
            );

            let title = Line::from(vec![
                Span::styled(" \u{2726} ", Style::default().fg(Color::Magenta)),
                Span::styled(
                    "NOMAD",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" v0.1", Style::default().fg(Color::DarkGray)),
            ]);
            Paragraph::new(title).render(
                Rect::new(
                    header_chunks[0].x,
                    header_chunks[0].y,
                    header_chunks[0].width,
                    1,
                ),
                frame.buffer_mut(),
            );

            frame.render_widget(
                &self.status_bar,
                Rect::new(
                    header_chunks[1].x,
                    header_chunks[1].y,
                    header_chunks[1].width,
                    2,
                ),
            );

            main_area = chunks[1];

            match &mode {
                AppMode::Normal => match tab {
                    Tab::Discovery => frame.render_widget(&mut self.discovery, chunks[1]),
                    Tab::Saved => frame.render_widget(&mut self.saved, chunks[1]),
                    Tab::MyNode => frame.render_widget(&mut self.mynode, chunks[1]),
                },
                AppMode::Browser | AppMode::Editing { .. } => {
                    frame.render_widget(&mut self.browser, chunks[1]);
                }
            }

            if let AppMode::Editing { field_name, masked } = &mode {
                let inner_width = 50u16.saturating_sub(4);
                let scroll = input_cursor.saturating_sub(inner_width as usize);

                let display_value = if *masked {
                    "*".repeat(input_value.len())
                } else {
                    input_value.clone()
                };

                let scrolled_value: String = display_value.chars().skip(scroll).collect();

                let content = vec![
                    Line::from(Span::styled(
                        scrolled_value,
                        Style::default().fg(Color::White),
                    )),
                    Line::from(""),
                ];

                let modal = Modal::new(field_name)
                    .content(content)
                    .buttons(vec![
                        ModalButton::new("Confirm", Color::Green),
                        ModalButton::new("Cancel", Color::Red),
                    ])
                    .border_color(Color::Cyan);

                let popup_area = modal.render_centered(area, frame.buffer_mut(), 50, 6);
                last_edit_popup_area = popup_area;

                let inner_x = popup_area.x + 1;
                let inner_y = popup_area.y + 1;
                let cursor_x = inner_x + (input_cursor - scroll) as u16;
                frame.set_cursor_position((cursor_x, inner_y));
            }

            let footer =
                Paragraph::new(keybinds.clone()).style(Style::default().bg(Color::Rgb(20, 20, 30)));
            frame.render_widget(footer, chunks[2]);
        })?;

        self.last_main_area = main_area;
        self.last_edit_popup_area = last_edit_popup_area;

        Ok(())
    }

    fn keybinds_for_mode(&self) -> Line<'static> {
        match &self.mode {
            AppMode::Editing { .. } => Line::from(vec![
                Span::styled(" [Enter]", Style::default().fg(Color::Magenta)),
                Span::raw(" Confirm  "),
                Span::styled("[Esc]", Style::default().fg(Color::Magenta)),
                Span::raw(" Cancel  "),
            ]),
            AppMode::Browser => Line::from(vec![
                Span::styled(" [Esc]", Style::default().fg(Color::Magenta)),
                Span::raw(" Back  "),
                Span::styled("[j/k]", Style::default().fg(Color::Magenta)),
                Span::raw(" Scroll  "),
                Span::styled("[Tab]", Style::default().fg(Color::Magenta)),
                Span::raw(" Next  "),
                Span::styled("[Enter]", Style::default().fg(Color::Magenta)),
                Span::raw(" Activate  "),
                Span::styled("[r]", Style::default().fg(Color::Magenta)),
                Span::raw(" Reload  "),
            ]),
            AppMode::Normal => match self.tab {
                Tab::Discovery => {
                    if self.discovery.is_modal_open() {
                        Line::from(vec![
                            Span::styled(" [Tab]", Style::default().fg(Color::Magenta)),
                            Span::raw(" Switch  "),
                            Span::styled("[Enter]", Style::default().fg(Color::Magenta)),
                            Span::raw(" Select  "),
                            Span::styled("[Esc]", Style::default().fg(Color::Magenta)),
                            Span::raw(" Cancel  "),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled(" [j/k]", Style::default().fg(Color::Magenta)),
                            Span::raw(" Navigate  "),
                            Span::styled("[Enter]", Style::default().fg(Color::Magenta)),
                            Span::raw(" Open  "),
                            Span::styled("[Tab]", Style::default().fg(Color::Magenta)),
                            Span::raw(" Switch Tab  "),
                            Span::styled("[q]", Style::default().fg(Color::Magenta)),
                            Span::raw(" Quit  "),
                        ])
                    }
                }
                Tab::Saved => Line::from(vec![
                    Span::styled(" [j/k]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Navigate  "),
                    Span::styled("[Enter]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Connect  "),
                    Span::styled("[d]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Remove  "),
                    Span::styled("[Tab]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Switch Tab  "),
                    Span::styled("[q]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Quit  "),
                ]),
                Tab::MyNode => Line::from(vec![
                    Span::styled(" [a]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Announce  "),
                    Span::styled("[Tab]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Switch Tab  "),
                    Span::styled("[q]", Style::default().fg(Color::Magenta)),
                    Span::raw(" Quit  "),
                ]),
            },
        }
    }

    fn handle_input(&mut self) -> io::Result<()> {
        if !event::poll(Duration::from_millis(50))? {
            return Ok(());
        }

        let evt = event::read()?;

        if let Event::Key(key) = &evt {
            if key.kind != KeyEventKind::Press {
                return Ok(());
            }
        }

        match &evt {
            Event::Key(key) => {
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

                if key.code == KeyCode::Char('c') && ctrl {
                    self.running = false;
                    return Ok(());
                }

                match &self.mode {
                    AppMode::Editing { .. } => self.handle_editing_key(&evt),
                    AppMode::Browser => self.handle_browser_key(key.code),
                    AppMode::Normal => self.handle_normal_key(key.code, ctrl),
                }
            }
            Event::Mouse(mouse) => {
                if !matches!(self.mode, AppMode::Editing { .. }) {
                    self.handle_mouse(mouse.kind, mouse.column, mouse.row);
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_normal_key(&mut self, code: KeyCode, _ctrl: bool) {
        if self.discovery.is_modal_open() {
            match code {
                KeyCode::Esc => self.discovery.close_modal(),
                KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => self.discovery.select_next(),
                KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => self.discovery.select_prev(),
                KeyCode::Enter => {
                    let action = self.discovery.modal_action();
                    self.handle_modal_action(action);
                }
                _ => {}
            }
            return;
        }

        if self.saved.is_modal_open() {
            match code {
                KeyCode::Esc => self.saved.close_modal(),
                KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => self.saved.select_next(),
                KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => self.saved.select_prev(),
                KeyCode::Enter => {
                    let action = self.saved.modal_action();
                    self.handle_saved_modal_action(action);
                }
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Char('q') => self.running = false,
            KeyCode::Tab => {
                self.tab = self.tab.next();
                self.tab_bar = TabBar::new(self.tab);
            }
            KeyCode::BackTab => {
                self.tab = self.tab.prev();
                self.tab_bar = TabBar::new(self.tab);
            }
            KeyCode::Down | KeyCode::Char('j') => self.handle_down(),
            KeyCode::Up | KeyCode::Char('k') => self.handle_up(),
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Char('a') => self.handle_announce(),
            KeyCode::Char('d') => self.handle_delete(),
            _ => {}
        }
    }

    fn handle_browser_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.mode = AppMode::Normal;
            }
            KeyCode::Down | KeyCode::Char('j') => self.browser.scroll_down(),
            KeyCode::Up | KeyCode::Char('k') => self.browser.scroll_up(),
            KeyCode::PageDown => self.browser.scroll_page_down(),
            KeyCode::PageUp => self.browser.scroll_page_up(),
            KeyCode::Tab => self.browser.select_next(),
            KeyCode::BackTab => self.browser.select_prev(),
            KeyCode::Left => self.browser.select_prev(),
            KeyCode::Right => self.browser.select_next(),
            KeyCode::Enter => {
                if let Some(interaction) = self.browser.interact() {
                    self.handle_interaction(interaction);
                }
            }
            KeyCode::Backspace => {
                self.browser.go_back();
            }
            KeyCode::Char('r') => {
                self.reload_page();
            }
            _ => {}
        }
    }

    fn handle_editing_key(&mut self, evt: &Event) {
        if let Event::Key(key) = evt {
            match key.code {
                KeyCode::Enter => {
                    self.confirm_edit();
                }
                KeyCode::Esc => {
                    self.cancel_edit();
                }
                _ => {
                    self.input.handle_event(evt);
                }
            }
        }
    }

    fn confirm_edit(&mut self) {
        if let AppMode::Editing { field_name, .. } = &self.mode {
            let value = self.input.value().to_string();
            let name = field_name.clone();
            self.browser.set_field_value(&name, value);
        }
        self.input.reset();
        self.mode = AppMode::Browser;
    }

    fn cancel_edit(&mut self) {
        self.input.reset();
        self.mode = AppMode::Browser;
    }

    fn handle_edit_modal_click(&mut self, x: u16, y: u16) {
        let modal = Modal::new("")
            .buttons(vec![
                ModalButton::new("Confirm", Color::Green),
                ModalButton::new("Cancel", Color::Red),
            ])
            .selected(0);

        if let Some(idx) = modal.hit_test_buttons(x, y, self.last_edit_popup_area) {
            match idx {
                0 => self.confirm_edit(),
                1 => self.cancel_edit(),
                _ => {}
            }
        }
    }

    fn handle_interaction(&mut self, interaction: micronaut::Interaction) {
        match interaction {
            micronaut::Interaction::Link(link) => {
                self.navigate_to_link(link);
            }
            micronaut::Interaction::EditField(field) => {
                self.input = Input::new(field.value);
                self.mode = AppMode::Editing {
                    field_name: field.name,
                    masked: field.masked,
                };
            }
        }
    }

    fn reload_page(&mut self) {
        if let Some(node) = self.browser.current_node().cloned() {
            if let Some(url) = self.browser.current_url() {
                let path = url.to_string();
                self.browser.set_loading(path.clone());
                let _ = self.cmd_tx.blocking_send(TuiCommand::FetchPage {
                    node,
                    path,
                    form_data: std::collections::HashMap::new(),
                });
            }
        }
    }

    fn handle_mouse(&mut self, kind: MouseEventKind, x: u16, y: u16) {
        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if y == 1 {
                    if let Some(tab) = self.tab_bar.hit_test(x) {
                        self.tab = tab;
                        self.tab_bar = TabBar::new(tab);
                        self.mode = AppMode::Normal;
                        return;
                    }
                }

                match &self.mode {
                    AppMode::Browser => {
                        use super::browser_view::NavAction;
                        if let Some(nav) = self.browser.click_nav(x, y) {
                            match nav {
                                NavAction::Back => {
                                    self.browser.go_back();
                                }
                                NavAction::Forward => {
                                    self.browser.go_forward();
                                }
                                NavAction::Reload => {
                                    self.reload_page();
                                }
                            }
                        } else if let Some(interaction) = self.browser.click(x, y) {
                            self.handle_interaction(interaction);
                        }
                    }
                    AppMode::Editing { .. } => {
                        self.handle_edit_modal_click(x, y);
                    }
                    AppMode::Normal => {
                        if self.discovery.is_modal_open() {
                            let modal_action =
                                self.discovery.click_modal(x, y, self.last_main_area);
                            if modal_action != ModalAction::None {
                                self.handle_modal_action(modal_action);
                            }
                            return;
                        }

                        if self.saved.is_modal_open() {
                            let modal_action = self.saved.click_modal(x, y, self.last_main_area);
                            if modal_action != SavedModalAction::None {
                                self.handle_saved_modal_action(modal_action);
                            }
                            return;
                        }

                        match self.tab {
                            Tab::Discovery => {
                                if self.discovery.click(x, y, self.last_main_area).is_some() {
                                    self.discovery.open_modal();
                                }
                            }
                            Tab::Saved => {
                                if self.saved.click(x, y, self.last_main_area).is_some() {
                                    self.saved.open_modal();
                                }
                            }
                            Tab::MyNode => {
                                if self.mynode.click(x, y) {
                                    self.send_announce();
                                }
                            }
                        }
                    }
                }
            }
            MouseEventKind::ScrollUp => match &self.mode {
                AppMode::Browser => self.browser.scroll_up(),
                AppMode::Normal => self.handle_up(),
                AppMode::Editing { .. } => {}
            },
            MouseEventKind::ScrollDown => match &self.mode {
                AppMode::Browser => self.browser.scroll_down(),
                AppMode::Normal => self.handle_down(),
                AppMode::Editing { .. } => {}
            },
            _ => {}
        }
    }

    fn handle_down(&mut self) {
        match self.tab {
            Tab::Discovery => self.discovery.select_next(),
            Tab::Saved => self.saved.select_next(),
            Tab::MyNode => {}
        }
    }

    fn handle_up(&mut self) {
        match self.tab {
            Tab::Discovery => self.discovery.select_prev(),
            Tab::Saved => self.saved.select_prev(),
            Tab::MyNode => {}
        }
    }

    fn handle_enter(&mut self) {
        match self.tab {
            Tab::Discovery => {
                self.discovery.open_modal();
            }
            Tab::Saved => {
                self.saved.open_modal();
            }
            Tab::MyNode => {
                self.send_announce();
            }
        }
    }

    fn handle_modal_action(&mut self, action: ModalAction) {
        match action {
            ModalAction::Connect => {
                if let Some(node) = self.discovery.selected_node().cloned() {
                    self.discovery.close_modal();
                    self.connect_to_node(&node);
                }
            }
            ModalAction::Save => {
                if let Some(node) = self.discovery.selected_node().cloned() {
                    self.saved.add_node(node);
                    self.discovery.close_modal();
                    self.status_bar.set_status("Node saved".into());
                }
            }
            ModalAction::Dismiss | ModalAction::None => {
                self.discovery.close_modal();
            }
        }
    }

    fn handle_saved_modal_action(&mut self, action: SavedModalAction) {
        match action {
            SavedModalAction::Connect => {
                if let Some(node) = self.saved.selected_node().cloned() {
                    self.saved.close_modal();
                    self.connect_to_node(&node);
                }
            }
            SavedModalAction::Delete => {
                if let Some(removed) = self.saved.remove_selected() {
                    self.saved.close_modal();
                    self.status_bar
                        .set_status(format!("Removed {}", removed.name));
                }
            }
            SavedModalAction::Cancel | SavedModalAction::None => {
                self.saved.close_modal();
            }
        }
    }

    fn connect_to_node(&mut self, node: &NodeInfo) {
        let path = "/page/index.mu".to_string();
        self.browser.set_current_node(node.clone());
        self.mode = AppMode::Browser;

        let _ = self.cmd_tx.blocking_send(TuiCommand::FetchPage {
            node: node.clone(),
            path,
            form_data: std::collections::HashMap::new(),
        });

        self.status_bar
            .set_status(format!("Connecting to {}...", node.name));
    }

    fn navigate_to_link(&mut self, link: micronaut::Link) {
        let all_nodes: Vec<NodeInfo> = self
            .discovery
            .nodes()
            .iter()
            .chain(self.saved.nodes().iter())
            .cloned()
            .collect();

        if let Some((node, path)) = self.browser.resolve_link(&link, &all_nodes) {
            self.browser.set_current_node(node.clone());
            self.browser.set_loading(path.clone());
            let _ = self.cmd_tx.blocking_send(TuiCommand::FetchPage {
                node,
                path,
                form_data: link.form_data,
            });
        }
    }

    fn handle_announce(&mut self) {
        if self.tab == Tab::MyNode {
            self.send_announce();
        }
    }

    fn send_announce(&mut self) {
        self.status_bar.set_status("Sending announce...".into());
        let _ = self.cmd_tx.blocking_send(TuiCommand::Announce);
    }

    fn handle_delete(&mut self) {
        if self.tab == Tab::Saved {
            self.saved.open_delete_modal();
        }
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableMouseCapture,
            LeaveAlternateScreen
        );
    }
}
