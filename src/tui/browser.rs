use micron::{parse, render, Document, RenderConfig};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Widget},
};

pub struct BrowserView {
    address: String,
    document: Option<Document>,
    scroll: u16,
}

impl Default for BrowserView {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserView {
    pub fn new() -> Self {
        let sample = r#">Welcome to Nomad
-
This is the `!Network`* browser.

Enter a node address above to visit a page.

`[Visit Example Node`nomad://example]
"#;
        Self {
            address: String::new(),
            document: Some(parse(sample)),
            scroll: 0,
        }
    }

    pub fn set_address(&mut self, addr: String) {
        self.address = addr;
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

impl Widget for &BrowserView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);

        let address_bar = Paragraph::new(format!(" {}", self.address))
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Address")
                    .style(Style::default()),
            );
        address_bar.render(chunks[0], buf);

        let content_block = Block::default().borders(Borders::ALL).title("Page");
        let inner = content_block.inner(chunks[1]);
        content_block.render(chunks[1], buf);

        if let Some(doc) = &self.document {
            let config = RenderConfig {
                width: inner.width,
                ..Default::default()
            };
            let text = render(doc, &config);
            let paragraph = Paragraph::new(text).scroll((self.scroll, 0));
            paragraph.render(inner, buf);
        } else {
            let empty = Paragraph::new("No page loaded");
            empty.render(inner, buf);
        }
    }
}
