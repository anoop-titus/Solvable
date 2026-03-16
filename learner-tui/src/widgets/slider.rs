use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::theme;

/// State for a single horizontal slider with red/yellow/green zones.
pub struct SliderState {
    pub label: String,
    pub value: u32,
    pub min: u32,
    pub max: u32,
    pub yellow_threshold: u32,
    pub red_threshold: u32,
    pub focused: bool,
    pub area: Rect,           // entire row area (for hit testing)
    pub bar_area: Rect,       // just the slider bar (for click-to-set)
    pub editing_number: bool, // if true, user is typing in the numeric box
    pub number_input: String,
}

impl SliderState {
    pub fn new(label: &str, value: u32, min: u32, max: u32, yellow: u32, red: u32) -> Self {
        Self {
            label: label.to_string(),
            value,
            min,
            max,
            yellow_threshold: yellow,
            red_threshold: red,
            focused: false,
            area: Rect::default(),
            bar_area: Rect::default(),
            editing_number: false,
            number_input: String::new(),
        }
    }

    /// Increase value by 1 (clamped to max).
    pub fn increment(&mut self) {
        self.value = (self.value + 1).min(self.max);
    }

    /// Decrease value by 1 (clamped to min).
    pub fn decrement(&mut self) {
        self.value = self.value.saturating_sub(1).max(self.min);
    }

    /// Set value from a click position on the bar area.
    pub fn set_from_click(&mut self, col: u16) {
        if self.bar_area.width == 0 { return; }
        let offset = col.saturating_sub(self.bar_area.x);
        let ratio = offset as f64 / self.bar_area.width as f64;
        let new_val = self.min as f64 + ratio * (self.max - self.min) as f64;
        self.value = (new_val.round() as u32).max(self.min).min(self.max);
    }

    /// Hit test for the entire slider row.
    pub fn hit_test(&self, col: u16, row: u16) -> bool {
        self.area.width > 0
            && col >= self.area.x && col < self.area.x + self.area.width
            && row >= self.area.y && row < self.area.y + self.area.height
    }

    /// Hit test specifically for the bar region (click-to-set).
    pub fn bar_hit_test(&self, col: u16, row: u16) -> bool {
        self.bar_area.width > 0
            && col >= self.bar_area.x && col < self.bar_area.x + self.bar_area.width
            && row >= self.bar_area.y && row < self.bar_area.y + self.bar_area.height
    }

    /// Hit test for the numeric box region (right side).
    pub fn numbox_hit_test(&self, col: u16, row: u16) -> bool {
        let numbox_x = self.bar_area.x + self.bar_area.width + 6; // after "  NNN "
        let numbox_w = 5u16;
        col >= numbox_x && col < numbox_x + numbox_w
            && row >= self.area.y && row < self.area.y + self.area.height
    }

    /// Start editing the numeric input box.
    pub fn start_editing(&mut self) {
        self.editing_number = true;
        self.number_input = self.value.to_string();
    }

    /// Commit typed number to value.
    pub fn commit_edit(&mut self) {
        if let Ok(v) = self.number_input.parse::<u32>() {
            self.value = v.max(self.min).min(self.max);
        }
        self.editing_number = false;
        self.number_input.clear();
    }

    /// Cancel editing.
    pub fn cancel_edit(&mut self) {
        self.editing_number = false;
        self.number_input.clear();
    }

    /// Type a character into the numeric box.
    pub fn type_char(&mut self, c: char) {
        if c.is_ascii_digit() && self.number_input.len() < 5 {
            self.number_input.push(c);
        }
    }

    /// Backspace in the numeric box.
    pub fn backspace(&mut self) {
        self.number_input.pop();
    }

    /// Color for the current value position.
    pub fn value_color(&self) -> Color {
        if self.value >= self.red_threshold {
            Color::Red
        } else if self.value >= self.yellow_threshold {
            Color::Yellow
        } else {
            Color::Green
        }
    }
}

/// Render a slider row:
///   Label (16 chars)  [====green====|====yellow====|====red=====>        ] NNN [NNN]
pub fn render_slider(f: &mut Frame, state: &mut SliderState, area: Rect) {
    state.area = area;

    let label_width: u16 = 18;
    let numbox_width: u16 = 10; // "  NNN [NNN]" or "  NNN [___]"
    let bar_width = area.width.saturating_sub(label_width + numbox_width + 4); // 4 for [ ] brackets

    if bar_width < 4 || area.height == 0 {
        return; // too small to render
    }

    // Store bar area for click detection
    state.bar_area = Rect::new(area.x + label_width + 1, area.y, bar_width + 2, 1);

    // ── Label ──
    let label_style = if state.focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        theme::LABEL
    };
    let label_text = format!("{:>16}  ", state.label);

    // ── Bar segments ──
    let range = (state.max - state.min).max(1) as f64;
    let fill_ratio = (state.value - state.min) as f64 / range;
    let fill_cols = (fill_ratio * bar_width as f64).round() as u16;

    let yellow_col = ((state.yellow_threshold - state.min) as f64 / range * bar_width as f64).round() as u16;
    let red_col = ((state.red_threshold - state.min) as f64 / range * bar_width as f64).round() as u16;

    // Build the bar character by character
    let mut bar_spans: Vec<Span> = Vec::new();
    bar_spans.push(Span::styled("[", if state.focused { Style::default().fg(Color::Cyan) } else { theme::BORDER }));

    let mut bar_chars = String::new();
    let mut current_color = Color::Green;
    let mut last_color = Color::Green;

    for col in 0..bar_width {
        let zone_color = if col < yellow_col {
            Color::Green
        } else if col < red_col {
            Color::Yellow
        } else {
            Color::Red
        };

        if col < fill_cols {
            // Filled region
            if zone_color != current_color && !bar_chars.is_empty() {
                bar_spans.push(Span::styled(bar_chars.clone(), Style::default().fg(current_color)));
                bar_chars.clear();
            }
            current_color = zone_color;
            if col == fill_cols - 1 {
                bar_chars.push('>');
            } else {
                bar_chars.push('=');
            }
        } else {
            // Empty region (dimmed zone background)
            if !bar_chars.is_empty() {
                bar_spans.push(Span::styled(bar_chars.clone(), Style::default().fg(current_color)));
                bar_chars.clear();
            }
            // Dim empty segments show zone color at low intensity
            let dim_color = if col < yellow_col {
                Color::DarkGray
            } else if col < red_col {
                Color::DarkGray
            } else {
                Color::DarkGray
            };
            if dim_color != last_color && !bar_chars.is_empty() {
                bar_spans.push(Span::styled(bar_chars.clone(), Style::default().fg(last_color)));
                bar_chars.clear();
            }
            last_color = dim_color;
            current_color = dim_color;
            bar_chars.push('\u{2500}'); // light horizontal line for empty space
        }
    }
    if !bar_chars.is_empty() {
        bar_spans.push(Span::styled(bar_chars, Style::default().fg(current_color)));
    }

    bar_spans.push(Span::styled("]", if state.focused { Style::default().fg(Color::Cyan) } else { theme::BORDER }));

    // ── Numeric display + input box ──
    let value_str = format!(" {:>3} ", state.value);
    let value_style = Style::default().fg(state.value_color());

    let numbox_str = if state.editing_number {
        format!("[{:<4}]", state.number_input)
    } else {
        format!("[{:>4}]", state.value)
    };
    let numbox_style = if state.editing_number {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else if state.focused {
        Style::default().fg(Color::Cyan)
    } else {
        theme::LABEL
    };

    // ── Assemble the line ──
    let mut spans = vec![Span::styled(label_text, label_style)];
    spans.extend(bar_spans);
    spans.push(Span::styled(value_str, value_style));
    spans.push(Span::styled(numbox_str, numbox_style));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
