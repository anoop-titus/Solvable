use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use crate::theme;

pub struct TextInputState {
    pub value: String,
    pub cursor_pos: usize,
    pub focused: bool,
    pub masked: bool,
    pub placeholder: String,
    pub label: String,
    pub area: Rect,
}

impl TextInputState {
    pub fn new(label: &str, masked: bool, placeholder: &str) -> Self {
        Self {
            value: String::new(),
            cursor_pos: 0,
            focused: false,
            masked,
            placeholder: placeholder.to_string(),
            label: label.to_string(),
            area: Rect::default(),
        }
    }

    pub fn display_value(&self) -> String {
        if self.value.is_empty() {
            return String::new();
        }
        if !self.masked {
            return self.value.clone();
        }
        // Show first 4 + last 4, mask middle
        let chars: Vec<char> = self.value.chars().collect();
        if chars.len() <= 8 {
            return "\u{2022}".repeat(chars.len());
        }
        let first: String = chars[..4].iter().collect();
        let last: String = chars[chars.len()-4..].iter().collect();
        format!("{}{}{}", first, "\u{2022}".repeat(chars.len() - 8), last)
    }

    pub fn insert_char(&mut self, c: char) {
        let byte_idx = self.value.char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len());
        self.value.insert(byte_idx, c);
        self.cursor_pos += 1;
    }

    pub fn delete_char_before(&mut self) {
        if self.cursor_pos > 0 {
            let byte_idx = self.value.char_indices()
                .nth(self.cursor_pos - 1)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.value.remove(byte_idx);
            self.cursor_pos -= 1;
        }
    }

    pub fn delete_char_at(&mut self) {
        let char_count = self.value.chars().count();
        if self.cursor_pos < char_count {
            let byte_idx = self.value.char_indices()
                .nth(self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(self.value.len());
            self.value.remove(byte_idx);
        }
    }

    pub fn move_cursor_left(&mut self) {
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        let max = self.value.chars().count();
        self.cursor_pos = (self.cursor_pos + 1).min(max);
    }

    pub fn move_cursor_home(&mut self) { self.cursor_pos = 0; }
    pub fn move_cursor_end(&mut self) { self.cursor_pos = self.value.chars().count(); }
}

pub fn render_text_input(f: &mut Frame, state: &mut TextInputState, area: Rect) {
    state.area = area;

    let border_style = if state.focused { theme::INPUT_BORDER_FOCUSED } else { theme::INPUT_BORDER };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} ", state.label),
            if state.focused { theme::INPUT_BORDER_FOCUSED.add_modifier(Modifier::BOLD) } else { theme::LABEL },
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if state.value.is_empty() && !state.focused {
        f.render_widget(
            Paragraph::new(Span::styled(&state.placeholder, theme::INPUT_BORDER)),
            inner,
        );
    } else {
        let display = state.display_value();
        if state.focused {
            let cursor_display_pos = if state.masked {
                state.cursor_pos.min(display.chars().count())
            } else {
                state.cursor_pos
            };
            f.render_widget(Paragraph::new(Line::from(vec![Span::styled(&display, theme::INPUT_TEXT)])), inner);
            // Set cursor position for blinking
            let cursor_x = inner.x + cursor_display_pos as u16;
            if cursor_x < inner.x + inner.width {
                f.set_cursor_position((cursor_x, inner.y));
            }
        } else {
            f.render_widget(Paragraph::new(Line::from(vec![Span::styled(display, theme::INPUT_TEXT)])), inner);
        }
    }
}
