use crate::network::NodeInfo;
use micronaut::{Browser, Interaction, Link, RatatuiRenderer};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    Back,
    Forward,
    Reload,
}

pub struct BrowserView {
    browser: Browser<RatatuiRenderer>,
    current_node: Option<NodeInfo>,
    loading_url: Option<String>,
    last_content_area: Rect,
    last_back_btn_area: Rect,
    last_fwd_btn_area: Rect,
    last_reload_btn_area: Rect,
}

impl Default for BrowserView {
    fn default() -> Self {
        Self::new()
    }
}

impl BrowserView {
    pub fn new() -> Self {
        Self {
            browser: Browser::new(RatatuiRenderer),
            current_node: None,
            loading_url: None,
            last_content_area: Rect::default(),
            last_back_btn_area: Rect::default(),
            last_fwd_btn_area: Rect::default(),
            last_reload_btn_area: Rect::default(),
        }
    }

    pub fn current_node(&self) -> Option<&NodeInfo> {
        self.current_node.as_ref()
    }

    pub fn set_current_node(&mut self, node: NodeInfo) {
        self.current_node = Some(node);
    }

    pub fn set_page_content(&mut self, url: &str, content: &str) {
        self.loading_url = None;
        self.browser.set_content(url, content);
    }

    pub fn set_loading(&mut self, url: String) {
        self.loading_url = Some(url);
        self.browser.clear();
    }

    pub fn clear_loading(&mut self) {
        self.loading_url = None;
    }

    pub fn scroll_up(&mut self) {
        self.browser.scroll_by(-1);
    }

    pub fn scroll_down(&mut self) {
        self.browser.scroll_by(1);
    }

    pub fn scroll_page_up(&mut self) {
        self.browser
            .scroll_by(-(self.last_content_area.height as i32));
    }

    pub fn scroll_page_down(&mut self) {
        self.browser.scroll_by(self.last_content_area.height as i32);
    }

    pub fn select_next(&mut self) {
        self.browser.select_next();
    }

    pub fn select_prev(&mut self) {
        self.browser.select_prev();
    }

    pub fn interact(&mut self) -> Option<Interaction> {
        self.browser.interact()
    }

    pub fn go_back(&mut self) -> bool {
        self.browser.back()
    }

    pub fn go_forward(&mut self) -> bool {
        self.browser.forward()
    }

    pub fn click_nav(&mut self, x: u16, y: u16) -> Option<NavAction> {
        if self.last_back_btn_area.intersects(Rect::new(x, y, 1, 1)) && self.browser.can_go_back() {
            return Some(NavAction::Back);
        }
        if self.last_fwd_btn_area.intersects(Rect::new(x, y, 1, 1)) && self.browser.can_go_forward()
        {
            return Some(NavAction::Forward);
        }
        if self.last_reload_btn_area.intersects(Rect::new(x, y, 1, 1))
            && self.browser.url().is_some()
        {
            return Some(NavAction::Reload);
        }
        None
    }

    pub fn current_url(&self) -> Option<&str> {
        self.browser.url()
    }

    pub fn click(&mut self, x: u16, y: u16) -> Option<Interaction> {
        let area = self.last_content_area;
        if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
            let rel_x = x - area.x;
            let rel_y = y - area.y;
            self.browser.click(rel_x, rel_y)
        } else {
            None
        }
    }

    pub fn resolve_link(
        &self,
        link: &Link,
        known_nodes: &[NodeInfo],
    ) -> super::link_handler::LinkAction {
        super::link_handler::resolve_link(&link.url, self.current_node.as_ref(), known_nodes)
    }

    pub fn last_content_area(&self) -> Rect {
        self.last_content_area
    }

    pub fn set_field_value(&mut self, name: &str, value: String) {
        self.browser.set_field_value(name, value);
    }
}

impl Widget for &mut BrowserView {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = self
            .current_node
            .as_ref()
            .map(|n| n.name.clone())
            .unwrap_or_else(|| "Browser".to_string());

        let title_color = if self.browser.url().is_some() {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(
                    title,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ]))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(title_color));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 3 {
            return;
        }

        let back_style = if self.browser.can_go_back() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let fwd_style = if self.browser.can_go_forward() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let reload_style = if self.browser.url().is_some() {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let url_span = if let Some(loading) = &self.loading_url {
            vec![
                Span::styled("Loading: ", Style::default().fg(Color::Yellow)),
                Span::styled(loading.clone(), Style::default().fg(Color::Yellow)),
            ]
        } else if let Some(url) = self.browser.url() {
            vec![Span::styled(
                url.to_string(),
                Style::default().fg(Color::DarkGray),
            )]
        } else {
            vec![Span::styled(
                "No page loaded",
                Style::default().fg(Color::DarkGray),
            )]
        };

        let mut nav_spans = vec![
            Span::styled("[\u{25c0}]", back_style),
            Span::raw(" "),
            Span::styled("[\u{25b6}]", fwd_style),
            Span::raw(" "),
            Span::styled("[\u{21bb}]", reload_style),
            Span::raw("  "),
        ];
        nav_spans.extend(url_span);

        let nav_bar = Line::from(nav_spans);

        self.last_back_btn_area = Rect::new(inner.x, inner.y, 3, 1);
        self.last_fwd_btn_area = Rect::new(inner.x + 4, inner.y, 3, 1);
        self.last_reload_btn_area = Rect::new(inner.x + 8, inner.y, 3, 1);

        let url_area = Rect::new(inner.x, inner.y, inner.width, 1);
        Paragraph::new(nav_bar).render(url_area, buf);

        let divider_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);
        let divider = "\u{2500}".repeat(inner.width as usize);
        Paragraph::new(Line::from(Span::styled(
            divider,
            Style::default().fg(Color::DarkGray),
        )))
        .render(divider_area, buf);

        let content_area = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );

        self.last_content_area = content_area;
        self.browser.resize(content_area.width, content_area.height);
        if let Some(paragraph) = self.browser.render() {
            paragraph.clone().render(content_area, buf);
        } else {
            Paragraph::new("No content")
                .style(Style::default().fg(Color::DarkGray))
                .render(content_area, buf);
        }

        if let Some(link_url) = self.browser.selected_link() {
            if content_area.height > 1 {
                let status_y = content_area.y + content_area.height - 1;
                let status_area = Rect::new(content_area.x, status_y, content_area.width, 1);
                let status_text = format!("\u{2192} {}", link_url);
                Paragraph::new(status_text)
                    .style(Style::default().fg(Color::DarkGray).bg(Color::Black))
                    .render(status_area, buf);
            }
        }
    }
}
