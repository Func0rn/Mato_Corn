use crate::theme::ThemeColors;
use crate::{
    client::{
        persistence::{load_state, SavedState},
        presets::{desk_path_for_name, existing_desk_names, load_presets, save_presets, TerminalPreset},
    },
    providers::DaemonProvider,
    terminal_provider::TerminalProvider,
    utils::new_id,
};
use ratatui::{layout::Rect, widgets::ListState};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Focus {
    Sidebar,
    Topbar,
    Content,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum JumpMode {
    None,
    Active, // ESC pressed in Content - can jump OR use arrows
}

pub const JUMP_LABELS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
pub const CONTENT_ESC_DOUBLE_PRESS_WINDOW_MS: u64 = 300;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum RenameTarget {
    Desk(usize),
    Tab(usize, usize),
    Office(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenameState {
    pub target: RenameTarget,
    pub buffer: String,
    pub cursor: usize, // char index
}

impl RenameState {
    pub fn new(target: RenameTarget, buffer: String) -> Self {
        let cursor = buffer.chars().count();
        Self {
            target,
            buffer,
            cursor,
        }
    }

    pub fn char_len(&self) -> usize {
        self.buffer.chars().count()
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_len());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.char_len();
    }

    pub fn insert_char(&mut self, c: char) {
        let idx = self.byte_index(self.cursor);
        self.buffer.insert(idx, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let end = self.byte_index(self.cursor);
        let start = self.byte_index(self.cursor - 1);
        self.buffer.replace_range(start..end, "");
        self.cursor -= 1;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.char_len() {
            return;
        }
        let start = self.byte_index(self.cursor);
        let end = self.byte_index(self.cursor + 1);
        self.buffer.replace_range(start..end, "");
    }

    pub fn cursor_byte_index(&self) -> usize {
        self.byte_index(self.cursor)
    }

    fn byte_index(&self, char_idx: usize) -> usize {
        if char_idx == 0 {
            return 0;
        }
        let total = self.char_len();
        if char_idx >= total {
            return self.buffer.len();
        }
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.buffer.len())
    }
}

pub struct OfficeSelectorState {
    pub active: bool,
    pub list_state: ListState,
}

impl OfficeSelectorState {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        Self {
            active: false,
            list_state,
        }
    }
}

impl Default for OfficeSelectorState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OfficeDeleteConfirm {
    pub office_idx: usize,
    pub input: String,
}

impl OfficeDeleteConfirm {
    pub fn new(office_idx: usize) -> Self {
        Self {
            office_idx,
            input: String::new(),
        }
    }
}

pub struct DeskDeleteConfirm {
    pub desk_idx: usize,
}

impl DeskDeleteConfirm {
    pub fn new(desk_idx: usize) -> Self {
        Self { desk_idx }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewDeskState {
    pub buffer: String,
    pub cursor: usize,
    pub suggestions: Vec<String>,
    pub selected: usize,
}

impl NewDeskState {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor: 0,
            suggestions: existing_desk_names(),
            selected: 0,
        }
    }

    pub fn filtered(&self) -> Vec<String> {
        let needle = self.buffer.trim();
        if needle.is_empty() {
            return self.suggestions.clone();
        }
        if let Ok(re) = regex::RegexBuilder::new(needle)
            .case_insensitive(true)
            .build()
        {
            return self
                .suggestions
                .iter()
                .filter(|name| re.is_match(name))
                .cloned()
                .collect();
        }
        let needle = needle.to_lowercase();
        self.suggestions
            .iter()
            .filter(|name| name.to_lowercase().contains(&needle))
            .cloned()
            .collect()
    }

    pub fn is_regex_filter(&self) -> bool {
        let needle = self.buffer.trim();
        !needle.is_empty()
            && regex::RegexBuilder::new(needle)
                .case_insensitive(true)
                .build()
                .is_ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresetField {
    Name,
    Command,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetEditor {
    pub index: Option<usize>,
    pub name: String,
    pub command: String,
    pub field: PresetField,
}

impl PresetEditor {
    pub fn new() -> Self {
        Self {
            index: None,
            name: String::new(),
            command: String::new(),
            field: PresetField::Name,
        }
    }

    pub fn edit(index: usize, preset: &TerminalPreset) -> Self {
        Self {
            index: Some(index),
            name: preset.name.clone(),
            command: preset.command.clone(),
            field: PresetField::Name,
        }
    }
}

pub struct TerminalPresetState {
    pub active: bool,
    pub selected: usize,
    pub manage: bool,
    pub editor: Option<PresetEditor>,
}

impl TerminalPresetState {
    pub fn new() -> Self {
        Self {
            active: false,
            selected: 0,
            manage: false,
            editor: None,
        }
    }
}

struct MouseModeCache {
    tab_id: String,
    mouse_enabled: bool,
    checked_at: Instant,
}

pub struct TabEntry {
    pub id: String,
    pub name: String,
    pub preset_name: Option<String>,
    pub provider: Box<dyn TerminalProvider>,
}

impl TabEntry {
    pub fn new_with_options(
        name: impl Into<String>,
        cwd: Option<String>,
        preset: Option<TerminalPreset>,
    ) -> Self {
        let id = new_id();
        let socket_path = crate::utils::get_socket_path()
            .to_string_lossy()
            .to_string();
        let mut provider = DaemonProvider::new(id.clone(), socket_path);
        provider.set_spawn_cwd(cwd);
        if let Some(preset) = &preset {
            provider.set_spawn_init_command(Some(preset.command.clone()));
        }
        Self {
            id,
            name: name.into(),
            preset_name: preset.map(|p| p.name),
            provider: Box::new(provider),
        }
    }

    pub fn with_saved(
        id: String,
        name: impl Into<String>,
        cwd: Option<String>,
        preset: Option<TerminalPreset>,
    ) -> Self {
        let socket_path = crate::utils::get_socket_path()
            .to_string_lossy()
            .to_string();
        let mut provider = DaemonProvider::new(id.clone(), socket_path);
        provider.set_spawn_cwd(cwd);
        if let Some(preset) = &preset {
            provider.set_spawn_init_command(Some(preset.command.clone()));
        }
        Self {
            id: id.clone(),
            name: name.into(),
            preset_name: preset.map(|p| p.name),
            provider: Box::new(provider),
        }
    }

    pub fn spawn_pty(&mut self, rows: u16, cols: u16) {
        self.provider.spawn(rows, cols);
    }

    pub fn resize_pty(&mut self, rows: u16, cols: u16) {
        self.provider.resize(rows, cols);
    }

    pub fn pty_write(&mut self, bytes: &[u8]) {
        self.provider.write(bytes);
    }
}

pub use crate::client::desk::Desk;

pub struct Office {
    pub id: String,
    pub name: String,
    pub desks: Vec<Desk>,
    pub active_desk: usize,
}

impl Office {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            desks: vec![Desk::new("Corn 1")],
            active_desk: 0,
        }
    }
}

pub struct App {
    pub offices: Vec<Office>,
    pub current_office: usize,
    pub list_state: ListState,
    pub focus: Focus,
    pub prev_focus: Focus,
    pub jump_mode: JumpMode,
    pub rename: Option<RenameState>,
    pub office_selector: OfficeSelectorState,
    pub term_rows: u16,
    pub term_cols: u16,
    pub dirty: bool,
    // layout rects
    pub sidebar_list_area: Rect,
    pub sidebar_area: Rect,
    pub topbar_area: Rect,
    pub content_area: Rect,
    pub new_desk_area: Rect,
    pub tab_areas: Vec<Rect>,
    pub tab_area_tab_indices: Vec<usize>,
    pub new_tab_area: Rect,
    pub tab_scroll: usize,
    pub last_click: Option<(u16, u16, std::time::Instant)>,
    /// tab_ids that are ACTIVE (have output in last 2 seconds)
    pub active_tabs: HashSet<String>,
    /// tab_ids that finished a meaningful output burst and need user attention.
    pub alarm_tabs: HashSet<String>,
    pub(crate) alarm_active_since: HashMap<String, Instant>,
    pub(crate) alarm_candidates: HashMap<String, Instant>,
    pub alarm_enabled: bool,
    pub daemon_connected: bool,
    pub daemon_last_ok: Instant,
    pub terminal_titles: HashMap<String, String>,
    /// Spinner animation state
    pub spinner_frame: usize,
    pub last_spinner_update: Instant,
    pub last_title_sync: Instant,
    pub theme: ThemeColors,
    /// Settings screen open
    pub show_settings: bool,
    pub settings_selected: usize,
    /// Some(version) if an update is available
    pub update_available: Option<String>,
    pub last_update_check: Instant,
    /// Trigger onboarding for new office
    pub should_show_onboarding: bool,
    /// Full-screen content-only copy/scroll mode.
    pub copy_mode: bool,
    /// Temporarily release mouse capture so the host terminal can select text
    /// inside Mato's normal layout.
    pub mouse_select_mode: bool,
    /// Office delete confirmation
    pub office_delete_confirm: Option<OfficeDeleteConfirm>,
    /// Desk delete confirmation (yes/no)
    pub desk_delete_confirm: Option<DeskDeleteConfirm>,
    mouse_mode_cache: Option<MouseModeCache>,
    pub(crate) active_status_rx: Option<Receiver<crate::client::status::ActivitySnapshot>>,
    tab_switch_started_at: Option<Instant>,
    pub pending_bell: bool,
    /// Timestamp of last ESC press in Content focus (for double-ESC detection)
    pub last_content_esc: Option<Instant>,
    /// Frame generation for screen cache change detection
    pub last_rendered_screen_gen: u64,
    /// Temporary notification message
    pub toast: Option<(String, Instant)>,
    pub new_desk: Option<NewDeskState>,
    pub terminal_presets: Vec<TerminalPreset>,
    pub terminal_preset_state: TerminalPresetState,
    pub current_terminal_preset: Option<String>,
    /// Whether the outer terminal supports Kitty graphics protocol.
    /// Detected at startup from environment variables.
    pub supports_kitty_graphics: bool,
}

impl App {
    /// Detect if the outer terminal supports Kitty graphics protocol.
    /// Uses env vars — no I/O needed, works before raw mode.
    fn detect_kitty_graphics() -> bool {
        if std::env::var("KITTY_WINDOW_ID").is_ok() {
            return true;
        }
        if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
            || std::env::var("GHOSTTY_BIN_DIR").is_ok()
        {
            return true;
        }
        if matches!(
            std::env::var("TERM_PROGRAM").as_deref(),
            Ok("WezTerm") | Ok("iTerm.app")
        ) {
            return true;
        }
        if std::env::var("WEZTERM_PANE").is_ok() {
            return true;
        }
        false
    }
    pub fn flush_pending_content_esc(&mut self) {
        let Some(prev) = self.last_content_esc else {
            return;
        };
        if prev.elapsed() >= Duration::from_millis(CONTENT_ESC_DOUBLE_PRESS_WINDOW_MS) {
            self.last_content_esc = None;
            self.pty_write(b"\x1b");
        }
    }

    pub fn new() -> Self {
        let mut list_state = ListState::default();
        let terminal_presets = load_presets();
        let (offices, current_office, current_terminal_preset, alarm_enabled): (
            Vec<Office>,
            usize,
            Option<String>,
            bool,
        ) = if let Ok(s) = load_state() {
            let saved_current_office = s.current_office;
            let saved_current_terminal_preset = s.current_terminal_preset.clone();
            let saved_alarm_enabled = s.alarm_enabled;
            let offices: Vec<Office> = s
                .offices
                .into_iter()
                .map(|o| {
                    let desks = o
                        .desks
                        .into_iter()
                        .map(|d| {
                            let cwd = d.cwd.or_else(|| {
                                Some(desk_path_for_name(&d.name).to_string_lossy().to_string())
                            });
                            if let Some(dir) = &cwd {
                                let _ = std::fs::create_dir_all(std::path::Path::new(dir));
                            }
                            let tabs = d
                                .tabs
                                .into_iter()
                                .map(|tb| {
                                    let preset = tb.preset.as_ref().and_then(|name| {
                                        terminal_presets
                                            .iter()
                                            .find(|p| p.name == *name)
                                            .cloned()
                                    });
                                    TabEntry::with_saved(tb.id, tb.name, cwd.clone(), preset)
                                })
                                .collect();
                            Desk {
                                id: d.id,
                                name: d.name,
                                cwd,
                                tabs,
                                active_tab: d.active_tab,
                            }
                        })
                        .collect();
                    Office {
                        id: o.id,
                        name: o.name,
                        desks,
                        active_desk: o.active_desk,
                    }
                })
                .collect();
            if offices.is_empty() {
                (
                    vec![Office::new("Default")],
                    0,
                    saved_current_terminal_preset,
                    saved_alarm_enabled,
                )
            } else {
                (
                    offices,
                    saved_current_office,
                    saved_current_terminal_preset,
                    saved_alarm_enabled,
                )
            }
        } else {
            (vec![Office::new("Default")], 0, None, true)
        };
        let current_office = current_office.min(offices.len().saturating_sub(1));
        let active_desk = offices[current_office]
            .active_desk
            .min(offices[current_office].desks.len().saturating_sub(1));
        let mut offices = offices;
        offices[current_office].active_desk = active_desk;
        list_state.select(Some(active_desk));
        Self {
            offices,
            current_office,
            list_state,
            focus: Focus::Sidebar,
            prev_focus: Focus::Sidebar,
            jump_mode: JumpMode::None,
            rename: None,
            office_selector: OfficeSelectorState::new(),
            term_rows: 24,
            term_cols: 80,
            dirty: false,
            sidebar_list_area: Rect::default(),
            sidebar_area: Rect::default(),
            topbar_area: Rect::default(),
            content_area: Rect::default(),
            new_desk_area: Rect::default(),
            tab_areas: vec![],
            new_tab_area: Rect::default(),
            tab_area_tab_indices: vec![],
            tab_scroll: 0,
            last_click: None,
            active_tabs: HashSet::new(),
            alarm_tabs: HashSet::new(),
            alarm_active_since: HashMap::new(),
            alarm_candidates: HashMap::new(),
            alarm_enabled,
            daemon_connected: false,
            daemon_last_ok: Instant::now(),
            terminal_titles: HashMap::new(),
            spinner_frame: 0,
            last_spinner_update: Instant::now(),
            last_title_sync: Instant::now() - Duration::from_millis(500),
            theme: crate::theme::load(),
            show_settings: false,
            settings_selected: crate::theme::selected_index(),
            update_available: None,
            // Force first update check immediately after startup.
            last_update_check: Instant::now() - std::time::Duration::from_secs(3601),
            should_show_onboarding: false,
            copy_mode: false,
            mouse_select_mode: false,
            office_delete_confirm: None,
            desk_delete_confirm: None,
            mouse_mode_cache: None,
            active_status_rx: None,
            tab_switch_started_at: None,
            pending_bell: false,
            last_content_esc: None,
            last_rendered_screen_gen: 0,
            toast: if !crate::theme::supports_truecolor()
                && crate::theme::selected_name() != "system"
            {
                Some((
                    "Theme disabled: terminal lacks truecolor (set COLORTERM=truecolor)".into(),
                    Instant::now(),
                ))
            } else {
                None
            },
            new_desk: None,
            terminal_presets,
            terminal_preset_state: TerminalPresetState::new(),
            current_terminal_preset,
            supports_kitty_graphics: Self::detect_kitty_graphics(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn from_saved(state: SavedState) -> Self {
        let mut list_state = ListState::default();
        let terminal_presets = load_presets();
        let mut offices = state
            .offices
            .into_iter()
            .map(|o| {
                let desks = o
                    .desks
                    .into_iter()
                    .map(|d| {
                        let cwd = d.cwd.or_else(|| {
                            Some(desk_path_for_name(&d.name).to_string_lossy().to_string())
                        });
                        if let Some(dir) = &cwd {
                            let _ = std::fs::create_dir_all(std::path::Path::new(dir));
                        }
                        let tabs: Vec<TabEntry> = d
                            .tabs
                            .into_iter()
                            .map(|tb| {
                                let preset = tb.preset.as_ref().and_then(|name| {
                                    terminal_presets
                                        .iter()
                                        .find(|p| p.name == *name)
                                        .cloned()
                                });
                                TabEntry::with_saved(tb.id, tb.name, cwd.clone(), preset)
                            })
                            .collect();
                        let n_tabs = tabs.len().max(1);
                        Desk {
                            id: d.id,
                            name: d.name,
                            cwd,
                            active_tab: d.active_tab.min(n_tabs - 1),
                            tabs,
                        }
                    })
                    .collect();
                Office {
                    id: o.id,
                    name: o.name,
                    desks,
                    active_desk: o.active_desk,
                }
            })
            .collect::<Vec<_>>();
        if offices.is_empty() {
            offices.push(Office::new("Default"));
        }
        let current_office = state.current_office.min(offices.len().saturating_sub(1));
        let active_desk = offices[current_office]
            .active_desk
            .min(offices[current_office].desks.len().saturating_sub(1));
        offices[current_office].active_desk = active_desk;
        list_state.select(Some(active_desk));

        Self {
            offices,
            current_office,
            list_state,
            focus: Focus::Sidebar,
            prev_focus: Focus::Sidebar,
            jump_mode: JumpMode::None,
            rename: None,
            office_selector: OfficeSelectorState::new(),
            term_rows: 24,
            term_cols: 80,
            dirty: false,
            sidebar_list_area: Rect::default(),
            sidebar_area: Rect::default(),
            topbar_area: Rect::default(),
            content_area: Rect::default(),
            new_desk_area: Rect::default(),
            tab_areas: vec![],
            new_tab_area: Rect::default(),
            tab_area_tab_indices: vec![],
            tab_scroll: 0,
            last_click: None,
            active_tabs: HashSet::new(),
            alarm_tabs: HashSet::new(),
            alarm_active_since: HashMap::new(),
            alarm_candidates: HashMap::new(),
            alarm_enabled: state.alarm_enabled,
            daemon_connected: false,
            daemon_last_ok: Instant::now(),
            terminal_titles: HashMap::new(),
            spinner_frame: 0,
            last_spinner_update: Instant::now(),
            last_title_sync: Instant::now() - Duration::from_millis(500),
            theme: crate::theme::load(),
            show_settings: false,
            settings_selected: crate::theme::selected_index(),
            update_available: None,
            // Force first update check immediately after startup.
            last_update_check: Instant::now() - std::time::Duration::from_secs(3601),
            should_show_onboarding: false,
            copy_mode: false,
            mouse_select_mode: false,
            office_delete_confirm: None,
            desk_delete_confirm: None,
            mouse_mode_cache: None,
            active_status_rx: None,
            tab_switch_started_at: None,
            pending_bell: false,
            last_content_esc: None,
            last_rendered_screen_gen: 0,
            toast: if !crate::theme::supports_truecolor()
                && crate::theme::selected_name() != "system"
            {
                Some((
                    "Theme disabled: terminal lacks truecolor (set COLORTERM=truecolor)".into(),
                    Instant::now(),
                ))
            } else {
                None
            },
            new_desk: None,
            terminal_presets,
            terminal_preset_state: TerminalPresetState::new(),
            current_terminal_preset: state.current_terminal_preset,
            supports_kitty_graphics: Self::detect_kitty_graphics(),
        }
    }

    // Helper methods to access current office's desks
    #[allow(dead_code)]
    pub fn desks(&self) -> &Vec<Desk> {
        &self.offices[self.current_office].desks
    }
    #[allow(dead_code)]
    pub fn desks_mut(&mut self) -> &mut Vec<Desk> {
        &mut self.offices[self.current_office].desks
    }
    #[allow(dead_code)]
    pub fn office(&self) -> &Office {
        &self.offices[self.current_office]
    }
    #[allow(dead_code)]
    pub fn office_mut(&mut self) -> &mut Office {
        &mut self.offices[self.current_office]
    }

    pub fn selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn select_desk(&mut self, desk_idx: usize) {
        if self.offices.is_empty() {
            self.list_state.select(None);
            return;
        }
        let max = self.offices[self.current_office]
            .desks
            .len()
            .saturating_sub(1);
        let idx = desk_idx.min(max);
        let prev = self.selected();
        self.list_state.select(Some(idx));
        self.offices[self.current_office].active_desk = idx;
        if prev != idx {
            self.dirty = true;
        }
    }

    #[allow(dead_code)]
    pub fn active_desk(&self) -> usize {
        self.selected()
    }

    pub fn cur_desk_mut(&mut self) -> &mut Desk {
        let i = self.selected();
        &mut self.offices[self.current_office].desks[i]
    }

    /// Create a new tab in the current desk's bound base directory.
    pub fn new_tab_inheriting_cwd(&mut self) {
        let i = self.selected();
        let cwd = self.offices[self.current_office].desks[i].cwd.clone();
        let preset = self.current_preset();
        self.offices[self.current_office].desks[i].new_tab(cwd, preset);
    }

    fn current_preset(&self) -> Option<TerminalPreset> {
        self.current_terminal_preset
            .as_ref()
            .and_then(|name| self.terminal_presets.iter().find(|p| p.name == *name).cloned())
    }

    pub fn switch_office(&mut self, office_idx: usize) {
        if office_idx < self.offices.len() {
            self.current_office = office_idx;
            let active_desk = self.offices[office_idx].active_desk;
            self.select_desk(active_desk);
            self.tab_scroll = 0;
            self.mark_tab_switch();
            self.spawn_active_pty();
            self.dirty = true;
        }
    }

    pub fn new_desk(&mut self) {
        self.new_desk = Some(NewDeskState::new());
        self.dirty = true;
    }

    pub fn commit_new_desk(&mut self) {
        let Some(state) = self.new_desk.take() else {
            return;
        };
        let name = state.buffer.trim().to_string();
        if name.is_empty() {
            return;
        }
        let path = desk_path_for_name(&name);
        if let Err(e) = std::fs::create_dir_all(&path) {
            self.show_toast(format!("Cannot create desk dir: {}", e));
            return;
        }
        let cwd = Some(path.to_string_lossy().to_string());
        self.offices[self.current_office]
            .desks
            .push(Desk::new_bound(name.clone(), cwd));
        self.select_desk(
            self.offices[self.current_office]
                .desks
                .len()
                .saturating_sub(1),
        );
        self.spawn_active_pty();
        self.show_toast(format!("Desk \"{}\" ready", name));
        self.dirty = true;
    }

    pub fn cancel_new_desk(&mut self) {
        self.new_desk = None;
        self.dirty = true;
    }

    pub fn new_desk_insert_char(&mut self, c: char) {
        if let Some(state) = &mut self.new_desk {
            state.buffer.insert(state.cursor, c);
            state.cursor += c.len_utf8();
            state.selected = 0;
            self.dirty = true;
        }
    }

    pub fn new_desk_backspace(&mut self) {
        if let Some(state) = &mut self.new_desk {
            if state.cursor == 0 {
                return;
            }
            if let Some((idx, _)) = state.buffer[..state.cursor].char_indices().last() {
                state.buffer.replace_range(idx..state.cursor, "");
                state.cursor = idx;
                state.selected = 0;
                self.dirty = true;
            }
        }
    }

    pub fn new_desk_select(&mut self, delta: i32) {
        if let Some(state) = &mut self.new_desk {
            let len = state.filtered().len();
            if len == 0 {
                return;
            }
            let max = len.saturating_sub(1) as i32;
            state.selected = (state.selected as i32 + delta).clamp(0, max) as usize;
            self.dirty = true;
        }
    }

    pub fn new_desk_complete(&mut self) {
        if let Some(state) = &mut self.new_desk {
            let filtered = state.filtered();
            if let Some(name) = filtered.get(state.selected) {
                state.buffer = name.clone();
                state.cursor = state.buffer.len();
                self.dirty = true;
            }
        }
    }

    pub fn open_terminal_presets(&mut self) {
        self.terminal_presets = load_presets();
        self.terminal_preset_state.active = true;
        self.terminal_preset_state.manage = false;
        self.terminal_preset_state.editor = None;
        self.terminal_preset_state.selected = self
            .current_terminal_preset
            .as_ref()
            .and_then(|name| self.terminal_presets.iter().position(|p| p.name == *name))
            .unwrap_or(self.terminal_preset_state.selected)
            .min(self.terminal_presets.len().saturating_sub(1));
        self.dirty = true;
    }

    pub fn close_terminal_presets(&mut self) {
        self.terminal_preset_state.active = false;
        self.terminal_preset_state.editor = None;
        self.dirty = true;
    }

    pub fn select_terminal_preset(&mut self, delta: i32) {
        if self.terminal_presets.is_empty() {
            return;
        }
        let max = self.terminal_presets.len().saturating_sub(1) as i32;
        self.terminal_preset_state.selected =
            (self.terminal_preset_state.selected as i32 + delta).clamp(0, max) as usize;
        self.dirty = true;
    }

    pub fn set_selected_preset_as_default(&mut self) {
        let Some(preset) = self
            .terminal_presets
            .get(self.terminal_preset_state.selected)
            .cloned()
        else {
            return;
        };
        self.current_terminal_preset = Some(preset.name.clone());
        self.show_toast(format!("Default preset: {}", preset.name));
        self.close_terminal_presets();
        self.dirty = true;
    }

    pub fn begin_new_preset(&mut self) {
        self.terminal_preset_state.editor = Some(PresetEditor::new());
        self.dirty = true;
    }

    pub fn begin_edit_preset(&mut self) {
        let idx = self.terminal_preset_state.selected;
        if let Some(preset) = self.terminal_presets.get(idx) {
            self.terminal_preset_state.editor = Some(PresetEditor::edit(idx, preset));
            self.dirty = true;
        }
    }

    pub fn delete_selected_preset(&mut self) {
        if self.terminal_presets.len() <= 1 {
            self.show_toast("Keep at least one preset");
            return;
        }
        let idx = self.terminal_preset_state.selected;
        if idx < self.terminal_presets.len() {
            let removed = self.terminal_presets.remove(idx);
            if self.current_terminal_preset.as_ref() == Some(&removed.name) {
                self.current_terminal_preset = None;
            }
            self.terminal_preset_state.selected = idx.min(self.terminal_presets.len() - 1);
            if let Err(e) = save_presets(&self.terminal_presets) {
                self.show_toast(format!("Save preset failed: {}", e));
            } else {
                self.show_toast(format!("Preset \"{}\" deleted", removed.name));
            }
            self.dirty = true;
        }
    }

    pub fn commit_preset_editor(&mut self) {
        let Some(editor) = self.terminal_preset_state.editor.take() else {
            return;
        };
        let name = editor.name.trim().to_string();
        if name.is_empty() {
            self.show_toast("Preset name is required");
            return;
        }
        let preset = TerminalPreset::new(name, editor.command.trim().to_string());
        if let Some(idx) = editor.index {
            if idx < self.terminal_presets.len() {
                let old_name = self.terminal_presets[idx].name.clone();
                if self.current_terminal_preset.as_ref() == Some(&old_name) {
                    self.current_terminal_preset = Some(preset.name.clone());
                }
                self.terminal_presets[idx] = preset;
            }
        } else {
            self.terminal_presets.push(preset);
            self.terminal_preset_state.selected = self.terminal_presets.len() - 1;
        }
        if let Err(e) = save_presets(&self.terminal_presets) {
            self.show_toast(format!("Save preset failed: {}", e));
        }
        self.dirty = true;
    }

    pub fn cancel_preset_editor(&mut self) {
        self.terminal_preset_state.editor = None;
        self.dirty = true;
    }

    pub fn request_close_desk(&mut self) {
        if self.offices[self.current_office].desks.len() <= 1 {
            return;
        }
        self.desk_delete_confirm = Some(DeskDeleteConfirm::new(self.selected()));
    }

    #[allow(dead_code)]
    pub fn close_desk(&mut self) {
        if self.offices[self.current_office].desks.len() <= 1 {
            return;
        }
        let idx = self.selected();
        self.close_desk_at(idx);
    }

    fn close_desk_at(&mut self, idx: usize) {
        if self.offices[self.current_office].desks.len() <= 1 {
            return;
        }
        if idx >= self.offices[self.current_office].desks.len() {
            return;
        }

        // Close all PTYs in this desk
        let desk = &self.offices[self.current_office].desks[idx];
        let socket_path = crate::utils::get_socket_path();
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&socket_path) {
            use crate::protocol::ClientMsg;
            use std::io::Write;
            for tab in &desk.tabs {
                let msg = ClientMsg::ClosePty {
                    tab_id: tab.id.clone(),
                };
                if let Ok(json) = serde_json::to_vec(&msg) {
                    let _ = stream.write_all(&json);
                    let _ = stream.write_all(b"\n");
                }
            }
            let _ = stream.flush();
        }

        self.offices[self.current_office].desks.remove(idx);
        self.select_desk(
            idx.min(
                self.offices[self.current_office]
                    .desks
                    .len()
                    .saturating_sub(1),
            ),
        );
        self.tab_scroll = 0;
        self.mark_tab_switch();
        self.spawn_active_pty();
        self.dirty = true;
    }

    pub fn confirm_close_desk(&mut self, desk_idx: usize) {
        self.desk_delete_confirm = None;
        let desk_name = self.offices[self.current_office]
            .desks
            .get(desk_idx)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        self.close_desk_at(desk_idx);
        if !desk_name.is_empty() {
            self.show_toast(format!("Desk \"{}\" closed", desk_name));
        }
    }

    pub fn nav(&mut self, delta: i32) {
        let max = self.offices[self.current_office]
            .desks
            .len()
            .saturating_sub(1) as i32;
        let next = (self.selected() as i32 + delta).clamp(0, max) as usize;
        let changed = self.selected() != next;
        self.select_desk(next);
        self.tab_scroll = 0;
        if changed {
            self.mark_tab_switch();
            self.spawn_active_pty();
            self.dirty = true;
        }
    }

    pub fn spawn_active_pty(&mut self) {
        let (rows, cols) = (self.term_rows, self.term_cols);
        let i = self.selected();
        let at = self.offices[self.current_office].desks[i].active_tab;
        self.offices[self.current_office].desks[i].tabs[at].spawn_pty(rows, cols);
    }

    pub fn restart_active_pty(&mut self) {
        let i = self.selected();
        let at = self.offices[self.current_office].desks[i].active_tab;
        let tab_id = self.offices[self.current_office].desks[i].tabs[at]
            .id
            .clone();

        let socket_path = crate::utils::get_socket_path();
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&socket_path) {
            use crate::protocol::ClientMsg;
            use std::io::Write;
            let msg = ClientMsg::ClosePty {
                tab_id: tab_id.clone(),
            };
            if let Ok(json) = serde_json::to_vec(&msg) {
                let _ = stream.write_all(&json);
                let _ = stream.write_all(b"\n");
                let _ = stream.flush();
            }
        }

        self.spawn_active_pty();
        self.mark_tab_switch();
    }

    /// Called after draw() detects the content area size changed.
    /// Sends resize to all PTYs in the current office.
    pub fn resize_all_ptys(&mut self, rows: u16, cols: u16) {
        for desk in &mut self.offices[self.current_office].desks {
            desk.resize_all_ptys(rows, cols);
        }
    }

    pub fn pty_write(&mut self, bytes: &[u8]) {
        let i = self.selected();
        let at = self.offices[self.current_office].desks[i].active_tab;
        self.offices[self.current_office].desks[i].tabs[at].pty_write(bytes);
    }

    pub fn pty_paste(&mut self, text: &str) {
        let i = self.selected();
        let at = self.offices[self.current_office].desks[i].active_tab;
        self.offices[self.current_office].desks[i].tabs[at]
            .provider
            .paste(text);
    }

    pub fn pty_scroll(&mut self, delta: i32) {
        let i = self.selected();
        let at = self.offices[self.current_office].desks[i].active_tab;
        self.offices[self.current_office].desks[i].tabs[at]
            .provider
            .scroll(delta);
    }

    pub fn active_provider_screen_generation(&self) -> u64 {
        let i = self.selected();
        let desk = &self.offices[self.current_office].desks[i];
        if desk.tabs.is_empty() {
            return 0;
        }
        desk.tabs[desk.active_tab].provider.screen_generation()
    }

    /// Emit any pending Kitty graphics APC sequences to the outer terminal.
    ///
    /// Should be called after each render, only when `supports_kitty_graphics` is true.
    /// Translates PTY cursor coordinates to outer terminal coordinates using `content_area`.
    pub fn emit_pending_graphics(&mut self) {
        if !self.supports_kitty_graphics {
            return;
        }
        let i = self.selected();
        let desk = &self.offices[self.current_office].desks[i];
        if desk.tabs.is_empty() {
            return;
        }
        let payloads = desk.tabs[desk.active_tab].provider.take_pending_graphics();
        if payloads.is_empty() {
            return;
        }

        // Get the current display cursor (row, col) relative to content area
        let (sub_rows, sub_cols) = (self.content_area.height, self.content_area.width);
        let content = desk.tabs[desk.active_tab]
            .provider
            .get_screen(sub_rows.saturating_sub(2), sub_cols.saturating_sub(2));
        let (cursor_row, cursor_col) = content.cursor;

        // Translate: content_area has a 1-cell border; content starts at (x+1, y+1)
        let outer_row = self.content_area.y + 1 + cursor_row;
        let outer_col = self.content_area.x + 1 + cursor_col;

        use std::io::Write;
        let mut stdout = std::io::stdout();
        // Save cursor, move to translated position, emit all APC sequences, restore cursor
        let _ = write!(stdout, "\x1b[s\x1b[{};{}H", outer_row + 1, outer_col + 1);
        for apc in &payloads {
            let _ = stdout.write_all(apc);
        }
        let _ = write!(stdout, "\x1b[u");
        let _ = stdout.flush();
    }

    pub fn pty_mouse_mode_enabled(&mut self) -> bool {
        let i = self.selected();
        let at = self.offices[self.current_office].desks[i].active_tab;
        let tab_id = self.offices[self.current_office].desks[i].tabs[at]
            .id
            .clone();
        if let Some(cache) = &self.mouse_mode_cache {
            if cache.tab_id == tab_id && cache.checked_at.elapsed() < Duration::from_millis(100) {
                return cache.mouse_enabled;
            }
        }

        let enabled = self.offices[self.current_office].desks[i].tabs[at]
            .provider
            .mouse_mode_enabled();
        self.mouse_mode_cache = Some(MouseModeCache {
            tab_id,
            mouse_enabled: enabled,
            checked_at: Instant::now(),
        });
        enabled
    }

    /// Send focus in/out events to PTY when focus changes.
    /// Only sends if the PTY application has enabled focus tracking (\x1b[?1004h).
    pub fn sync_focus_events(&mut self) {
        if self.prev_focus == self.focus {
            return;
        }
        let was_content = self.prev_focus == Focus::Content;
        let is_content = self.focus == Focus::Content;
        if is_content || was_content {
            let i = self.selected();
            let desk = &self.offices[self.current_office].desks[i];
            let enabled =
                !desk.tabs.is_empty() && desk.tabs[desk.active_tab].provider.focus_events_enabled();
            if enabled {
                if is_content {
                    self.pty_write(b"\x1b[I");
                }
                if was_content {
                    self.pty_write(b"\x1b[O");
                }
            }
        }
        self.prev_focus = self.focus;
    }

    /// Send FocusIn (`\x1b[I`) or FocusOut (`\x1b[O`) to the currently active
    /// PTY. Only fires when in Content focus and the terminal has opted in via
    /// `\x1b[?1004h`. Call this before and after any desk/tab switch that
    /// happens while remaining in Content focus, so inner TUI apps (vim, helix,
    /// etc.) are notified even when mato's focus mode doesn't change.
    pub fn pty_send_focus_event(&mut self, focus_in: bool) {
        if self.focus != Focus::Content {
            return;
        }
        let i = self.selected();
        let desk = &self.offices[self.current_office].desks[i];
        if desk.tabs.is_empty() {
            return;
        }
        if desk.tabs[desk.active_tab].provider.focus_events_enabled() {
            self.pty_write(if focus_in { b"\x1b[I" } else { b"\x1b[O" });
        }
    }

    pub fn begin_rename_desk(&mut self, idx: usize) {
        let name = self.offices[self.current_office].desks[idx].name.clone();
        self.rename = Some(RenameState::new(RenameTarget::Desk(idx), name));
    }

    /// Start renaming active tab of current task
    pub fn begin_rename_tab(&mut self) {
        let ti = self.selected();
        let at = self.offices[self.current_office].desks[ti].active_tab;
        let name = self.offices[self.current_office].desks[ti].tabs[at]
            .name
            .clone();
        self.rename = Some(RenameState::new(RenameTarget::Tab(ti, at), name));
    }

    pub fn commit_rename(&mut self) {
        if let Some(rename) = self.rename.take() {
            let name = rename.buffer.trim().to_string();
            if name.is_empty() {
                return;
            }
            match rename.target {
                RenameTarget::Desk(i) => {
                    self.offices[self.current_office].desks[i].name = name.clone();
                    let path = desk_path_for_name(&name);
                    if std::fs::create_dir_all(&path).is_ok() {
                        self.offices[self.current_office].desks[i].cwd =
                            Some(path.to_string_lossy().to_string());
                    }
                    self.show_toast(format!("Desk renamed to \"{}\"", name));
                }
                RenameTarget::Tab(ti, at) => {
                    self.offices[self.current_office].desks[ti].tabs[at].name = name.clone();
                    self.show_toast(format!("Tab renamed to \"{}\"", name));
                }
                RenameTarget::Office(i) => {
                    self.offices[i].name = name.clone();
                    self.show_toast(format!("Office renamed to \"{}\"", name));
                }
            }
            self.dirty = true;
        }
    }

    pub fn cancel_rename(&mut self) {
        self.rename = None;
    }

    /// Handle jump mode character selection
    pub fn handle_jump_selection(&mut self, c: char) {
        let targets = self.jump_targets();
        let labels = self.jump_labels();
        let origin_focus = self.focus;

        // Map character to target
        if let Some(idx) = labels.iter().position(|&ch| ch == c) {
            if idx < targets.len() {
                let (kind, task_idx, tab_idx) = targets[idx];
                match kind {
                    't' => {
                        // Jump to desk target.
                        self.pty_send_focus_event(false); // FocusOut to current tab
                        self.select_desk(task_idx);
                        self.focus = match origin_focus {
                            Focus::Content => Focus::Content,
                            Focus::Topbar => Focus::Sidebar,
                            Focus::Sidebar => Focus::Sidebar,
                        };
                        self.mark_tab_switch();
                        self.spawn_active_pty();
                        self.pty_send_focus_event(true); // FocusIn to new tab
                    }
                    'b' => {
                        // Jump to tab target.
                        self.pty_send_focus_event(false); // FocusOut to current tab
                        self.offices[self.current_office].desks[task_idx].active_tab = tab_idx;
                        self.focus = match origin_focus {
                            Focus::Content => Focus::Content,
                            Focus::Sidebar => Focus::Topbar,
                            Focus::Topbar => Focus::Topbar,
                        };
                        self.mark_tab_switch();
                        self.spawn_active_pty();
                        self.pty_send_focus_event(true); // FocusIn to new tab
                    }
                    _ => {}
                }
                self.dirty = true;
            }
        }

        // Exit jump mode
        self.jump_mode = JumpMode::None;
    }

    pub fn mark_tab_switch(&mut self) {
        self.tab_switch_started_at = Some(Instant::now());
        self.acknowledge_active_tab_alarm();
    }

    pub fn finish_tab_switch_measurement(&mut self) -> Option<Duration> {
        self.tab_switch_started_at.take().map(|t| t.elapsed())
    }

    /// Show a transient toast notification (bottom-right, 3s).
    pub fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
        self.dirty = true;
    }

    /// Check if any tab is active
    pub fn has_active_tabs(&self) -> bool {
        !self.active_tabs.is_empty()
    }

    pub fn has_alarm_tabs(&self) -> bool {
        self.alarm_enabled && !self.alarm_tabs.is_empty()
    }

    pub fn active_tab_id(&self) -> Option<&str> {
        let desk = self.offices.get(self.current_office)?.desks.get(self.selected())?;
        desk.tabs.get(desk.active_tab).map(|tab| tab.id.as_str())
    }

    pub fn acknowledge_active_tab_alarm(&mut self) {
        let Some(tab_id) = self.active_tab_id().map(str::to_string) else {
            return;
        };
        if self.alarm_tabs.remove(&tab_id) {
            self.dirty = true;
        }
    }

    pub fn clear_alarm_state(&mut self) {
        let had_alarm_state = !self.alarm_tabs.is_empty()
            || !self.alarm_candidates.is_empty()
            || !self.alarm_active_since.is_empty();
        self.alarm_tabs.clear();
        self.alarm_candidates.clear();
        self.alarm_active_since.clear();
        if had_alarm_state {
            self.dirty = true;
        }
    }

    pub fn toggle_alarm_mode(&mut self) {
        self.alarm_enabled = !self.alarm_enabled;
        if !self.alarm_enabled {
            self.clear_alarm_state();
        }
        self.show_toast(if self.alarm_enabled {
            "Alarm mode on"
        } else {
            "Alarm mode off"
        });
        self.dirty = true;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
