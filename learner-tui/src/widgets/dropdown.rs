use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use crate::theme;

pub struct DropdownState {
    pub options: Vec<String>,
    pub selected_index: usize,
    pub expanded: bool,
    pub label: String,
    pub area: Rect,
    pub focused: bool,
}

impl DropdownState {
    pub fn new(label: &str, options: Vec<String>) -> Self {
        Self {
            options,
            selected_index: 0,
            expanded: false,
            label: label.to_string(),
            area: Rect::default(),
            focused: false,
        }
    }

    pub fn selected_value(&self) -> &str {
        self.options.get(self.selected_index).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn select_next(&mut self) {
        if self.selected_index + 1 < self.options.len() {
            self.selected_index += 1;
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn hit_test(&self, col: u16, row: u16) -> bool {
        self.area.width > 0
            && col >= self.area.x && col < self.area.x + self.area.width
            && row >= self.area.y && row < self.area.y + self.area.height
    }
}

pub fn render_dropdown(f: &mut Frame, state: &mut DropdownState, area: Rect) {
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

    let indicator = if state.expanded { "\u{25b2}" } else { "\u{25bc}" };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", state.selected_value()), theme::INPUT_TEXT),
            Span::styled(indicator, theme::LABEL),
        ])),
        inner,
    );

    // Expanded overlay
    if state.expanded {
        let list_height = (state.options.len() as u16).min(8);
        let overlay_area = Rect::new(
            area.x,
            area.y + area.height,
            area.width,
            list_height + 2, // +2 for borders
        );
        f.render_widget(Clear, overlay_area);
        let items: Vec<ListItem> = state.options.iter().enumerate().map(|(i, opt)| {
            let style = if i == state.selected_index {
                theme::HIGHLIGHT
            } else {
                theme::DATA
            };
            ListItem::new(Line::from(Span::styled(format!(" {} ", opt), style)))
        }).collect();
        f.render_widget(
            List::new(items).block(
                Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(theme::INPUT_BORDER_FOCUSED)
            ),
            overlay_area,
        );
    }
}
