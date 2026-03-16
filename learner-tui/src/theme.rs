use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, BorderType, Borders};

// Composed styles (replacing ui.rs constants)
pub const BORDER: Style = Style::new().fg(Color::DarkGray);
pub const TITLE: Style = Style::new().fg(Color::Cyan);
pub const LABEL: Style = Style::new().fg(Color::DarkGray);
pub const DATA: Style = Style::new().fg(Color::White);
pub const SUCCESS: Style = Style::new().fg(Color::Green);
pub const HIGHLIGHT: Style = Style::new().fg(Color::White).bg(Color::DarkGray);
pub const MAGENTA: Style = Style::new().fg(Color::Magenta);

// Interactive widget styles (for future use)
pub const BTN_NORMAL: Style = Style::new().fg(Color::White);
pub const BTN_HOVER: Style = Style::new().fg(Color::Cyan);
pub const BTN_ACTIVE: Style = Style::new().fg(Color::Black).bg(Color::Cyan);
pub const INPUT_BORDER: Style = Style::new().fg(Color::DarkGray);
pub const INPUT_BORDER_FOCUSED: Style = Style::new().fg(Color::Cyan);
pub const INPUT_TEXT: Style = Style::new().fg(Color::White);
pub const INPUT_CURSOR: Style = Style::new().fg(Color::Black).bg(Color::Cyan);

pub fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(BORDER)
        .title(Span::styled(
            format!(" {} ", title),
            TITLE.add_modifier(Modifier::BOLD),
        ))
}

pub fn styled_block_accent(title: &str, accent: Color) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(BORDER)
        .title(Span::styled(
            format!(" {} ", title),
            Style::new().fg(accent).add_modifier(Modifier::BOLD),
        ))
}
