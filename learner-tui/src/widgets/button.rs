use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonVisual {
    Normal,
    Hover,
    Active,
}

pub struct ButtonState {
    pub label: String,
    pub area: Rect,
    pub visual: ButtonVisual,
}

impl ButtonState {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            area: Rect::default(),
            visual: ButtonVisual::Normal,
        }
    }

    pub fn hit_test(&self, col: u16, row: u16) -> bool {
        self.area.width > 0
            && col >= self.area.x && col < self.area.x + self.area.width
            && row >= self.area.y && row < self.area.y + self.area.height
    }
}

pub fn render_button(f: &mut Frame, state: &mut ButtonState, area: Rect) {
    state.area = area;
    let style = match state.visual {
        ButtonVisual::Normal => theme::BTN_NORMAL,
        ButtonVisual::Hover => theme::BTN_HOVER,
        ButtonVisual::Active => theme::BTN_ACTIVE,
    };
    let text = format!("[ {} ]", state.label);
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(text, style)])),
        area,
    );
}
