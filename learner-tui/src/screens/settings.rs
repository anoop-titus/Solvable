use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use std::path::{Path, PathBuf};

use crate::theme;
use crate::io_layer::sysinfo::{self, SystemInfo};
use crate::widgets::button::{ButtonState, ButtonVisual, render_button};
use crate::widgets::slider::{SliderState, render_slider};

// ── Default pipeline parameter values ──────────────────────────────────

const DEFAULT_INGESTION_BATCH: u32 = 50;
const DEFAULT_LEARNING_CHUNK: u32 = 75;
const DEFAULT_RESEARCH_LIMIT: u32 = 30;
const DEFAULT_ISSUE_MESH_CAP: u32 = 60;
const DEFAULT_SOLUTION_LIMIT: u32 = 30;
const DEFAULT_LVL2_BATCH: u32 = 20;
const DEFAULT_CONFLUENCE_CAP: u32 = 50;
const DEFAULT_DEBLOAT_RATE: u32 = 60;

const SLIDER_MAX_MULTIPLIER: u32 = 3; // max = base * 3

/// Pipeline parameter definitions: (label, default, max, kind).
const PARAM_DEFS: [(&str, u32, &str); 8] = [
    ("Ingestion Batch", DEFAULT_INGESTION_BATCH, "fetch"),
    ("Learning Chunk", DEFAULT_LEARNING_CHUNK, "batch"),
    ("Research Limit", DEFAULT_RESEARCH_LIMIT, "volume"),
    ("Issue Mesh Cap", DEFAULT_ISSUE_MESH_CAP, "volume"),
    ("Solution Limit", DEFAULT_SOLUTION_LIMIT, "volume"),
    ("Lvl2 Batch", DEFAULT_LVL2_BATCH, "volume"),
    ("Confluence Cap", DEFAULT_CONFLUENCE_CAP, "volume"),
    ("Debloat Rate", DEFAULT_DEBLOAT_RATE, "volume"),
];

// ── Focus items ────────────────────────────────────────────────────────
// 0..7 = sliders
// 8 = Refresh button
// 9 = Save sysinfo button
// 10 = Save All button
// 11 = Reset Defaults button
const FOCUS_SLIDER_START: usize = 0;
const FOCUS_SLIDER_END: usize = 7; // inclusive
const FOCUS_REFRESH: usize = 8;
const FOCUS_SAVE_SYSINFO: usize = 9;
const FOCUS_SAVE_ALL: usize = 10;
const FOCUS_RESET: usize = 11;
const FOCUS_COUNT: usize = 12;

// ── Settings State ─────────────────────────────────────────────────────

pub struct SettingsState {
    pub sysinfo: SystemInfo,
    pub sliders: Vec<SliderState>,
    pub focus_index: usize,
    pub refresh_btn: ButtonState,
    pub save_sysinfo_btn: ButtonState,
    pub save_btn: ButtonState,
    pub reset_btn: ButtonState,
    pub status_message: Option<(String, bool)>, // (msg, is_success)
    pub status_tick: u64,
    params_path: PathBuf,
    sysinfo_path: PathBuf,
}

impl SettingsState {
    pub fn new(env_dir: &Path) -> Self {
        let sysinfo = SystemInfo::collect();
        let params_path = env_dir.join("pipeline_params.json");
        let sysinfo_path = env_dir.join("sysinfo");

        let mut state = Self {
            sysinfo: sysinfo.clone(),
            sliders: Vec::new(),
            focus_index: 0,
            refresh_btn: ButtonState::new("Refresh"),
            save_sysinfo_btn: ButtonState::new("Save sysinfo"),
            save_btn: ButtonState::new("Save All"),
            reset_btn: ButtonState::new("Reset Defaults"),
            status_message: None,
            status_tick: 0,
            params_path,
            sysinfo_path,
        };

        state.init_sliders(&sysinfo);
        state.load_params();
        state.apply_focus();
        state
    }

    fn init_sliders(&mut self, info: &SystemInfo) {
        self.sliders.clear();
        for (label, default, _kind) in &PARAM_DEFS {
            let max = default * SLIDER_MAX_MULTIPLIER;
            let (yellow, red) = sysinfo::compute_thresholds(max, info);
            self.sliders.push(SliderState::new(label, *default, 1, max, yellow, red));
        }
    }

    /// Load pipeline_params.json if it exists, applying saved values to sliders.
    fn load_params(&mut self) {
        let content = match std::fs::read_to_string(&self.params_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(obj) = parsed.as_object() {
            for slider in &mut self.sliders {
                let key = slider_key(&slider.label);
                if let Some(val) = obj.get(&key).and_then(|v| v.as_u64()) {
                    slider.value = (val as u32).max(slider.min).min(slider.max);
                }
            }
        }
    }

    /// Save current slider values to pipeline_params.json.
    pub fn save_params(&self) -> std::io::Result<()> {
        let mut map = serde_json::Map::new();
        for slider in &self.sliders {
            map.insert(
                slider_key(&slider.label),
                serde_json::Value::Number(serde_json::Number::from(slider.value)),
            );
        }
        let json = serde_json::to_string_pretty(&serde_json::Value::Object(map))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        // Ensure parent directory exists
        if let Some(parent) = self.params_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.params_path, json)
    }

    /// Save sysinfo to plain-text file.
    pub fn save_sysinfo(&self) -> std::io::Result<()> {
        if let Some(parent) = self.sysinfo_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.sysinfo.save_to_file(&self.sysinfo_path)
    }

    /// Re-collect system info and update slider thresholds.
    pub fn refresh_sysinfo(&mut self) {
        self.sysinfo = SystemInfo::collect();
        for slider in &mut self.sliders {
            let (yellow, red) = sysinfo::compute_thresholds(slider.max, &self.sysinfo);
            slider.yellow_threshold = yellow;
            slider.red_threshold = red;
        }
    }

    /// Reset all sliders to default values.
    pub fn reset_defaults(&mut self) {
        for (i, (_, default, _)) in PARAM_DEFS.iter().enumerate() {
            if let Some(slider) = self.sliders.get_mut(i) {
                slider.value = *default;
            }
        }
    }

    // ── Focus management ───────────────────────────────────────────

    pub fn focus_count(&self) -> usize { FOCUS_COUNT }

    pub fn advance_focus(&mut self) {
        // If currently editing a number, commit first
        if let Some(slider) = self.focused_slider_mut_raw() {
            if slider.editing_number {
                slider.commit_edit();
            }
        }
        self.clear_all_focus();
        self.focus_index = (self.focus_index + 1) % FOCUS_COUNT;
        self.apply_focus();
    }

    pub fn retreat_focus(&mut self) {
        if let Some(slider) = self.focused_slider_mut_raw() {
            if slider.editing_number {
                slider.commit_edit();
            }
        }
        self.clear_all_focus();
        self.focus_index = (self.focus_index + FOCUS_COUNT - 1) % FOCUS_COUNT;
        self.apply_focus();
    }

    fn clear_all_focus(&mut self) {
        for slider in &mut self.sliders {
            slider.focused = false;
        }
        self.refresh_btn.visual = ButtonVisual::Normal;
        self.save_sysinfo_btn.visual = ButtonVisual::Normal;
        self.save_btn.visual = ButtonVisual::Normal;
        self.reset_btn.visual = ButtonVisual::Normal;
    }

    fn apply_focus(&mut self) {
        match self.focus_index {
            i @ FOCUS_SLIDER_START..=FOCUS_SLIDER_END => {
                if let Some(slider) = self.sliders.get_mut(i) {
                    slider.focused = true;
                }
            }
            FOCUS_REFRESH => self.refresh_btn.visual = ButtonVisual::Active,
            FOCUS_SAVE_SYSINFO => self.save_sysinfo_btn.visual = ButtonVisual::Active,
            FOCUS_SAVE_ALL => self.save_btn.visual = ButtonVisual::Active,
            FOCUS_RESET => self.reset_btn.visual = ButtonVisual::Active,
            _ => {}
        }
    }

    /// Does the currently focused element capture text input?
    pub fn has_focused_input(&self) -> bool {
        if self.focus_index <= FOCUS_SLIDER_END {
            if let Some(slider) = self.sliders.get(self.focus_index) {
                return slider.editing_number;
            }
        }
        false
    }

    /// Is a slider currently focused (for arrow key capture)?
    pub fn has_focused_slider(&self) -> bool {
        self.focus_index >= FOCUS_SLIDER_START && self.focus_index <= FOCUS_SLIDER_END
    }

    /// Get mutable reference to the focused slider (if any).
    pub fn focused_slider_mut(&mut self) -> Option<&mut SliderState> {
        if self.focus_index >= FOCUS_SLIDER_START && self.focus_index <= FOCUS_SLIDER_END {
            self.sliders.get_mut(self.focus_index)
        } else {
            None
        }
    }

    /// Internal: get focused slider without borrow issues.
    fn focused_slider_mut_raw(&mut self) -> Option<&mut SliderState> {
        if self.focus_index >= FOCUS_SLIDER_START && self.focus_index <= FOCUS_SLIDER_END {
            self.sliders.get_mut(self.focus_index)
        } else {
            None
        }
    }

    /// Which button action was activated via Enter?
    pub fn focused_action(&self) -> Option<SettingsAction> {
        match self.focus_index {
            FOCUS_REFRESH => Some(SettingsAction::Refresh),
            FOCUS_SAVE_SYSINFO => Some(SettingsAction::SaveSysinfo),
            FOCUS_SAVE_ALL => Some(SettingsAction::SaveAll),
            FOCUS_RESET => Some(SettingsAction::ResetDefaults),
            i if i <= FOCUS_SLIDER_END => Some(SettingsAction::ToggleEdit),
            _ => None,
        }
    }

    /// Handle a mouse click at the given position. Returns true if handled.
    pub fn handle_click(&mut self, col: u16, row: u16) -> bool {
        // Check slider bars — find which one was clicked first
        let mut clicked_slider: Option<usize> = None;
        for (i, slider) in self.sliders.iter().enumerate() {
            if slider.bar_hit_test(col, row) {
                clicked_slider = Some(i);
                break;
            }
        }
        if let Some(i) = clicked_slider {
            self.clear_all_focus();
            self.focus_index = i;
            self.sliders[i].set_from_click(col);
            self.sliders[i].focused = true;
            return true;
        }
        // Check buttons
        if self.refresh_btn.hit_test(col, row) {
            self.clear_all_focus();
            self.focus_index = FOCUS_REFRESH;
            self.apply_focus();
            return true;
        }
        if self.save_sysinfo_btn.hit_test(col, row) {
            self.clear_all_focus();
            self.focus_index = FOCUS_SAVE_SYSINFO;
            self.apply_focus();
            return true;
        }
        if self.save_btn.hit_test(col, row) {
            self.clear_all_focus();
            self.focus_index = FOCUS_SAVE_ALL;
            self.apply_focus();
            return true;
        }
        if self.reset_btn.hit_test(col, row) {
            self.clear_all_focus();
            self.focus_index = FOCUS_RESET;
            self.apply_focus();
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SettingsAction {
    Refresh,
    SaveSysinfo,
    SaveAll,
    ResetDefaults,
    ToggleEdit,
}

// ── Rendering ──────────────────────────────────────────────────────────

pub fn render(f: &mut Frame, settings: &mut SettingsState, area: Rect) {
    let block = theme::styled_block("\u{2699} Settings");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let sections = Layout::vertical([
        Constraint::Length(1),  // status line
        Constraint::Length(5),  // system analysis box
        Constraint::Length(1),  // spacer
        Constraint::Min(12),   // pipeline parameters box
    ]).split(inner);

    // ── Status line ──
    if let Some((ref msg, is_success)) = settings.status_message {
        let style = if is_success { theme::SUCCESS } else { Style::default().fg(Color::Red) };
        f.render_widget(
            Paragraph::new(Span::styled(format!("  {}", msg), style)),
            sections[0],
        );
    } else {
        f.render_widget(
            Paragraph::new(Span::styled("  System analysis & pipeline tuning", theme::LABEL)),
            sections[0],
        );
    }

    // ── System Analysis box ──
    render_sysinfo_box(f, settings, sections[1]);

    // ── Pipeline Parameters box ──
    render_pipeline_box(f, settings, sections[3]);
}

fn render_sysinfo_box(f: &mut Frame, settings: &mut SettingsState, area: Rect) {
    let block = theme::styled_block_accent("System Analysis", Color::Cyan);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // CPU + Load line
        Constraint::Length(1), // RAM + Used line
        Constraint::Length(1), // Tier + buttons
    ]).split(inner);

    let info = &settings.sysinfo;

    // Row 1: CPU + Load
    let load_color = match info.tier() {
        "IDLE" => Color::Green,
        "NORMAL" => Color::Yellow,
        "BUSY" => Color::LightRed,
        _ => Color::Red,
    };
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  CPU: ", theme::LABEL),
        Span::styled(format!("{} cores", info.cpu_count), theme::DATA),
        Span::styled("    Load: ", theme::LABEL),
        Span::styled(format!("{:.2}", info.load_avg_1), Style::default().fg(load_color)),
        Span::styled(format!(" / {:.2} / {:.2}", info.load_avg_5, info.load_avg_15), theme::LABEL),
        Span::styled(format!("  ({})", info.tier()), Style::default().fg(load_color).add_modifier(Modifier::BOLD)),
    ])), rows[0]);

    // Row 2: RAM
    let ram_color = if info.ram_usage_pct < 50.0 {
        Color::Green
    } else if info.ram_usage_pct < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    };
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  RAM: ", theme::LABEL),
        Span::styled(format!("{:.1} GB", info.total_ram_gb()), theme::DATA),
        Span::styled("    Used: ", theme::LABEL),
        Span::styled(format!("{:.1} GB", info.used_ram_gb()), Style::default().fg(ram_color)),
        Span::styled(format!(" ({:.0}%)", info.ram_usage_pct), Style::default().fg(ram_color)),
    ])), rows[1]);

    // Row 3: Tier label + buttons
    let btn_cols = Layout::horizontal([
        Constraint::Min(20),     // tier label
        Constraint::Length(12),  // Refresh button
        Constraint::Length(1),   // spacer
        Constraint::Length(18),  // Save sysinfo button
        Constraint::Min(0),     // fill
    ]).split(rows[2]);

    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  Tier: ", theme::LABEL),
        Span::styled(info.tier(), Style::default().fg(load_color).add_modifier(Modifier::BOLD)),
    ])), btn_cols[0]);

    render_button(f, &mut settings.refresh_btn, btn_cols[1]);
    render_button(f, &mut settings.save_sysinfo_btn, btn_cols[3]);
}

fn render_pipeline_box(f: &mut Frame, settings: &mut SettingsState, area: Rect) {
    let block = theme::styled_block_accent("Pipeline Parameters", Color::Magenta);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // sliders (8) + spacer + button row
    let slider_count = settings.sliders.len() as u16;
    let mut constraints: Vec<Constraint> = Vec::new();
    for _ in 0..slider_count {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1)); // spacer
    constraints.push(Constraint::Length(1)); // zone legend
    constraints.push(Constraint::Length(1)); // spacer
    constraints.push(Constraint::Length(1)); // button row
    constraints.push(Constraint::Min(0));   // fill

    let rows = Layout::vertical(constraints).split(inner);

    // Render each slider
    for (i, slider) in settings.sliders.iter_mut().enumerate() {
        let slider_area = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..rows[i]
        };
        render_slider(f, slider, slider_area);
    }

    // Zone legend
    let legend_row = rows[slider_count as usize + 1];
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("                    ", theme::LABEL),
        Span::styled("\u{2588}", Style::default().fg(Color::Green)),
        Span::styled(" Safe  ", theme::LABEL),
        Span::styled("\u{2588}", Style::default().fg(Color::Yellow)),
        Span::styled(" Throttle risk  ", theme::LABEL),
        Span::styled("\u{2588}", Style::default().fg(Color::Red)),
        Span::styled(" CPU/RAM pressure", theme::LABEL),
    ])), Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..legend_row });

    // Button row
    let btn_row = rows[slider_count as usize + 3];
    let btn_cols = Layout::horizontal([
        Constraint::Length(20), // left padding
        Constraint::Length(12), // Save All
        Constraint::Length(2),  // spacer
        Constraint::Length(20), // Reset Defaults
        Constraint::Min(0),    // fill
    ]).split(Rect { x: inner.x + 1, width: inner.width.saturating_sub(2), ..btn_row });

    render_button(f, &mut settings.save_btn, btn_cols[1]);
    render_button(f, &mut settings.reset_btn, btn_cols[3]);
}

/// Render the settings footer.
pub fn render_footer(f: &mut Frame, settings: &SettingsState, area: Rect) {
    let focused_label = match settings.focus_index {
        i @ FOCUS_SLIDER_START..=FOCUS_SLIDER_END => {
            if let Some(s) = settings.sliders.get(i) {
                if s.editing_number {
                    "Type number, Enter=confirm, Esc=cancel"
                } else {
                    "Left/Right=adjust  Enter=type value  Tab=next"
                }
            } else {
                ""
            }
        }
        FOCUS_REFRESH => "Enter=refresh system info",
        FOCUS_SAVE_SYSINFO => "Enter=save sysinfo to file",
        FOCUS_SAVE_ALL => "Enter=save pipeline params",
        FOCUS_RESET => "Enter=reset to defaults",
        _ => "",
    };

    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  Tab: cycle focus  ", theme::LABEL),
        Span::styled(focused_label, Style::default().fg(Color::Cyan)),
        Span::styled("  |  q: quit  r: refresh", theme::LABEL),
    ])), area);
}

// ── Helpers ────────────────────────────────────────────────────────────

/// Convert a slider label to a JSON key: "Ingestion Batch" -> "ingestion_batch".
fn slider_key(label: &str) -> String {
    label.to_lowercase().replace(' ', "_")
}
