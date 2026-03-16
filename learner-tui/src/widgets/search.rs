use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::theme;

/// A single search result with match scoring and highlight positions.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub index: usize,                    // index in the original list
    pub score: i32,                      // match score (higher = better)
    pub label: String,                   // the full text that was matched
    pub highlights: Vec<(usize, usize)>, // byte ranges to highlight
}

/// Search bar state with fuzzy matching capabilities.
pub struct SearchState {
    pub query: String,
    pub cursor_pos: usize,
    pub active: bool,
    pub results: Vec<SearchResult>,
    pub selected: usize,
    pub list_state: ListState,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            cursor_pos: 0,
            active: false,
            results: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
        }
    }
}

impl SearchState {
    /// Activate search mode.
    pub fn activate(&mut self) {
        self.active = true;
        self.query.clear();
        self.cursor_pos = 0;
        self.results.clear();
        self.selected = 0;
        self.list_state.select(None);
    }

    /// Deactivate search mode, keeping results for potential use.
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Get the index of the currently selected search result (in the original list).
    pub fn selected_result_index(&self) -> Option<usize> {
        self.results.get(self.selected).map(|r| r.index)
    }

    /// Insert a character at cursor position.
    pub fn insert_char(&mut self, c: char) {
        let byte_pos = self
            .query
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.query.len());
        self.query.insert(byte_pos, c);
        self.cursor_pos += 1;
    }

    /// Delete character before cursor.
    pub fn delete_char_before(&mut self) {
        if self.cursor_pos > 0 {
            let byte_pos = self
                .query
                .char_indices()
                .nth(self.cursor_pos - 1)
                .map(|(i, _)| i)
                .unwrap_or(0);
            let next_byte = self
                .query
                .char_indices()
                .nth(self.cursor_pos)
                .map(|(i, _)| i)
                .unwrap_or(self.query.len());
            self.query.replace_range(byte_pos..next_byte, "");
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor left.
    pub fn move_cursor_left(&mut self) {
        self.cursor_pos = self.cursor_pos.saturating_sub(1);
    }

    /// Move cursor right.
    pub fn move_cursor_right(&mut self) {
        let max = self.query.chars().count();
        if self.cursor_pos < max {
            self.cursor_pos += 1;
        }
    }

    /// Navigate up in results.
    pub fn select_prev(&mut self) {
        if !self.results.is_empty() {
            self.selected = self.selected.saturating_sub(1);
            self.list_state.select(Some(self.selected));
        }
    }

    /// Navigate down in results.
    pub fn select_next(&mut self) {
        if !self.results.is_empty() {
            let max = self.results.len().saturating_sub(1);
            if self.selected < max {
                self.selected += 1;
            }
            self.list_state.select(Some(self.selected));
        }
    }

    /// Fuzzy match: characters must appear in order, not necessarily contiguous.
    /// Returns (score, highlight_byte_ranges) or None if no match.
    pub fn fuzzy_match(query: &str, text: &str) -> Option<(i32, Vec<(usize, usize)>)> {
        if query.is_empty() {
            return None;
        }

        let query_lower: Vec<char> = query.to_lowercase().chars().collect();
        let text_chars: Vec<char> = text.chars().collect();
        let text_lower: Vec<char> = text.to_lowercase().chars().collect();

        let mut q_idx = 0;
        let mut score = 0i32;
        let mut highlights: Vec<(usize, usize)> = Vec::new();
        let mut prev_match_char_idx: Option<usize> = None;

        // Build byte offset map for char indices
        let byte_offsets: Vec<usize> = text
            .char_indices()
            .map(|(byte_idx, _)| byte_idx)
            .collect();

        for (t_idx, &tc) in text_lower.iter().enumerate() {
            if q_idx < query_lower.len() && tc == query_lower[q_idx] {
                let byte_start = byte_offsets[t_idx];
                let byte_end = if t_idx + 1 < byte_offsets.len() {
                    byte_offsets[t_idx + 1]
                } else {
                    text.len()
                };
                highlights.push((byte_start, byte_end));

                score += 10;

                // Bonus for consecutive matches
                if let Some(prev) = prev_match_char_idx {
                    if prev + 1 == t_idx {
                        score += 5;
                    }
                }

                // Bonus for match at start of word
                if t_idx == 0
                    || text_chars
                        .get(t_idx.wrapping_sub(1))
                        .map(|&c| c == ' ' || c == '/' || c == '-' || c == '_')
                        .unwrap_or(false)
                {
                    score += 3;
                }

                // Bonus for case-exact match
                if text_chars[t_idx] == query.chars().nth(q_idx).unwrap_or(' ') {
                    score += 1;
                }

                prev_match_char_idx = Some(t_idx);
                q_idx += 1;
            }
        }

        if q_idx == query_lower.len() {
            // Bonus for shorter strings (tighter match)
            score += (100i32).saturating_sub(text_chars.len() as i32).max(0);
            Some((score, highlights))
        } else {
            None
        }
    }

    /// Update results by running fuzzy match against a list of (index, label) pairs.
    pub fn update_results(&mut self, items: &[(usize, &str)]) {
        if self.query.is_empty() {
            self.results.clear();
            self.selected = 0;
            self.list_state.select(None);
            return;
        }

        self.results = items
            .iter()
            .filter_map(|(idx, label)| {
                SearchState::fuzzy_match(&self.query, label).map(|(score, highlights)| {
                    SearchResult {
                        index: *idx,
                        score,
                        label: label.to_string(),
                        highlights,
                    }
                })
            })
            .collect();

        self.results.sort_by(|a, b| b.score.cmp(&a.score));

        // Clamp selection
        if self.results.is_empty() {
            self.selected = 0;
            self.list_state.select(None);
        } else {
            self.selected = self.selected.min(self.results.len() - 1);
            self.list_state.select(Some(self.selected));
        }
    }
}

/// Render the search bar at a given area.
/// Returns the area below the search bar that can be used for results overlay.
pub fn render_search_bar(f: &mut Frame, state: &SearchState, area: Rect) {
    if !state.active {
        return;
    }

    let match_count = state.results.len();
    let match_text = if state.query.is_empty() {
        "type to search".to_string()
    } else {
        format!("{} match{}", match_count, if match_count == 1 { "" } else { "es" })
    };

    // Calculate how much of the query to show
    let max_query_width = (area.width as usize).saturating_sub(18); // room for icon + match count
    let display_query = if state.query.chars().count() > max_query_width && max_query_width > 3 {
        format!(
            "{}",
            state
                .query
                .chars()
                .skip(state.query.chars().count() - max_query_width)
                .collect::<String>()
        )
    } else {
        state.query.clone()
    };

    // Build the search bar line with cursor indicator
    let cursor_char = if state.active { "_" } else { "" };

    let line = Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(&display_query, Style::default().fg(Color::White)),
        Span::styled(cursor_char, Style::default().fg(Color::Cyan).add_modifier(Modifier::SLOW_BLINK)),
        Span::raw(" "),
        Span::styled(
            match_text,
            Style::default().fg(if match_count > 0 {
                Color::Green
            } else if state.query.is_empty() {
                Color::DarkGray
            } else {
                Color::Red
            }),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            " Search ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));

    f.render_widget(Paragraph::new(line).block(block), area);
}

/// Render the search results overlay below the search bar.
pub fn render_search_results(
    f: &mut Frame,
    state: &mut SearchState,
    area: Rect,
    max_visible: u16,
) {
    if !state.active || state.results.is_empty() {
        return;
    }

    let result_count = state.results.len().min(max_visible as usize);
    let overlay_height = result_count as u16 + 2; // +2 for borders
    let overlay_area = Rect::new(
        area.x,
        area.y,
        area.width,
        overlay_height.min(area.height),
    );

    // Clear the overlay area
    f.render_widget(Clear, overlay_area);

    let available_width = overlay_area.width.saturating_sub(4) as usize;

    let items: Vec<ListItem> = state
        .results
        .iter()
        .take(max_visible as usize)
        .map(|result| {
            // Build spans with highlighted characters
            let spans = build_highlighted_spans(&result.label, &result.highlights, available_width);
            ListItem::new(Line::from(spans))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));

    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme::HIGHLIGHT),
        overlay_area,
        &mut state.list_state,
    );
}

/// Build a line of spans with highlighted (matched) characters shown in cyan+bold.
fn build_highlighted_spans(text: &str, highlights: &[(usize, usize)], max_width: usize) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut last_end = 0;

    // Truncate if needed
    let display_text: String = if text.chars().count() > max_width && max_width > 3 {
        format!(
            "{}...",
            text.chars().take(max_width - 3).collect::<String>()
        )
    } else {
        text.to_string()
    };

    let display_len = display_text.len();

    spans.push(Span::styled(" ", Style::default()));

    for &(start, end) in highlights {
        if start >= display_len || end > display_len {
            continue;
        }

        // Non-highlighted segment before this match
        if start > last_end {
            spans.push(Span::styled(
                display_text[last_end..start].to_string(),
                theme::DATA,
            ));
        }

        // Highlighted (matched) segment
        spans.push(Span::styled(
            display_text[start..end].to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

        last_end = end;
    }

    // Remaining non-highlighted text
    if last_end < display_len {
        spans.push(Span::styled(
            display_text[last_end..].to_string(),
            theme::DATA,
        ));
    }

    spans
}
