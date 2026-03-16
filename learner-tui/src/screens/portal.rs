use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::theme;
use crate::io_layer::oauth::OAuthStatus;
use crate::widgets::text_input::{TextInputState, render_text_input};
use crate::widgets::button::{ButtonState, render_button};
use crate::widgets::dropdown::{DropdownState, render_dropdown};

pub struct PortalState {
    // AI Model section
    pub api_key: TextInputState,
    pub model_dropdown: DropdownState,
    pub ai_save_btn: ButtonState,

    // Dropbox section
    pub dropbox_token: TextInputState,
    pub dropbox_save_btn: ButtonState,

    // IMAP section
    pub imap_host: TextInputState,
    pub imap_port: TextInputState,
    pub imap_user: TextInputState,
    pub imap_pass: TextInputState,
    pub imap_save_btn: ButtonState,

    // Airtable section
    pub airtable_key: TextInputState,
    pub airtable_base: TextInputState,
    pub airtable_save_btn: ButtonState,

    // OAuth authorize buttons
    pub google_auth_btn: ButtonState,
    pub microsoft_auth_btn: ButtonState,
    pub dropbox_auth_btn: ButtonState,
    pub oauth_status: OAuthStatus,

    // Focus tracking
    pub focus_index: usize,
    pub scroll_offset: u16,
    pub status_message: Option<(String, bool)>, // (message, is_success)
    pub status_tick: u64,
}

impl PortalState {
    pub fn new() -> Self {
        Self {
            api_key: TextInputState::new("API Key", true, "sk-or-v1-..."),
            model_dropdown: DropdownState::new("Model", vec![
                "openrouter/hunter-alpha".into(),
                "anthropic/claude-sonnet-4-6".into(),
                "openai/gpt-4o".into(),
                "google/gemini-2.5-pro".into(),
                "meta-llama/llama-3.3-70b".into(),
            ]),
            ai_save_btn: ButtonState::new("Save"),

            dropbox_token: TextInputState::new("Access Token", true, "sl.B7gX..."),
            dropbox_save_btn: ButtonState::new("Save"),

            imap_host: TextInputState::new("Host", false, "imap.gmail.com"),
            imap_port: TextInputState::new("Port", false, "993"),
            imap_user: TextInputState::new("Username", false, "user@gmail.com"),
            imap_pass: TextInputState::new("Password", true, "app-specific password"),
            imap_save_btn: ButtonState::new("Save"),

            airtable_key: TextInputState::new("API Key", true, "pat-..."),
            airtable_base: TextInputState::new("Base ID", false, "appXXXXXXXXX"),
            airtable_save_btn: ButtonState::new("Save"),

            google_auth_btn: ButtonState::new("Authorize Google"),
            microsoft_auth_btn: ButtonState::new("Authorize Microsoft"),
            dropbox_auth_btn: ButtonState::new("Authorize Dropbox"),
            oauth_status: OAuthStatus::Idle,

            focus_index: 0,
            scroll_offset: 0,
            status_message: None,
            status_tick: 0,
        }
    }

    /// Load values from .env HashMap into widget states
    pub fn load_from_env(&mut self, env: &std::collections::HashMap<String, String>) {
        if let Some(v) = env.get("OPENROUTER_API_KEY") { self.api_key.value = v.clone(); }
        if let Some(v) = env.get("DROPBOX_TOKEN") { self.dropbox_token.value = v.clone(); }
        if let Some(v) = env.get("IMAP_HOST") { self.imap_host.value = v.clone(); }
        if let Some(v) = env.get("IMAP_PORT") { self.imap_port.value = v.clone(); }
        if let Some(v) = env.get("IMAP_USER") { self.imap_user.value = v.clone(); }
        if let Some(v) = env.get("IMAP_PASS") { self.imap_pass.value = v.clone(); }
        if let Some(v) = env.get("AIRTABLE_API_KEY") { self.airtable_key.value = v.clone(); }
        if let Some(v) = env.get("AIRTABLE_BASE_ID") { self.airtable_base.value = v.clone(); }
        if let Some(v) = env.get("MODEL_OVERRIDE") {
            if let Some(idx) = self.model_dropdown.options.iter().position(|o| o == v) {
                self.model_dropdown.selected_index = idx;
            }
        }
    }

    // Focus layout (16 items):
    //  0: api_key          1: model_dropdown    2: ai_save_btn
    //  3: dropbox_auth_btn  4: dropbox_token     5: dropbox_save_btn
    //  6: google_auth_btn   7: microsoft_auth_btn
    //  8: imap_host         9: imap_port        10: imap_user       11: imap_pass
    // 12: imap_save_btn
    // 13: airtable_key     14: airtable_base    15: airtable_save_btn
    pub fn focus_count(&self) -> usize { 16 }

    pub fn advance_focus(&mut self) {
        self.clear_all_focus();
        self.focus_index = (self.focus_index + 1) % self.focus_count();
        self.apply_focus();
    }

    pub fn retreat_focus(&mut self) {
        self.clear_all_focus();
        self.focus_index = (self.focus_index + self.focus_count() - 1) % self.focus_count();
        self.apply_focus();
    }

    fn clear_all_focus(&mut self) {
        self.api_key.focused = false;
        self.model_dropdown.focused = false;
        self.model_dropdown.expanded = false;
        self.dropbox_token.focused = false;
        self.imap_host.focused = false;
        self.imap_port.focused = false;
        self.imap_user.focused = false;
        self.imap_pass.focused = false;
        self.airtable_key.focused = false;
        self.airtable_base.focused = false;
    }

    fn apply_focus(&mut self) {
        match self.focus_index {
            0 => self.api_key.focused = true,
            1 => self.model_dropdown.focused = true,
            4 => self.dropbox_token.focused = true,
            8 => self.imap_host.focused = true,
            9 => self.imap_port.focused = true,
            10 => self.imap_user.focused = true,
            11 => self.imap_pass.focused = true,
            13 => self.airtable_key.focused = true,
            14 => self.airtable_base.focused = true,
            // Buttons (2,3,5,6,7,12,15) don't have a "focused" field on TextInputState
            _ => {}
        }
    }

    pub fn has_focused_input(&self) -> bool {
        self.api_key.focused || self.dropbox_token.focused
            || self.imap_host.focused || self.imap_port.focused
            || self.imap_user.focused || self.imap_pass.focused
            || self.airtable_key.focused || self.airtable_base.focused
    }

    pub fn has_focused_dropdown(&self) -> bool {
        self.model_dropdown.focused
    }

    /// Get mutable ref to currently focused TextInputState, if any
    pub fn focused_input_mut(&mut self) -> Option<&mut TextInputState> {
        if self.api_key.focused { return Some(&mut self.api_key); }
        if self.dropbox_token.focused { return Some(&mut self.dropbox_token); }
        if self.imap_host.focused { return Some(&mut self.imap_host); }
        if self.imap_port.focused { return Some(&mut self.imap_port); }
        if self.imap_user.focused { return Some(&mut self.imap_user); }
        if self.imap_pass.focused { return Some(&mut self.imap_pass); }
        if self.airtable_key.focused { return Some(&mut self.airtable_key); }
        if self.airtable_base.focused { return Some(&mut self.airtable_base); }
        None
    }

    pub fn focused_dropdown_mut(&mut self) -> Option<&mut DropdownState> {
        if self.model_dropdown.focused { return Some(&mut self.model_dropdown); }
        None
    }

    /// Returns which save section was activated (if a save button is currently focused)
    pub fn focused_save_section(&self) -> Option<&str> {
        match self.focus_index {
            2 => Some("ai"),
            5 => Some("dropbox"),
            12 => Some("imap"),
            15 => Some("airtable"),
            _ => None,
        }
    }

    /// Returns which OAuth provider button is currently focused
    pub fn focused_oauth_provider(&self) -> Option<crate::io_layer::oauth::OAuthProvider> {
        use crate::io_layer::oauth::OAuthProvider;
        match self.focus_index {
            3 => Some(OAuthProvider::Dropbox),
            6 => Some(OAuthProvider::Google),
            7 => Some(OAuthProvider::Microsoft),
            _ => None,
        }
    }
}

pub fn render(f: &mut Frame, portal: &mut PortalState, area: Rect) {
    let block = theme::styled_block("Portal");
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Layout sections vertically
    let sections = Layout::vertical([
        Constraint::Length(1),  // header / status
        Constraint::Length(6),  // AI Model section
        Constraint::Length(1),  // spacer
        Constraint::Length(5),  // Dropbox section (now includes OAuth btn)
        Constraint::Length(1),  // spacer
        Constraint::Length(12), // IMAP section (now includes 2 OAuth btns)
        Constraint::Length(1),  // spacer
        Constraint::Length(7),  // Airtable section
        Constraint::Min(0),    // fill
    ]).split(inner);

    // Header or status message (includes OAuth status)
    let header_text = match &portal.oauth_status {
        OAuthStatus::WaitingForBrowser => Some(("  Opening browser for authorization...".to_string(), false)),
        OAuthStatus::WaitingForDevice { url, code } => {
            Some((format!("  Go to {} and enter code: {}", url, code), false))
        }
        OAuthStatus::Error(msg) => Some((format!("  OAuth error: {}", msg), false)),
        OAuthStatus::Success(_) => Some(("  OAuth authorization complete!".to_string(), true)),
        _ => None,
    };

    if let Some((ref msg, is_success)) = header_text {
        let style = if is_success { theme::SUCCESS } else { Style::default().fg(Color::Yellow) };
        if matches!(portal.oauth_status, OAuthStatus::Error(_)) {
            f.render_widget(
                Paragraph::new(Span::styled(msg.clone(), Style::default().fg(Color::Red))),
                sections[0],
            );
        } else {
            f.render_widget(
                Paragraph::new(Span::styled(msg.clone(), style)),
                sections[0],
            );
        }
    } else if let Some((ref msg, is_success)) = portal.status_message {
        let style = if is_success { theme::SUCCESS } else { Style::default().fg(Color::Red) };
        f.render_widget(
            Paragraph::new(Span::styled(format!("  {}", msg), style)),
            sections[0],
        );
    } else {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Configure your service credentials", theme::LABEL),
            ])),
            sections[0],
        );
    }

    // AI Model section
    render_ai_section(f, portal, sections[1]);

    // Dropbox section (with OAuth button)
    render_dropbox_section(f, portal, sections[3]);

    // IMAP section (with Google + Microsoft OAuth buttons)
    render_imap_section(f, portal, sections[5]);

    // Airtable section
    render_airtable_section(f, portal, sections[7]);
}

fn render_section_header(f: &mut Frame, title: &str, configured: bool, area: Rect) {
    let status = if configured {
        Span::styled(" \u{2713} ", Style::default().fg(Color::Green))
    } else {
        Span::styled(" \u{2717} ", Style::default().fg(Color::Red))
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("  {} ", title), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            status,
        ])),
        area,
    );
}

fn render_ai_section(f: &mut Frame, portal: &mut PortalState, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Length(3),  // API key
        Constraint::Length(1),  // dropdown + save (horizontal)
    ]).split(area);

    render_section_header(f, "AI Model", !portal.api_key.value.is_empty(), rows[0]);

    let key_area = Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[1] };
    render_text_input(f, &mut portal.api_key, key_area);

    let bottom = Layout::horizontal([
        Constraint::Percentage(70),
        Constraint::Percentage(30),
    ]).split(rows[2]);
    let dropdown_area = Rect { x: bottom[0].x + 2, width: bottom[0].width.saturating_sub(4), ..bottom[0] };
    render_dropdown(f, &mut portal.model_dropdown, Rect { height: 3, ..dropdown_area });
    let btn_area = Rect { x: bottom[1].x, width: 10, ..bottom[1] };
    render_button(f, &mut portal.ai_save_btn, btn_area);
}

fn render_dropbox_section(f: &mut Frame, portal: &mut PortalState, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Length(1),  // OAuth button row
        Constraint::Length(3),  // token + save
    ]).split(area);

    render_section_header(f, "Dropbox", !portal.dropbox_token.value.is_empty(), rows[0]);

    // OAuth authorize button
    let oauth_area = Rect { x: area.x + 2, width: 22, ..rows[1] };
    render_focus_button(f, &mut portal.dropbox_auth_btn, oauth_area, portal.focus_index == 3);

    let fields = Layout::horizontal([
        Constraint::Min(20),
        Constraint::Length(10),
    ]).split(Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[2] });
    render_text_input(f, &mut portal.dropbox_token, fields[0]);
    render_button(f, &mut portal.dropbox_save_btn, Rect { y: fields[1].y + 1, height: 1, ..fields[1] });
}

fn render_imap_section(f: &mut Frame, portal: &mut PortalState, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Length(1),  // OAuth buttons row (Google + Microsoft)
        Constraint::Length(1),  // spacer / "or enter manually" label
        Constraint::Length(3),  // host + port
        Constraint::Length(3),  // user + pass
        Constraint::Length(1),  // save button
    ]).split(area);

    render_section_header(f, "Email / IMAP", !portal.imap_host.value.is_empty(), rows[0]);

    // OAuth authorize buttons (side by side)
    let oauth_cols = Layout::horizontal([
        Constraint::Length(22),
        Constraint::Length(1),
        Constraint::Length(26),
        Constraint::Min(0),
    ]).split(Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[1] });
    render_focus_button(f, &mut portal.google_auth_btn, oauth_cols[0], portal.focus_index == 6);
    render_focus_button(f, &mut portal.microsoft_auth_btn, oauth_cols[2], portal.focus_index == 7);

    // "or enter manually" label
    f.render_widget(
        Paragraph::new(Span::styled("  or enter manually:", Style::default().fg(Color::DarkGray))),
        rows[2],
    );

    let row1 = Layout::horizontal([
        Constraint::Percentage(65),
        Constraint::Percentage(35),
    ]).split(Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[3] });
    render_text_input(f, &mut portal.imap_host, row1[0]);
    render_text_input(f, &mut portal.imap_port, row1[1]);

    let row2 = Layout::horizontal([
        Constraint::Percentage(50),
        Constraint::Percentage(50),
    ]).split(Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[4] });
    render_text_input(f, &mut portal.imap_user, row2[0]);
    render_text_input(f, &mut portal.imap_pass, row2[1]);

    let btn_area = Rect { x: area.x + 2, width: 10, ..rows[5] };
    render_button(f, &mut portal.imap_save_btn, btn_area);
}

/// Render a button with focus highlight (used for OAuth authorize buttons)
fn render_focus_button(f: &mut Frame, state: &mut ButtonState, area: Rect, focused: bool) {
    state.area = area;
    let style = if focused {
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let text = format!("[ {} ]", state.label);
    f.render_widget(
        Paragraph::new(Line::from(vec![Span::styled(text, style)])),
        area,
    );
}

fn render_airtable_section(f: &mut Frame, portal: &mut PortalState, area: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Length(3),  // api key
        Constraint::Length(3),  // base id + save
    ]).split(area);

    render_section_header(f, "Airtable", !portal.airtable_key.value.is_empty(), rows[0]);

    let key_area = Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[1] };
    render_text_input(f, &mut portal.airtable_key, key_area);

    let row2 = Layout::horizontal([
        Constraint::Min(20),
        Constraint::Length(10),
    ]).split(Rect { x: area.x + 2, width: area.width.saturating_sub(4), ..rows[2] });
    render_text_input(f, &mut portal.airtable_base, row2[0]);
    render_button(f, &mut portal.airtable_save_btn, Rect { y: row2[1].y + 1, height: 1, ..row2[1] });
}
