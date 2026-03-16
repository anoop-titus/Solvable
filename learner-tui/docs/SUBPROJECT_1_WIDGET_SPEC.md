# Solvable -- Sub-project 1: Widget Architecture Specification

**Version**: 1.0
**Date**: 2026-03-16
**Target**: Ratatui 0.29 + Crossterm 0.28
**Baseline**: 3 files (main.rs 128 LOC, app.rs 302 LOC, ui.rs 421 LOC)

---

## Table of Contents

1. [Module Structure](#1-module-structure)
2. [Design Tokens and Color System](#2-design-tokens-and-color-system)
3. [Tab Bar Widget](#3-tab-bar-widget)
4. [Welcome Screen](#4-welcome-screen)
5. [Button Widget](#5-button-widget)
6. [Text Input Widget](#6-text-input-widget)
7. [Dropdown Widget](#7-dropdown-widget)
8. [Access Portal Tab](#8-access-portal-tab)
9. [App State Additions](#9-app-state-additions)
10. [Event Routing Architecture](#10-event-routing-architecture)
11. [Crossterm Event Map](#11-crossterm-event-map)

---

## 1. Module Structure

The current flat 3-file structure cannot support 8 tabs, reusable widgets, and form
state management. The codebase moves to a module tree.

```
src/
  main.rs                  -- event loop, terminal setup (refactored)
  app.rs                   -- top-level App state, Tab enum, screen routing
  theme.rs                 -- design tokens: all Color/Style/Modifier constants
  widgets/
    mod.rs                 -- pub use re-exports
    button.rs              -- ButtonWidget + ButtonState
    text_input.rs          -- TextInputWidget + TextInputState
    dropdown.rs            -- DropdownWidget + DropdownState
    tab_bar.rs             -- TabBarWidget + click-region tracking
  screens/
    mod.rs                 -- pub use re-exports
    welcome.rs             -- first-launch welcome screen
    learnings.rs           -- existing Learnings tab (extracted from ui.rs)
    research.rs            -- existing Research tab (extracted from ui.rs)
    issues.rs              -- stub: Issues tab
    solutions.rs           -- stub: Solutions tab
    confluence.rs          -- stub: Confluence tab
    solve.rs               -- stub: Solve tab
    portal.rs              -- Access Portal credential form
    settings.rs            -- stub: Settings tab
  io/
    mod.rs                 -- pub use re-exports
    env_store.rs           -- .env read/write, credential persistence
    db.rs                  -- SQLite query layer (extracted from app.rs)
```

**Rationale**: Widgets are decoupled from screens. Each screen composes widgets.
IO is isolated so that widget state never performs filesystem or database access
directly -- the App orchestrates data flow.

---

## 2. Design Tokens and Color System

All style constants move from `ui.rs` to a dedicated `theme.rs`. The existing
codebase uses raw `Color` enum values -- this spec retains that approach (no
RGB hex) to guarantee correct rendering on both 16-color and 256-color terminals.

### 2.1 Color Palette

```rust
// theme.rs

use ratatui::style::{Color, Modifier, Style};

// -- Surface colors --
pub const BG_PRIMARY:    Color = Color::Reset;       // terminal default
pub const BG_ELEVATED:   Color = Color::DarkGray;    // cards, hover states
pub const BG_INPUT:      Color = Color::Reset;        // input fields

// -- Text colors --
pub const FG_PRIMARY:    Color = Color::White;
pub const FG_SECONDARY:  Color = Color::DarkGray;
pub const FG_MUTED:      Color = Color::Gray;
pub const FG_DISABLED:   Color = Color::DarkGray;

// -- Accent colors --
pub const ACCENT_CYAN:   Color = Color::Cyan;         // primary interactive
pub const ACCENT_MAGENTA:Color = Color::Magenta;      // secondary accent
pub const ACCENT_GREEN:  Color = Color::Green;         // success / solved
pub const ACCENT_YELLOW: Color = Color::Yellow;        // warning / pending
pub const ACCENT_RED:    Color = Color::Red;           // error / critical
pub const ACCENT_BLUE:   Color = Color::Blue;          // info / glow

// -- Composed styles (mirrors existing codebase convention) --
pub const BORDER:        Style = Style::new().fg(Color::DarkGray);
pub const TITLE:         Style = Style::new().fg(Color::Cyan);
pub const LABEL:         Style = Style::new().fg(Color::DarkGray);
pub const DATA:          Style = Style::new().fg(Color::White);
pub const SUCCESS:       Style = Style::new().fg(Color::Green);
pub const HIGHLIGHT:     Style = Style::new().fg(Color::White).bg(Color::DarkGray);

// -- Interactive element styles --
pub const BTN_NORMAL:    Style = Style::new().fg(Color::White);
pub const BTN_BORDER:    Style = Style::new().fg(Color::DarkGray);
pub const BTN_HOVER:     Style = Style::new().fg(Color::Cyan);
pub const BTN_ACTIVE:    Style = Style::new().fg(Color::Black).bg(Color::Cyan);

pub const INPUT_BORDER:         Style = Style::new().fg(Color::DarkGray);
pub const INPUT_BORDER_FOCUSED: Style = Style::new().fg(Color::Cyan);
pub const INPUT_TEXT:           Style = Style::new().fg(Color::White);
pub const INPUT_PLACEHOLDER:   Style = Style::new().fg(Color::DarkGray);
pub const INPUT_CURSOR:        Style = Style::new().fg(Color::Black).bg(Color::Cyan);

pub const TAB_ACTIVE:    Style = Style::new().fg(Color::Cyan);  // + BOLD + REVERSED at runtime
pub const TAB_INACTIVE:  Style = Style::new().fg(Color::DarkGray);
pub const TAB_SETTINGS:  Style = Style::new().fg(Color::Gray);  // gear icon neutral
```

### 2.2 Terminal Compatibility Notes

- All colors use the base-16 `Color` enum variants, never `Color::Rgb(r, g, b)`.
  This ensures compatibility with every terminal emulator that supports basic ANSI.
- `Color::Reset` defers to the user's terminal background, respecting both light and
  dark themes without detection logic.
- Active tab uses `Modifier::REVERSED` which swaps fg/bg -- this renders correctly
  on both light and dark terminals because it inverts whatever the base palette is.
- `DarkGray` for borders/labels provides sufficient contrast against both black
  (dark theme) and white (light theme) terminal backgrounds at WCAG AA level
  for decorative/secondary text.

---

## 3. Tab Bar Widget

### 3.1 Overview

Replaces the current inline `render_tab_bar` function (ui.rs:96-110) with a
dedicated stateful widget that tracks 8 clickable tab regions, supports keyboard
and mouse interaction, and anchors the Settings tab to the right edge.

### 3.2 Ratatui Primitives Used

| Primitive | Role |
|-----------|------|
| `Paragraph` | Renders the composed `Line` of tab `Span` segments |
| `Line` | Single-line container holding all tab label spans |
| `Span` | Individual styled text segment per tab label |
| `Rect` | Stored per-tab for click hit-testing |
| `Layout` | Horizontal split: left tab group (Fill) + right-anchored Settings (Length) |

**Why Paragraph, not Tabs widget**: Ratatui's built-in `Tabs` widget does not
expose per-tab `Rect` regions for click detection. We need precise hit-test
rectangles, so we render manually with `Span`s and compute each label's `Rect`
from its character offset and width.

### 3.3 Tab Enum (revised)

```rust
// app.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Tab {
    Learnings  = 0,
    Research   = 1,
    Issues     = 2,
    Solutions  = 3,
    Confluence = 4,
    Solve      = 5,
    Portal     = 6,
    Settings   = 7,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::Learnings, Tab::Research, Tab::Issues, Tab::Solutions,
        Tab::Confluence, Tab::Solve, Tab::Portal, Tab::Settings,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Learnings  => "Learnings",
            Tab::Research   => "Research",
            Tab::Issues     => "Issues",
            Tab::Solutions  => "Solutions",
            Tab::Confluence => "Confluence",
            Tab::Solve      => "Solve",
            Tab::Portal     => "Portal",
            Tab::Settings   => "\u{2699} Settings",  // gear unicode U+2699
        }
    }

    /// Color accent per tab for the active state highlight.
    pub fn accent(&self) -> Color {
        match self {
            Tab::Learnings  => Color::Cyan,
            Tab::Research   => Color::Magenta,
            Tab::Issues     => Color::Yellow,
            Tab::Solutions  => Color::Green,
            Tab::Confluence => Color::Blue,
            Tab::Solve      => Color::Cyan,
            Tab::Portal     => Color::Green,
            Tab::Settings   => Color::Gray,
        }
    }

    pub fn next(&self) -> Tab {
        let idx = (*self as u8 + 1) % 8;
        Tab::ALL[idx as usize]
    }

    pub fn prev(&self) -> Tab {
        let idx = (*self as u8 + 7) % 8;  // wraps backward
        Tab::ALL[idx as usize]
    }

    pub fn from_number(n: u8) -> Option<Tab> {
        if n >= 1 && n <= 8 { Some(Tab::ALL[(n - 1) as usize]) } else { None }
    }
}
```

### 3.4 State Struct

```rust
// widgets/tab_bar.rs

pub struct TabBarState {
    /// Rect of each tab label, computed during render.
    /// Indexed by Tab as u8. Reset each frame.
    pub tab_rects: [Rect; 8],
}

impl Default for TabBarState {
    fn default() -> Self {
        Self { tab_rects: [Rect::default(); 8] }
    }
}
```

### 3.5 Rendering Algorithm

```
fn render_tab_bar(frame, app, area, tab_bar_state):
    // Split area: [left_tabs: Fill(1), right_settings: Length(settings_label_width + 4)]
    let settings_width = " \u{2699} Settings ".len() as u16 + 2  // padding
    let chunks = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(settings_width),
    ]).split(area)

    // -- Render left tabs (indices 0..7, excluding Settings) --
    let mut spans: Vec<Span> = Vec::new()
    let mut x_cursor: u16 = chunks[0].x + 1  // 1-char left padding

    for tab in Tab::ALL[0..7]:
        let label = format!(" {} ", tab.label())
        let label_width = label.len() as u16

        // Record Rect for this tab
        tab_bar_state.tab_rects[tab as u8] = Rect {
            x: x_cursor,
            y: area.y,
            width: label_width,
            height: 1,
        }

        // Style: active gets accent color + BOLD + REVERSED; inactive gets LABEL
        let style = if app.current_tab == tab {
            Style::default()
                .fg(tab.accent())
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            LABEL  // DarkGray
        }

        spans.push(Span::styled(label, style))

        // Separator (except after last left tab)
        if tab != Tab::Portal:
            spans.push(Span::styled(" | ", LABEL))
            x_cursor += label_width + 3  // label + " | "
        else:
            x_cursor += label_width

    frame.render_widget(Paragraph::new(Line::from(spans)), chunks[0])

    // -- Render Settings tab (right-anchored) --
    let settings_label = format!(" {} ", Tab::Settings.label())
    let settings_style = if app.current_tab == Tab::Settings {
        Style::default()
            .fg(Tab::Settings.accent())
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        TAB_SETTINGS
    }

    tab_bar_state.tab_rects[Tab::Settings as u8] = Rect {
        x: chunks[1].x,
        y: area.y,
        width: chunks[1].width,
        height: 1,
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(settings_label, settings_style)))
            .alignment(Alignment::Right),
        chunks[1],
    )
```

### 3.6 Click Hit-Testing

```rust
impl TabBarState {
    /// Returns which tab was clicked, if any.
    pub fn tab_at(&self, col: u16, row: u16) -> Option<Tab> {
        for (i, rect) in self.tab_rects.iter().enumerate() {
            if col >= rect.x
                && col < rect.x + rect.width
                && row >= rect.y
                && row < rect.y + rect.height
            {
                return Some(Tab::ALL[i]);
            }
        }
        None
    }
}
```

### 3.7 Event Handling

| Event | Source | Action |
|-------|--------|--------|
| `MouseEventKind::Down(MouseButton::Left)` | Crossterm | `tab_bar_state.tab_at(col, row)` -> set `app.current_tab` |
| `KeyCode::Tab` | Crossterm | `app.current_tab = app.current_tab.next()` |
| `KeyCode::BackTab` (Shift+Tab) | Crossterm | `app.current_tab = app.current_tab.prev()` |
| `KeyCode::Char('1')` .. `KeyCode::Char('8')` | Crossterm | `Tab::from_number(n)` -> set `app.current_tab` |

**Important**: Number-key tab switching is only active when no text input has
focus. The event router must check `app.has_focused_input()` before dispatching
number keys to tab switching.

### 3.8 Minimum Terminal Width

At minimum 80 columns, the 7 left labels plus separators consume:

```
 Learnings  |  Research  |  Issues  |  Solutions  |  Confluence  |  Solve  |  Portal
 11 + 3 + 10 + 3 + 9 + 3 + 12 + 3 + 13 + 3 + 8 + 3 + 9 = 80 chars
```

Plus the right-anchored `" \u{2699} Settings "` = 12 chars. Total: 92 chars minimum.

**Narrow terminal fallback** (width < 92): Truncate labels to first 3 characters
(e.g., "Lea", "Res", "Iss", ...) which fits in ~50 columns. Implementation:
check `area.width` at render time and switch to short labels.

```rust
impl Tab {
    pub fn short_label(&self) -> &'static str {
        match self {
            Tab::Learnings  => "Lrn",
            Tab::Research   => "Rsc",
            Tab::Issues     => "Iss",
            Tab::Solutions  => "Sol",
            Tab::Confluence => "Cnf",
            Tab::Solve      => "Slv",
            Tab::Portal     => "Ptl",
            Tab::Settings   => "\u{2699}",
        }
    }
}
```

---

## 4. Welcome Screen

### 4.1 Overview

Displays once on first launch when no `.env` file exists at the expected path.
Replaces the entire frame content (no tab bar, no footer). Contains a centered
title, tagline, description, and a clickable "Get Started" button that navigates
to the Portal tab.

### 4.2 Ratatui Primitives Used

| Primitive | Role |
|-----------|------|
| `Paragraph` | Title text, tagline, description paragraph |
| `Block` | Outer border with rounded corners |
| `Layout` | Vertical centering via `[Fill, Length, Fill]` pattern |
| `Constraint::Fill` | Top/bottom spacers for vertical centering |
| `Constraint::Length` | Fixed heights for title, tagline, spacer, description, button |
| `ButtonWidget` (custom) | "Get Started" interactive button |

### 4.3 State

```rust
// screens/welcome.rs

pub struct WelcomeState {
    pub button: ButtonState,  // for "Get Started"
}
```

### 4.4 Rendering Algorithm

```
fn render_welcome(frame, welcome_state, area):
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(BORDER)
    let inner = outer.inner(area)
    frame.render_widget(outer, area)

    // Vertical centering: content block is ~14 lines tall
    let content_height = 14u16
    let v_pad = inner.height.saturating_sub(content_height) / 2

    let layout = Layout::vertical([
        Constraint::Length(v_pad),       // top spacer
        Constraint::Length(3),           // title
        Constraint::Length(1),           // blank
        Constraint::Length(1),           // tagline
        Constraint::Length(1),           // blank
        Constraint::Length(4),           // description
        Constraint::Length(1),           // blank
        Constraint::Length(3),           // button
        Constraint::Fill(1),            // bottom spacer
    ]).split(inner)

    // Title -- large ASCII art or styled text
    // Using simple styled text (ASCII art optional for later enhancement)
    let title = Paragraph::new(Line::from(vec![
        Span::styled("S", Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("olvable", Style::new().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]))
    .alignment(Alignment::Center)
    frame.render_widget(title, layout[2])

    // Tagline
    let tagline = Paragraph::new(
        Span::styled("Access to all your Solutions", Style::new().fg(Color::DarkGray))
    ).alignment(Alignment::Center)
    frame.render_widget(tagline, layout[4])

    // Description
    let desc = Paragraph::new(vec![
        Line::from("A terminal dashboard for your learnings, research,"),
        Line::from("issue tracking, and solution management."),
        Line::from(""),
        Line::from("Set up your credentials to get started."),
    ])
    .style(Style::new().fg(Color::Gray))
    .alignment(Alignment::Center)
    frame.render_widget(desc, layout[6])

    // Get Started button -- centered
    let btn_width = 18u16  // "[ Get Started ]" = 16 + 2 border
    let btn_x = layout[8].x + (layout[8].width.saturating_sub(btn_width)) / 2
    let btn_area = Rect { x: btn_x, y: layout[8].y, width: btn_width, height: 3 }

    render_button(frame, &mut welcome_state.button, "Get Started", btn_area)
```

### 4.5 Title Rendering -- ASCII Art Option

For a more impactful welcome, an optional ASCII art title can be used. This
renders in 5 lines instead of 1 and requires adjusting the layout constraints
accordingly.

```
  ____        _             _     _
 / ___|  ___ | |_   ____ _ | |__ | | ___
 \___ \ / _ \| \ \ / / _` || '_ \| |/ _ \
  ___) | (_) | |\ V / (_| || |_) | |  __/
 |____/ \___/|_| \_/ \__,_||_.__/|_|\___|
```

Rendered as a `Paragraph` with `Style::new().fg(Color::Cyan)` and centered
alignment. Each line is a `Line::raw(...)`.

### 4.6 Event Handling

| Event | Action |
|-------|--------|
| `MouseEventKind::Down(Left)` in button Rect | Set `app.current_tab = Tab::Portal`, set `app.screen = Screen::Main` |
| `KeyCode::Enter` | Same as click (button is auto-focused on welcome) |
| `KeyCode::Char('q')` | Quit (always available) |

### 4.7 Screen State Machine

```rust
// app.rs

pub enum Screen {
    Welcome,   // no .env file detected
    Main,      // tab bar + content
}
```

On startup: if `.env` path does not exist, `app.screen = Screen::Welcome`.
After Portal saves at least one credential, screen transitions to `Screen::Main`.

---

## 5. Button Widget

### 5.1 Overview

A reusable interactive button that renders as bordered text, supports three
visual states (normal, hover, active), and reports click events to its parent
via state inspection.

### 5.2 Ratatui Primitives Used

| Primitive | Role |
|-----------|------|
| `Block` | Border around the label (Borders::ALL, BorderType::Rounded) |
| `Paragraph` | Label text centered inside the block |
| `Rect` | Stored in state for click detection |

### 5.3 State Struct

```rust
// widgets/button.rs

pub struct ButtonState {
    /// The Rect where this button was last rendered.
    /// Updated each frame during render.
    pub area: Rect,

    /// Whether the mouse cursor is currently over this button.
    pub hovered: bool,

    /// Tick count when button was last clicked.
    /// Used to show active state for a brief visual flash.
    /// Set to current tick on MouseDown; active style shown while
    /// (current_tick - click_tick) < ACTIVE_FLASH_TICKS.
    pub click_tick: Option<u64>,

    /// Set to true on the frame the button is clicked.
    /// The parent reads and clears this flag each frame.
    pub clicked: bool,
}

const ACTIVE_FLASH_TICKS: u64 = 1;  // ~200ms at 200ms tick rate
```

### 5.4 Rendering

```
fn render_button(frame, state: &mut ButtonState, label: &str, area: Rect):
    state.area = area

    let is_active = state.click_tick
        .map(|t| app_tick.saturating_sub(t) < ACTIVE_FLASH_TICKS)
        .unwrap_or(false)

    let (border_style, text_style) = if is_active {
        (BTN_ACTIVE, BTN_ACTIVE)                       // inverted: black on cyan
    } else if state.hovered {
        (BTN_HOVER, Style::new().fg(Color::Cyan))      // cyan border + cyan text
    } else {
        (BTN_BORDER, BTN_NORMAL)                       // dark border + white text
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)

    let paragraph = Paragraph::new(Span::styled(label, text_style))
        .alignment(Alignment::Center)
        .block(block)

    frame.render_widget(paragraph, area)
```

**Dimensions**: A button occupies 3 rows (1 top border + 1 content + 1 bottom border)
and `label.len() + 4` columns (1 left border + 1 padding + label + 1 padding + 1 right border).

For a single-line compact variant (no block border), render as:

```
[ Label ]
```

Using `Span::styled("[ ", border_style)` + `Span::styled(label, text_style)` + `Span::styled(" ]", border_style)`, which occupies 1 row and `label.len() + 4` columns.

### 5.5 Event Handling

Buttons do not consume events directly. The parent event handler performs
hit-testing against `state.area`:

```rust
// In the main event loop or screen-level handler:

fn handle_button_mouse(state: &mut ButtonState, kind: MouseEventKind, col: u16, row: u16, tick: u64) {
    let inside = col >= state.area.x
        && col < state.area.x + state.area.width
        && row >= state.area.y
        && row < state.area.y + state.area.height;

    match kind {
        MouseEventKind::Moved => {
            state.hovered = inside;
        }
        MouseEventKind::Down(MouseButton::Left) if inside => {
            state.clicked = true;
            state.click_tick = Some(tick);
        }
        _ => {}
    }
}
```

The parent checks `state.clicked`, acts on it, then sets `state.clicked = false`.

### 5.6 Keyboard Interaction

Buttons also respond to `KeyCode::Enter` when they hold focus in the form's
focus chain. Focus is tracked by the parent screen, not by the button itself.

---

## 6. Text Input Widget

### 6.1 Overview

A single-line text input field with label, cursor, editing operations, and
an optional password masking mode. This is the most complex new widget.

### 6.2 Ratatui Primitives Used

| Primitive | Role |
|-----------|------|
| `Block` | Border around the input area (color changes on focus) |
| `Paragraph` | Label text rendered above or to the left of the input |
| `Span` | Composing the visible text, cursor character, masked text |
| `Line` | The single line of input content + cursor |
| `Rect` | Stored in state for click-to-focus detection |

### 6.3 State Struct

```rust
// widgets/text_input.rs

pub struct TextInputState {
    /// The current text value.
    pub value: String,

    /// Cursor position as a character index (not byte index).
    pub cursor_pos: usize,

    /// Horizontal scroll offset (character index of the first visible char).
    pub scroll_offset: usize,

    /// Whether this input currently has focus.
    pub focused: bool,

    /// Whether to mask input (password mode).
    pub masked: bool,

    /// The Rect where this input was last rendered (for click detection).
    pub area: Rect,

    /// Optional placeholder text shown when value is empty and unfocused.
    pub placeholder: String,

    /// Tick counter for cursor blink animation.
    /// Cursor visible when (tick / BLINK_RATE) % 2 == 0.
    pub tick: u64,
}

const BLINK_RATE: u64 = 3;  // toggle every 3 ticks = ~600ms at 200ms tick
```

### 6.4 Password Masking Logic

```rust
impl TextInputState {
    /// Returns the display string for the current value.
    /// In masked mode: shows first 4 and last 4 characters in clear text,
    /// everything in between as bullets.
    /// If value.len() <= 8, all characters are shown as bullets (no reveal).
    pub fn display_value(&self) -> String {
        if !self.masked {
            return self.value.clone();
        }

        let chars: Vec<char> = self.value.chars().collect();
        let len = chars.len();

        if len == 0 {
            return String::new();
        }

        if len <= 8 {
            // Too short to partially reveal -- all bullets
            return "\u{2022}".repeat(len);  // bullet: U+2022
        }

        let mut display = String::with_capacity(len);
        for (i, ch) in chars.iter().enumerate() {
            if i < 4 || i >= len - 4 {
                display.push(*ch);
            } else {
                display.push('\u{2022}');
            }
        }
        display
    }
}
```

### 6.5 Rendering

```
fn render_text_input(frame, state: &mut TextInputState, label: &str, area: Rect):
    // Layout: label on top (1 line), input below (3 lines: border+content+border)
    // If area.height >= 4, use vertical label. Otherwise, inline.

    let (label_area, input_area) = if area.height >= 4 {
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
        ]).split(area)
        (chunks[0], chunks[1])
    } else {
        // Inline: label takes first 15 chars, rest is input
        let chunks = Layout::horizontal([
            Constraint::Length(label.len() as u16 + 2),
            Constraint::Fill(1),
        ]).split(area)
        (chunks[0], chunks[1])
    }

    // Render label
    frame.render_widget(
        Paragraph::new(Span::styled(label, LABEL)),
        label_area,
    )

    // Store the input area for click detection
    state.area = input_area

    // Border style: focused = cyan, unfocused = dark gray
    let border_style = if state.focused {
        INPUT_BORDER_FOCUSED
    } else {
        INPUT_BORDER
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)

    let inner = block.inner(input_area)
    frame.render_widget(block, input_area)

    // Compute visible text
    let display_text = state.display_value()
    let display_chars: Vec<char> = display_text.chars().collect()
    let visible_width = inner.width as usize

    // Adjust scroll_offset to keep cursor visible
    if state.cursor_pos < state.scroll_offset {
        state.scroll_offset = state.cursor_pos
    }
    if state.cursor_pos >= state.scroll_offset + visible_width {
        state.scroll_offset = state.cursor_pos.saturating_sub(visible_width - 1)
    }

    // Build the visible line
    let visible_end = (state.scroll_offset + visible_width).min(display_chars.len())
    let visible_slice: String = display_chars[state.scroll_offset..visible_end].iter().collect()

    if state.value.is_empty() && !state.focused {
        // Show placeholder
        frame.render_widget(
            Paragraph::new(Span::styled(&state.placeholder, INPUT_PLACEHOLDER)),
            inner,
        )
    } else if state.focused {
        // Render text with cursor
        let cursor_visible = (state.tick / BLINK_RATE) % 2 == 0
        let cursor_offset = state.cursor_pos - state.scroll_offset

        let mut spans = Vec::new()

        // Text before cursor
        if cursor_offset > 0 {
            let before: String = display_chars[state.scroll_offset..state.scroll_offset + cursor_offset].iter().collect()
            spans.push(Span::styled(before, INPUT_TEXT))
        }

        // Cursor character
        if cursor_visible {
            let cursor_char = if state.cursor_pos < display_chars.len() {
                display_chars[state.cursor_pos].to_string()
            } else {
                " ".to_string()
            }
            spans.push(Span::styled(cursor_char, INPUT_CURSOR))  // black on cyan
        } else if state.cursor_pos < display_chars.len() {
            spans.push(Span::styled(
                display_chars[state.cursor_pos].to_string(),
                INPUT_TEXT,
            ))
        }

        // Text after cursor
        let after_start = state.cursor_pos + 1
        if after_start < display_chars.len() {
            let after_end = visible_end.min(display_chars.len())
            if after_start < after_end {
                let after: String = display_chars[after_start..after_end].iter().collect()
                spans.push(Span::styled(after, INPUT_TEXT))
            }
        }

        frame.render_widget(Paragraph::new(Line::from(spans)), inner)
    } else {
        // Unfocused, has value
        frame.render_widget(
            Paragraph::new(Span::styled(visible_slice, INPUT_TEXT)),
            inner,
        )
    }
```

### 6.6 Editing Operations

All operations are methods on `TextInputState`:

```rust
impl TextInputState {
    pub fn insert_char(&mut self, ch: char) {
        let byte_idx = self.value.char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len());
        self.value.insert(byte_idx, ch);
        self.cursor_pos += 1;
    }

    pub fn delete_char_before(&mut self) {
        // Backspace
        if self.cursor_pos == 0 { return; }
        let byte_idx = self.value.char_indices()
            .nth(self.cursor_pos - 1)
            .map(|(i, _)| i)
            .unwrap();
        self.value.remove(byte_idx);
        self.cursor_pos -= 1;
    }

    pub fn delete_char_at(&mut self) {
        // Delete key
        let len = self.value.chars().count();
        if self.cursor_pos >= len { return; }
        let byte_idx = self.value.char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap();
        self.value.remove(byte_idx);
    }

    pub fn move_cursor_left(&mut self)  { self.cursor_pos = self.cursor_pos.saturating_sub(1); }
    pub fn move_cursor_right(&mut self) {
        let len = self.value.chars().count();
        if self.cursor_pos < len { self.cursor_pos += 1; }
    }
    pub fn move_cursor_home(&mut self)  { self.cursor_pos = 0; }
    pub fn move_cursor_end(&mut self)   { self.cursor_pos = self.value.chars().count(); }

    pub fn paste(&mut self, text: &str) {
        for ch in text.chars() {
            self.insert_char(ch);
        }
    }
}
```

### 6.7 Event Handling

| Event | Condition | Action |
|-------|-----------|--------|
| `MouseEventKind::Down(Left)` in area | Always | Set `focused = true` on this input, `focused = false` on all others (parent manages) |
| `KeyCode::Char(c)` | `focused == true` | `insert_char(c)` |
| `KeyCode::Backspace` | `focused == true` | `delete_char_before()` |
| `KeyCode::Delete` | `focused == true` | `delete_char_at()` |
| `KeyCode::Left` | `focused == true` | `move_cursor_left()` |
| `KeyCode::Right` | `focused == true` | `move_cursor_right()` |
| `KeyCode::Home` | `focused == true` | `move_cursor_home()` |
| `KeyCode::End` | `focused == true` | `move_cursor_end()` |
| `Event::Paste(text)` | `focused == true` | `paste(&text)` (Crossterm bracketed paste) |
| `KeyCode::Tab` | `focused == true` | Move focus to next widget in form (parent handles) |
| `KeyCode::Enter` | `focused == true` | Move focus to next widget (or trigger section save if last field) |

**Important**: When a text input has focus, `KeyCode::Char('1')` through
`KeyCode::Char('8')` must be routed to the input, NOT to tab switching. The
event router checks `app.has_focused_input()` before dispatching to tab keys.

---

## 7. Dropdown Widget

### 7.1 Overview

A select-one dropdown that shows the current selection, expands on click to
display all options in an overlay list, and collapses on selection or outside click.

### 7.2 Ratatui Primitives Used

| Primitive | Role |
|-----------|------|
| `Block` | Border around the collapsed dropdown and the expanded list |
| `Paragraph` | Renders the currently selected value + arrow indicator |
| `List` + `ListItem` | The expanded options list |
| `ListState` | Tracks highlighted item in expanded list for keyboard nav |
| `Clear` | Clears the area under the expanded overlay before drawing |
| `Rect` | Stored for click detection (both collapsed and expanded) |

### 7.3 State Struct

```rust
// widgets/dropdown.rs

pub struct DropdownState {
    /// All available options.
    pub options: Vec<String>,

    /// Index of the currently selected option.
    pub selected: usize,

    /// Whether the dropdown is currently expanded (open).
    pub expanded: bool,

    /// ListState for keyboard navigation when expanded.
    pub list_state: ListState,

    /// Rect of the collapsed dropdown (for click-to-open).
    pub area: Rect,

    /// Rect of the expanded option list (for click detection).
    pub expanded_area: Rect,

    /// Whether this dropdown has focus.
    pub focused: bool,
}
```

### 7.4 Rendering

```
fn render_dropdown(frame, state: &mut DropdownState, label: &str, area: Rect):
    // area is a single-line rect (height=3 with border, or height=1 without)
    state.area = area

    let border_style = if state.focused || state.expanded {
        INPUT_BORDER_FOCUSED
    } else {
        INPUT_BORDER
    }

    // -- Collapsed view --
    let current_label = state.options.get(state.selected)
        .map(|s| s.as_str())
        .unwrap_or("(none)")
    let arrow = if state.expanded { "\u{25B2}" } else { "\u{25BC}" }  // up/down triangle

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)

    let inner = block.inner(area)
    frame.render_widget(block, area)

    let content = Line::from(vec![
        Span::styled(format!(" {} ", current_label), INPUT_TEXT),
        Span::styled(arrow, Style::new().fg(Color::DarkGray)),
    ])
    frame.render_widget(Paragraph::new(content), inner)

    // -- Expanded overlay --
    if state.expanded:
        let list_height = (state.options.len() as u16 + 2).min(10)  // max 8 visible + border
        let expanded_rect = Rect {
            x: area.x,
            y: area.y + area.height,  // directly below the collapsed area
            width: area.width,
            height: list_height,
        }
        state.expanded_area = expanded_rect

        // Clear area under overlay to prevent bleed-through
        frame.render_widget(Clear, expanded_rect)

        let items: Vec<ListItem> = state.options.iter().enumerate().map(|(i, opt)| {
            let style = if i == state.selected {
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                DATA
            }
            ListItem::new(Span::styled(format!(" {} ", opt), style))
        }).collect()

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(INPUT_BORDER_FOCUSED))
            .highlight_style(HIGHLIGHT)

        frame.render_stateful_widget(list, expanded_rect, &mut state.list_state)
```

### 7.5 Event Handling

| Event | Condition | Action |
|-------|-----------|--------|
| `MouseDown(Left)` in `area` | `!expanded` | Set `expanded = true`, `list_state.select(Some(selected))` |
| `MouseDown(Left)` in `expanded_area` | `expanded` | Compute clicked index from `(row - expanded_area.y - 1)`, set `selected`, set `expanded = false` |
| `MouseDown(Left)` outside both areas | `expanded` | Set `expanded = false` (dismiss) |
| `KeyCode::Enter` or `KeyCode::Char(' ')` | `focused && !expanded` | Set `expanded = true` |
| `KeyCode::Enter` | `expanded` | Set `selected = list_state.selected()`, `expanded = false` |
| `KeyCode::Esc` | `expanded` | Set `expanded = false` |
| `KeyCode::Up` | `expanded` | Move list selection up |
| `KeyCode::Down` | `expanded` | Move list selection down |
| `KeyCode::Down` | `focused && !expanded` | Open dropdown |

### 7.6 Overlay Z-Order

The expanded list renders **after** all other widgets on the frame, overlapping
content beneath it. This requires the dropdown render call to happen last in
the screen's render function, or to be deferred to a second pass.

**Implementation strategy**: The portal screen collects dropdown state during
its main render pass, then in a final step calls `render_dropdown_overlay()`
for any expanded dropdown. This guarantees the overlay paints on top.

---

## 8. Access Portal Tab

### 8.1 Overview

A scrollable credential entry form organized into collapsible sections. Each
section groups related configuration fields (e.g., all AI Model settings, all
Dropbox settings). The form reads from and writes to a `.env` file.

### 8.2 Sections

| Section | Fields | Types |
|---------|--------|-------|
| **AI Model** | `MODEL_PROVIDER` (dropdown: OpenAI, Anthropic, Groq, Local), `MODEL_API_KEY` (password), `MODEL_NAME` (text) | Dropdown + 2 inputs |
| **Dropbox** | `DROPBOX_TOKEN` (password), `DROPBOX_FOLDER` (text) | 2 inputs |
| **Email / IMAP** | `IMAP_HOST` (text), `IMAP_PORT` (text), `IMAP_USER` (text), `IMAP_PASSWORD` (password) | 4 inputs |
| **Airtable** | `AIRTABLE_API_KEY` (password), `AIRTABLE_BASE_ID` (text) | 2 inputs |
| **Google Drive** | `GDRIVE_CREDENTIALS_PATH` (text), `GDRIVE_FOLDER_ID` (text) | 2 inputs |

Each section also has a **[Save]** button and a **[?]** help button.

### 8.3 Ratatui Primitives Used

| Primitive | Role |
|-----------|------|
| `Block` | Section headers with collapsible borders |
| `Paragraph` | Section titles, help text panel |
| `Layout` | Vertical stacking of sections, horizontal split for form + help |
| `Scrollbar` + `ScrollbarState` | Vertical scroll when form exceeds viewport |
| `TextInputWidget` (custom) | Each text/password field |
| `DropdownWidget` (custom) | Model provider selection |
| `ButtonWidget` (custom) | Save and help buttons |

### 8.4 State Struct

```rust
// screens/portal.rs

pub struct PortalState {
    /// Vertical scroll offset in pixels (character rows).
    pub scroll_offset: u16,

    /// Total content height (computed during render).
    pub content_height: u16,

    /// Which section's help panel is expanded (None = all closed).
    pub help_expanded: Option<PortalSection>,

    /// Focus chain: ordered list of focusable widget IDs.
    /// Current focus index into this list.
    pub focus_index: usize,

    /// All focusable elements in order.
    pub focus_chain: Vec<FocusTarget>,

    // -- Per-section state --
    pub ai_model: AiModelSectionState,
    pub dropbox: DropboxSectionState,
    pub email: EmailSectionState,
    pub airtable: AirtableSectionState,
    pub gdrive: GDriveSectionState,

    /// Status message shown after save (e.g., "Saved!" or error text).
    pub status_message: Option<(String, StatusKind)>,
    pub status_tick: u64,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PortalSection {
    AiModel,
    Dropbox,
    Email,
    Airtable,
    GDrive,
}

pub enum StatusKind { Success, Error }

pub struct AiModelSectionState {
    pub provider: DropdownState,    // MODEL_PROVIDER
    pub api_key: TextInputState,    // MODEL_API_KEY (masked)
    pub model_name: TextInputState, // MODEL_NAME
    pub save_btn: ButtonState,
    pub help_btn: ButtonState,
}

pub struct DropboxSectionState {
    pub token: TextInputState,      // DROPBOX_TOKEN (masked)
    pub folder: TextInputState,     // DROPBOX_FOLDER
    pub save_btn: ButtonState,
    pub help_btn: ButtonState,
}

pub struct EmailSectionState {
    pub host: TextInputState,       // IMAP_HOST
    pub port: TextInputState,       // IMAP_PORT
    pub user: TextInputState,       // IMAP_USER
    pub password: TextInputState,   // IMAP_PASSWORD (masked)
    pub save_btn: ButtonState,
    pub help_btn: ButtonState,
}

pub struct AirtableSectionState {
    pub api_key: TextInputState,    // AIRTABLE_API_KEY (masked)
    pub base_id: TextInputState,    // AIRTABLE_BASE_ID
    pub save_btn: ButtonState,
    pub help_btn: ButtonState,
}

pub struct GDriveSectionState {
    pub creds_path: TextInputState, // GDRIVE_CREDENTIALS_PATH
    pub folder_id: TextInputState,  // GDRIVE_FOLDER_ID
    pub save_btn: ButtonState,
    pub help_btn: ButtonState,
}

/// Identifies a specific focusable widget for the focus chain.
#[derive(Clone, Copy, PartialEq)]
pub enum FocusTarget {
    AiProvider,           // dropdown
    AiApiKey,             // text input
    AiModelName,          // text input
    AiSave,               // button
    DropboxToken,
    DropboxFolder,
    DropboxSave,
    EmailHost,
    EmailPort,
    EmailUser,
    EmailPassword,
    EmailSave,
    AirtableApiKey,
    AirtableBaseId,
    AirtableSave,
    GDriveCredsPath,
    GDriveFolderId,
    GDriveSave,
}
```

### 8.5 Focus Management

The focus chain is a flat ordered list of all focusable widgets:

```
AiProvider -> AiApiKey -> AiModelName -> AiSave ->
DropboxToken -> DropboxFolder -> DropboxSave ->
EmailHost -> EmailPort -> EmailUser -> EmailPassword -> EmailSave ->
AirtableApiKey -> AirtableBaseId -> AirtableSave ->
GDriveCredsPath -> GDriveFolderId -> GDriveSave
```

Total: 18 focusable elements.

**Tab** advances `focus_index` by 1 (wrapping). **Shift+Tab** moves back by 1.
**Mouse click** on any focusable widget sets `focus_index` to that widget's
position in the chain.

When focus changes, the previous widget's `focused` flag is set to `false` and
the new widget's `focused` flag is set to `true`. The portal screen's
`apply_focus()` method handles this synchronization:

```rust
impl PortalState {
    pub fn apply_focus(&mut self) {
        // Clear all focus flags
        self.ai_model.provider.focused = false;
        self.ai_model.api_key.focused = false;
        self.ai_model.model_name.focused = false;
        // ... all other fields ...

        // Set focus on current target
        match self.focus_chain[self.focus_index] {
            FocusTarget::AiProvider   => self.ai_model.provider.focused = true,
            FocusTarget::AiApiKey     => self.ai_model.api_key.focused = true,
            FocusTarget::AiModelName  => self.ai_model.model_name.focused = true,
            FocusTarget::AiSave       => { /* button focused state tracked by parent */ }
            // ... etc
        }
    }

    pub fn focus_next(&mut self) {
        self.focus_index = (self.focus_index + 1) % self.focus_chain.len();
        self.apply_focus();
    }

    pub fn focus_prev(&mut self) {
        self.focus_index = (self.focus_index + self.focus_chain.len() - 1) % self.focus_chain.len();
        self.apply_focus();
    }

    pub fn has_focused_input(&self) -> bool {
        matches!(
            self.focus_chain.get(self.focus_index),
            Some(FocusTarget::AiApiKey)
            | Some(FocusTarget::AiModelName)
            | Some(FocusTarget::DropboxToken)
            | Some(FocusTarget::DropboxFolder)
            | Some(FocusTarget::EmailHost)
            | Some(FocusTarget::EmailPort)
            | Some(FocusTarget::EmailUser)
            | Some(FocusTarget::EmailPassword)
            | Some(FocusTarget::AirtableApiKey)
            | Some(FocusTarget::AirtableBaseId)
            | Some(FocusTarget::GDriveCredsPath)
            | Some(FocusTarget::GDriveFolderId)
        )
    }
}
```

### 8.6 Layout Structure

```
fn render_portal(frame, app, portal_state, area):
    // Main split: form area (70%) + help panel (30%) when help is expanded
    // or form area (100%) when no help is expanded

    let (form_area, help_area) = if portal_state.help_expanded.is_some() {
        let chunks = Layout::horizontal([
            Constraint::Percentage(65),
            Constraint::Percentage(35),
        ]).split(area)
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    }

    // -- Scrollable form content --
    // Total content height calculation:
    //   Each section: 1 (header) + field_count * 4 (label+input per field) + 1 (button row) + 1 (spacer)
    //   AI Model:    1 + 3*4 + 1 + 1 = 15
    //   Dropbox:     1 + 2*4 + 1 + 1 = 11
    //   Email:       1 + 4*4 + 1 + 1 = 19
    //   Airtable:    1 + 2*4 + 1 + 1 = 11
    //   GDrive:      1 + 2*4 + 1 + 1 = 11
    //   Total: ~67 lines

    portal_state.content_height = 67  // updated dynamically

    // If content exceeds viewport, render a virtual viewport
    let viewport_height = form_area.height
    let max_scroll = portal_state.content_height.saturating_sub(viewport_height)
    portal_state.scroll_offset = portal_state.scroll_offset.min(max_scroll)

    // Render each section at its computed Y offset relative to scroll
    let mut y_cursor: u16 = 0

    render_section_header(frame, "AI Model", y_cursor, portal_state, form_area)
    y_cursor += 1
    // Render fields at y_cursor, advancing y_cursor by field height each time
    render_dropdown_field(frame, &mut portal_state.ai_model.provider, "Provider", ...)
    y_cursor += 4
    render_text_field(frame, &mut portal_state.ai_model.api_key, "API Key", ...)
    y_cursor += 4
    render_text_field(frame, &mut portal_state.ai_model.model_name, "Model Name", ...)
    y_cursor += 4
    render_button_row(frame, &portal_state.ai_model.save_btn, &portal_state.ai_model.help_btn, ...)
    y_cursor += 2

    // ... repeat for each section ...

    // Scrollbar
    if portal_state.content_height > viewport_height {
        let mut sb_state = ScrollbarState::new(portal_state.content_height as usize)
            .position(portal_state.scroll_offset as usize)
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            form_area,
            &mut sb_state,
        )
    }

    // -- Help panel --
    if let Some(section) = portal_state.help_expanded {
        if let Some(help_rect) = help_area {
            render_help_panel(frame, section, help_rect)
        }
    }

    // -- Dropdown overlay (must render last for z-order) --
    if portal_state.ai_model.provider.expanded {
        render_dropdown_overlay(frame, &mut portal_state.ai_model.provider)
    }
```

### 8.7 Per-Section Layout Detail

Each section follows this internal layout:

```
+-- Section Title (bold, accent color) -------------------+
|                                                          |
|  Label                                                   |  <- 1 line, LABEL style
|  +----------------------------------------------------+  |  <- 3 lines (border + content + border)
|  | field value with cursor_                            |  |
|  +----------------------------------------------------+  |
|                                                          |
|  Label                                                   |
|  +----------------------------------------------------+  |
|  | ************key_end                                 |  |
|  +----------------------------------------------------+  |
|                                                          |
|     [ Save ]    [ ? ]                                    |  <- 1 line (compact buttons)
|                                                          |
+----------------------------------------------------------+
```

**Constraint breakdown for one field**:

```rust
let field_layout = Layout::vertical([
    Constraint::Length(1),   // label
    Constraint::Length(3),   // input (border + content + border)
]).split(field_area);
```

**Constraint breakdown for button row**:

```rust
let btn_layout = Layout::horizontal([
    Constraint::Length(2),    // left padding
    Constraint::Length(10),   // [ Save ] button (6 chars + 4 border/pad)
    Constraint::Length(2),    // gap
    Constraint::Length(7),    // [ ? ] button (3 chars + 4 border/pad)
    Constraint::Fill(1),     // remaining space
]).split(button_row_area);
```

### 8.8 Scrolling

The portal uses a virtual viewport approach since Ratatui does not have a
native scroll container. The implementation:

1. During render, compute each widget's absolute Y position as if the form
   were infinitely tall.
2. Subtract `scroll_offset` from each widget's Y to get its viewport-relative position.
3. Skip rendering any widget whose viewport-relative Y + height is < 0
   (scrolled above viewport) or whose viewport-relative Y >= viewport height
   (scrolled below viewport).
4. Clip widgets that partially overlap the viewport boundary using
   Ratatui's `Rect` intersection.

```rust
fn visible_rect(widget_y: u16, widget_height: u16, scroll: u16, viewport: Rect) -> Option<Rect> {
    let rel_y = widget_y as i32 - scroll as i32;
    let rel_bottom = rel_y + widget_height as i32;

    if rel_bottom <= 0 || rel_y >= viewport.height as i32 {
        return None;  // entirely outside viewport
    }

    let clipped_y = rel_y.max(0) as u16;
    let clipped_bottom = (rel_bottom as u16).min(viewport.height);
    let clipped_height = clipped_bottom - clipped_y;

    Some(Rect {
        x: viewport.x,
        y: viewport.y + clipped_y,
        width: viewport.width,
        height: clipped_height,
    })
}
```

**Scroll events**:

| Event | Action |
|-------|--------|
| `MouseEventKind::ScrollUp` in form area | `scroll_offset = scroll_offset.saturating_sub(3)` |
| `MouseEventKind::ScrollDown` in form area | `scroll_offset = (scroll_offset + 3).min(max_scroll)` |
| `KeyCode::PageUp` | `scroll_offset = scroll_offset.saturating_sub(viewport_height)` |
| `KeyCode::PageDown` | `scroll_offset = (scroll_offset + viewport_height).min(max_scroll)` |

**Auto-scroll on focus change**: When Tab/Shift-Tab moves focus to a widget that
is outside the current viewport, the scroll_offset is adjusted to bring that
widget into view:

```rust
fn ensure_visible(widget_y: u16, widget_height: u16, scroll: &mut u16, viewport_h: u16) {
    if widget_y < *scroll {
        *scroll = widget_y;
    } else if widget_y + widget_height > *scroll + viewport_h {
        *scroll = (widget_y + widget_height).saturating_sub(viewport_h);
    }
}
```

### 8.9 Help Panel

When the user clicks `[?]` on a section, a help panel slides open on the right
side of the form (the 65/35 split activates). The panel shows section-specific
setup instructions.

```
fn render_help_panel(frame, section: PortalSection, area: Rect):
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::Blue))
        .title(Span::styled(" How To ", Style::new().fg(Color::Blue).add_modifier(Modifier::BOLD)))
    let inner = block.inner(area)
    frame.render_widget(block, area)

    let text = match section {
        PortalSection::AiModel => vec![
            Line::from(Span::styled("AI Model Setup", Style::new().fg(Color::White).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("1. Choose your AI provider from the dropdown."),
            Line::from("2. Paste your API key from the provider's dashboard."),
            Line::from("3. Enter the model name (e.g., gpt-4o, claude-opus-4-6)."),
            Line::from(""),
            Line::from(Span::styled("Supported providers:", LABEL)),
            Line::from("  - OpenAI (api.openai.com)"),
            Line::from("  - Anthropic (api.anthropic.com)"),
            Line::from("  - Groq (api.groq.com)"),
            Line::from("  - Local (localhost:11434)"),
        ],
        // ... similar for each section
    }

    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }),
        inner,
    )
```

### 8.10 .env Read/Write

IO is decoupled from widget state. The `io/env_store.rs` module handles
all filesystem operations.

```rust
// io/env_store.rs

use std::collections::HashMap;
use std::fs;
use std::path::Path;

const ENV_PATH: &str = ".env";

/// Reads the .env file and returns key-value pairs.
/// Returns empty HashMap if file does not exist.
pub fn read_env() -> HashMap<String, String> {
    let path = Path::new(ENV_PATH);
    if !path.exists() {
        return HashMap::new();
    }
    let content = fs::read_to_string(path).unwrap_or_default();
    content.lines()
        .filter(|line| !line.starts_with('#') && line.contains('='))
        .filter_map(|line| {
            let mut parts = line.splitn(2, '=');
            let key = parts.next()?.trim().to_string();
            let value = parts.next()?.trim().trim_matches('"').to_string();
            Some((key, value))
        })
        .collect()
}

/// Writes/updates specific key-value pairs in the .env file.
/// Preserves existing keys not in the update set.
/// Preserves comments and blank lines.
pub fn write_env(updates: &HashMap<String, String>) -> std::io::Result<()> {
    let path = Path::new(ENV_PATH);
    let existing_content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut existing: Vec<String> = existing_content.lines().map(String::from).collect();
    let mut written_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Update existing lines
    for line in existing.iter_mut() {
        if line.starts_with('#') || !line.contains('=') { continue; }
        if let Some(key) = line.splitn(2, '=').next() {
            let key = key.trim().to_string();
            if let Some(new_value) = updates.get(&key) {
                *line = format!("{}=\"{}\"", key, new_value);
                written_keys.insert(key);
            }
        }
    }

    // Append new keys
    for (key, value) in updates {
        if !written_keys.contains(key) {
            existing.push(format!("{}=\"{}\"", key, value));
        }
    }

    fs::write(path, existing.join("\n") + "\n")
}

/// Returns true if .env file exists and has at least one key.
pub fn env_exists() -> bool {
    Path::new(ENV_PATH).exists()
}
```

**Data flow for Save**:

1. User clicks `[Save]` in the AI Model section.
2. Event handler calls `portal_state.save_ai_model()`.
3. That method collects current values from the section's `TextInputState` and `DropdownState`.
4. Builds a `HashMap` with the relevant keys (`MODEL_PROVIDER`, `MODEL_API_KEY`, `MODEL_NAME`).
5. Calls `env_store::write_env(&updates)`.
6. Sets `portal_state.status_message = Some(("Saved!", StatusKind::Success))`.

**Data flow on Portal tab entry**:

1. When `app.current_tab` changes to `Tab::Portal`, call `portal_state.load_from_env()`.
2. That method calls `env_store::read_env()` and populates each input's `value` field.

---

## 9. App State Additions

The top-level `App` struct gains these fields:

```rust
// app.rs (additions)

pub struct App {
    // ... existing fields ...

    // Screen routing
    pub screen: Screen,

    // Revised tab
    pub current_tab: Tab,  // expanded enum with 8 variants

    // Welcome screen state
    pub welcome: WelcomeState,

    // Portal tab state
    pub portal: PortalState,

    // Tab bar click regions
    pub tab_bar_state: TabBarState,
}

impl App {
    pub fn new(/* ... */) -> Self {
        let screen = if env_store::env_exists() {
            Screen::Main
        } else {
            Screen::Welcome
        };
        // ...
    }

    /// Returns true if any text input widget currently has focus.
    /// Used by the event router to decide whether character keys
    /// go to the focused input or to global shortcuts (tab switching).
    pub fn has_focused_input(&self) -> bool {
        match self.current_tab {
            Tab::Portal => self.portal.has_focused_input(),
            _ => false,
        }
    }
}
```

---

## 10. Event Routing Architecture

The current event loop in `main.rs` handles all events in a flat `match`. With
interactive widgets, this needs to become a layered dispatch.

### 10.1 Dispatch Order

```
Event arrives from crossterm
    |
    v
[1] Global shortcuts (always active)
    - Ctrl+C, 'q' (when no input focused) -> quit
    |
    v
[2] Screen-level routing
    - Screen::Welcome -> welcome_handle_event()
    - Screen::Main    -> main_handle_event()
    |
    v
[3] Tab bar events (Screen::Main only)
    - Mouse click on tab bar region -> tab switch
    - Tab/Shift-Tab key (when no input focused) -> tab cycle
    - Number keys 1-8 (when no input focused) -> direct tab
    |
    v
[4] Active tab content events
    - Tab::Portal -> portal_handle_event()
    - Tab::Learnings -> learnings_handle_event()
    - etc.
    |
    v
[5] Widget-level events (within the active tab)
    - Focused text input -> character input, cursor movement
    - Expanded dropdown -> option selection, dismiss
    - Button hover/click -> state update
```

### 10.2 Event Handler Signatures

```rust
/// Returns true if the event was consumed and should not propagate further.
fn handle_event(app: &mut App, event: &Event) -> bool {
    // [1] Global shortcuts
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press { return false; }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.should_quit = true;
                return true;
            }
            KeyCode::Char('q') if !app.has_focused_input() => {
                app.should_quit = true;
                return true;
            }
            _ => {}
        }
    }

    // [2] Screen routing
    match app.screen {
        Screen::Welcome => handle_welcome_event(app, event),
        Screen::Main    => handle_main_event(app, event),
    }
}

fn handle_main_event(app: &mut App, event: &Event) -> bool {
    // [3] Tab bar
    if let Event::Mouse(mouse) = event {
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            if let Some(tab) = app.tab_bar_state.tab_at(mouse.column, mouse.row) {
                app.current_tab = tab;
                return true;
            }
        }
    }

    if let Event::Key(key) = event {
        if key.kind == KeyEventKind::Press && !app.has_focused_input() {
            match key.code {
                KeyCode::Tab      => { app.current_tab = app.current_tab.next(); return true; }
                KeyCode::BackTab  => { app.current_tab = app.current_tab.prev(); return true; }
                KeyCode::Char(c) if c >= '1' && c <= '8' => {
                    if let Some(tab) = Tab::from_number(c as u8 - b'0') {
                        app.current_tab = tab;
                        return true;
                    }
                }
                _ => {}
            }
        }
    }

    // [4] Active tab content
    match app.current_tab {
        Tab::Portal    => handle_portal_event(app, event),
        Tab::Learnings => handle_learnings_event(app, event),
        Tab::Research  => handle_research_event(app, event),
        _              => false,  // stub tabs consume nothing
    }
}
```

### 10.3 Portal Event Handler

```rust
fn handle_portal_event(app: &mut App, event: &Event) -> bool {
    let portal = &mut app.portal;

    // Dropdown takes priority when expanded (captures all keys/clicks)
    if portal.ai_model.provider.expanded {
        return handle_dropdown_event(&mut portal.ai_model.provider, event);
    }

    // Mouse events: check click targets
    if let Event::Mouse(mouse) = event {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check each widget's area for click-to-focus
                let col = mouse.column;
                let row = mouse.row;

                // Check text inputs (sets focus)
                if is_in_rect(col, row, portal.ai_model.api_key.area) {
                    portal.focus_to(FocusTarget::AiApiKey);
                    return true;
                }
                // ... check all other inputs and buttons ...

                // Check buttons
                if is_in_rect(col, row, portal.ai_model.save_btn.area) {
                    portal.save_ai_model();
                    return true;
                }
                if is_in_rect(col, row, portal.ai_model.help_btn.area) {
                    portal.toggle_help(PortalSection::AiModel);
                    return true;
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let delta: i16 = if mouse.kind == MouseEventKind::ScrollUp { -3 } else { 3 };
                portal.scroll(delta);
                return true;
            }
            MouseEventKind::Moved => {
                // Update hover state on all buttons
                handle_button_mouse(&mut portal.ai_model.save_btn, mouse.kind, mouse.column, mouse.row, app.tick_count);
                // ... all other buttons ...
            }
            _ => {}
        }
    }

    // Key events: route to focused widget
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press { return false; }

        match key.code {
            KeyCode::Tab => {
                portal.focus_next();
                return true;
            }
            KeyCode::BackTab => {
                portal.focus_prev();
                return true;
            }
            KeyCode::Enter => {
                // If focused on a Save button, trigger save
                // If focused on a text input, advance focus
                match portal.current_focus_target() {
                    Some(FocusTarget::AiSave) => { portal.save_ai_model(); return true; }
                    // ... other save buttons ...
                    _ => { portal.focus_next(); return true; }
                }
            }
            _ => {
                // Route to focused text input
                if let Some(input) = portal.focused_input_mut() {
                    return handle_text_input_key(input, key);
                }
            }
        }
    }

    // Bracketed paste
    if let Event::Paste(text) = event {
        if let Some(input) = portal.focused_input_mut() {
            input.paste(text);
            return true;
        }
    }

    false
}
```

---

## 11. Crossterm Event Map

Complete mapping of all Crossterm events to application actions across all
screens and widgets.

### 11.1 Global (all screens)

| Crossterm Event | Condition | Action |
|-----------------|-----------|--------|
| `Key(Char('c'), CONTROL)` | Always | `app.should_quit = true` |
| `Key(Char('q'))` | No text input focused | `app.should_quit = true` |
| `Key(Char('r'))` | No text input focused | `app.refresh()` |

### 11.2 Welcome Screen

| Crossterm Event | Action |
|-----------------|--------|
| `Key(Enter)` | Navigate to Portal tab, `screen = Screen::Main` |
| `Mouse(Down(Left))` in button | Same as Enter |

### 11.3 Tab Bar (Screen::Main, no text input focused)

| Crossterm Event | Action |
|-----------------|--------|
| `Key(Tab)` | `current_tab = current_tab.next()` |
| `Key(BackTab)` | `current_tab = current_tab.prev()` |
| `Key(Char('1'))` .. `Key(Char('8'))` | `current_tab = Tab::from_number(n)` |
| `Mouse(Down(Left))` in tab rect | `current_tab = clicked_tab` |

### 11.4 Learnings / Research Tabs (unchanged from current)

| Crossterm Event | Action |
|-----------------|--------|
| `Key(Up)` | Scroll active list up by 1 |
| `Key(Down)` | Scroll active list down by 1 |
| `Mouse(ScrollUp)` in panel | Scroll panel up by 3 |
| `Mouse(ScrollDown)` in panel | Scroll panel down by 3 |

### 11.5 Portal Tab

| Crossterm Event | Condition | Action |
|-----------------|-----------|--------|
| `Key(Tab)` | Always (in Portal) | Focus next widget |
| `Key(BackTab)` | Always (in Portal) | Focus previous widget |
| `Key(Char(c))` | Text input focused | `input.insert_char(c)` |
| `Key(Backspace)` | Text input focused | `input.delete_char_before()` |
| `Key(Delete)` | Text input focused | `input.delete_char_at()` |
| `Key(Left)` | Text input focused | `input.move_cursor_left()` |
| `Key(Right)` | Text input focused | `input.move_cursor_right()` |
| `Key(Home)` | Text input focused | `input.move_cursor_home()` |
| `Key(End)` | Text input focused | `input.move_cursor_end()` |
| `Key(Enter)` | Save button focused | Trigger section save |
| `Key(Enter)` | Text input focused | Focus next widget |
| `Key(Enter)` | Dropdown focused, closed | Open dropdown |
| `Key(Enter)` | Dropdown open | Select highlighted, close |
| `Key(Esc)` | Dropdown open | Close dropdown |
| `Key(Up)` | Dropdown open | Move highlight up |
| `Key(Down)` | Dropdown open | Move highlight down |
| `Key(Down)` | Dropdown focused, closed | Open dropdown |
| `Key(PageUp)` | Always (in Portal) | Scroll form up by viewport height |
| `Key(PageDown)` | Always (in Portal) | Scroll form down by viewport height |
| `Paste(text)` | Text input focused | `input.paste(&text)` |
| `Mouse(Down(Left))` on input | Always | Focus that input |
| `Mouse(Down(Left))` on dropdown | Closed | Open dropdown |
| `Mouse(Down(Left))` on option | Open | Select option, close |
| `Mouse(Down(Left))` outside dropdown | Open | Close dropdown |
| `Mouse(Down(Left))` on Save btn | Always | Trigger section save |
| `Mouse(Down(Left))` on [?] btn | Always | Toggle help panel |
| `Mouse(ScrollUp)` in form area | Always | Scroll form up by 3 |
| `Mouse(ScrollDown)` in form area | Always | Scroll form down by 3 |
| `Mouse(Moved)` over button | Always | Update button hover state |

---

## Appendix A: New Dependency Requirements

```toml
# No new crate dependencies required.
# All widgets are built from Ratatui primitives.
# Crossterm 0.28 already supports:
#   - MouseEventKind::Down, Moved, ScrollUp, ScrollDown
#   - KeyCode::BackTab (Shift+Tab)
#   - Event::Paste (bracketed paste)
#   - EnableMouseCapture (already enabled in main.rs)
```

To enable bracketed paste support (for clipboard paste into text inputs),
add to terminal setup in `main.rs`:

```rust
use crossterm::event::EnableBracketedPaste;
// In setup:
execute!(stdout, EnterAlternateScreen, EnableMouseCapture, EnableBracketedPaste)?;
// In teardown:
use crossterm::event::DisableBracketedPaste;
execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture, DisableBracketedPaste)?;
```

## Appendix B: Migration Path from Current Code

The refactor is incremental. Each step compiles and runs:

1. **Extract `theme.rs`**: Move all `const` style definitions from `ui.rs` to `theme.rs`.
   Update imports. Zero behavior change.

2. **Expand `Tab` enum**: Add 6 new variants. Update `next_tab()`/`prev_tab()` to use
   the new `next()`/`prev()` methods. Add stub `match` arms that render "Coming Soon"
   `Paragraph` widgets. Zero visual change on Learnings/Research tabs.

3. **Create `widgets/` module**: Implement `ButtonWidget`, `TextInputWidget`,
   `DropdownWidget` as standalone modules. Unit-testable without a terminal.

4. **Create `TabBarState`**: Replace the inline `render_tab_bar` with the new widget.
   Add click-region tracking. Wire `MouseEventKind::Down` in the event loop.

5. **Create `screens/portal.rs`**: Build the Portal tab using the new widgets.
   Wire into the tab router.

6. **Create `screens/welcome.rs`**: Implement the welcome screen. Add `Screen` enum
   to `App`. Wire `.env` detection.

7. **Create `io/env_store.rs`**: Extract `.env` read/write logic. Wire Save buttons
   in Portal to the store.

8. **Extract existing tabs**: Move Learnings rendering to `screens/learnings.rs` and
   Research rendering to `screens/research.rs`. The `ui.rs` file becomes a thin
   orchestrator that calls into `screens/` and `widgets/`.

---

## Appendix C: Accessibility Considerations

### Keyboard-Only Navigation

Every interactive element is reachable via keyboard alone:
- Tab/Shift-Tab cycles through all focusable widgets in a predictable order.
- Enter activates buttons and confirms dropdown selections.
- Arrow keys navigate within dropdowns.
- Number keys 1-8 provide direct tab access.
- No mouse-only features exist; every mouse action has a keyboard equivalent.

### Visual Indicators

- Focus state is always visible (cyan border on focused inputs, reversed style on
  focused tabs).
- Active/hover states provide distinct visual feedback for sighted mouse users.
- Color is never the sole indicator of state -- bold weight and border changes
  accompany all color changes.
- The password partial-reveal (first 4 + last 4 chars) allows users to verify
  their input without fully exposing credentials.

### Terminal Compatibility

- No RGB colors: all styles use the base-16 ANSI palette.
- `Color::Reset` respects the terminal's configured background.
- `Modifier::REVERSED` adapts automatically to light/dark terminal themes.
- Minimum 80-column support with graceful degradation (truncated tab labels).
- Unicode characters used (gear icon, bullets, triangles) are in the Basic
  Multilingual Plane and supported by all modern terminal fonts.
