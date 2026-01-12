use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
};

#[derive(Clone)]
pub struct Button {
    pub label: String,
    pub color: Color,
    pub area: Rect,
}

impl Button {
    pub fn new(label: impl Into<String>, color: Color) -> Self {
        Self {
            label: label.into(),
            color,
            area: Rect::default(),
        }
    }

    pub fn render(&mut self, x: u16, y: u16, buf: &mut Buffer) -> u16 {
        let label = format!(" {} ", self.label);
        let width = label.len() as u16;

        self.area = Rect::new(x, y, width, 1);

        let style = Style::default()
            .fg(Color::Black)
            .bg(self.color)
            .add_modifier(Modifier::BOLD);

        buf.set_string(x, y, &label, style);
        width
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        self.area.contains((x, y).into())
    }
}

pub struct ButtonRow {
    buttons: Vec<Button>,
    spacing: u16,
}

impl ButtonRow {
    pub fn new(buttons: Vec<Button>) -> Self {
        Self {
            buttons,
            spacing: 2,
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let total_width: u16 = self
            .buttons
            .iter()
            .map(|b| b.label.len() as u16 + 2)
            .sum::<u16>()
            + self.spacing * (self.buttons.len().saturating_sub(1)) as u16;

        let start_x = area.x + (area.width.saturating_sub(total_width)) / 2;
        let y = area.y;

        let mut cur_x = start_x;
        for button in &mut self.buttons {
            let width = button.render(cur_x, y, buf);
            cur_x += width + self.spacing;
        }
    }

    pub fn render_left(&mut self, x: u16, y: u16, buf: &mut Buffer) {
        let mut cur_x = x;
        for button in &mut self.buttons {
            let width = button.render(cur_x, y, buf);
            cur_x += width + self.spacing;
        }
    }

    pub fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        self.buttons.iter().position(|b| b.contains(x, y))
    }
}
