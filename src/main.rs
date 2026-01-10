#![windows_subsystem = "windows"]

use eframe::egui;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::mpsc::{Receiver, Sender};
use std::io::{BufRead, Read};
use std::sync::OnceLock;
use std::path::{Path, PathBuf};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

mod fuzzydate;

// TODO
// get monitor size to set size of bar (x)

pub struct Config {
    fuzzy_time: bool,
    datetime_format: String,
    font: String,
    font_size: f32,
    bg_color: egui::Color32,
    title_color: egui::Color32,
    direction_color: egui::Color32,
    paused_color: egui::Color32,
    time_color: egui::Color32,
    focused_ws_color: egui::Color32,
    active_ws_color: egui::Color32,
    inactive_ws_color: egui::Color32,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

const BAR_HEIGHT: f32 = 26.0;
const BAR_PADDING_Y: f32 = 3.0;

fn get_config() -> &'static Config {
    CONFIG.get_or_init(|| {
        Config {
            fuzzy_time: true,
            datetime_format: "%m/%d/%Y %H:%M".to_string(),
            font: "Cascadia Code".to_string(),
            font_size: 14.0,
            bg_color: egui::Color32::from_rgb(15, 13, 14),
            title_color: egui::Color32::from_rgb(136, 218, 242),
            direction_color: egui::Color32::from_rgb(252, 186, 40),
            paused_color: egui::Color32::from_rgb(252, 186, 40),
            time_color: egui::Color32::from_rgb(252, 186, 40),
            focused_ws_color: egui::Color32::from_rgb(247, 18, 255),
            active_ws_color: egui::Color32::from_rgb(76, 67, 69),
            inactive_ws_color: egui::Color32::from_rgb(76, 67, 69),
        }
    })
}

fn clamp_font_size(size: f32) -> f32 {
    let max_size = (BAR_HEIGHT - (BAR_PADDING_Y * 2.0)).max(1.0);
    size.min(max_size).max(1.0)
}

fn resolve_font_path(font: &str) -> Option<PathBuf> {
    if font.trim().is_empty() {
        return None;
    }
    let direct = Path::new(font);
    if direct.is_file() {
        return Some(direct.to_path_buf());
    }
    #[cfg(windows)]
    {
        let base = PathBuf::from(r"C:\Windows\Fonts");
        let trimmed = font.trim();
        let compact = trimmed.replace(' ', "");
        let candidates = [
            format!("{trimmed}.ttf"),
            format!("{trimmed}.ttc"),
            format!("{trimmed}.otf"),
            format!("{compact}.ttf"),
            format!("{compact}.ttc"),
            format!("{compact}.otf"),
        ];
        for name in candidates {
            let candidate = base.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn apply_theme(ctx: &egui::Context) {
    let config = get_config();
    let font_size = clamp_font_size(config.font_size);

    let mut fonts = egui::FontDefinitions::default();
    if let Some(path) = resolve_font_path(&config.font) {
        if let Ok(data) = std::fs::read(&path) {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("custom-font")
                .to_string();
            fonts
                .font_data
                .insert(name.clone(), egui::FontData::from_owned(data).into());
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                family.insert(0, name.clone());
            }
            if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                family.insert(0, name.clone());
            }
        }
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    let text_size = font_size;
    let small_size = (font_size - 2.0).max(1.0);
    style.text_styles = [
        (
            egui::TextStyle::Heading,
            egui::FontId::new(text_size, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            egui::FontId::new(text_size, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            egui::FontId::new(text_size, egui::FontFamily::Monospace),
        ),
        (
            egui::TextStyle::Button,
            egui::FontId::new(text_size, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            egui::FontId::new(small_size, egui::FontFamily::Proportional),
        ),
    ]
    .into();
    style.spacing.item_spacing.y = 0.0;
    style.spacing.button_padding.y = 0.0;
    ctx.set_style(style);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = config.bg_color;
    visuals.window_fill = config.bg_color;
    visuals.faint_bg_color = config.bg_color;
    ctx.set_visuals(visuals);
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        window_builder: Some(Box::new(|builder| {
            builder
                .with_taskbar(false)
                .with_decorations(false)
                .with_always_on_top()
                .with_transparent(false)
                .with_inner_size(eframe::egui::vec2(1920.0, BAR_HEIGHT))
                .with_position(eframe::egui::pos2(0.0, 0.0))
        })),
        ..Default::default()
    };

    eframe::run_native(
        "rebar",
        options,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            
            let (tx, rx) = std::sync::mpsc::channel();
            spawn_ipc_thread(tx);
            Ok(Box::new(Rebar::new(rx)))
        }),
    )
}

impl Rebar {
    fn new(rx: Receiver<IPCEvent>) -> Self {
        Self {
            state: RebarState::default(),
            rx,
        }
    }
}

impl eframe::App for Rebar {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_ipc();
        self.update_time();
        self.draw_bar(ctx);
    }
}

// Drawing functions

impl Rebar {
    fn draw_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("rebar")
            .exact_height(BAR_HEIGHT)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.spacing_mut().button_padding.y = 0.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    self.draw_workspaces(ui);
                    ui.separator();
                    self.draw_focused_title(ui);
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            self.draw_time(ui);
                            ui.separator();
                            self.draw_status_indicators(ui);
                        },
                    )
                });
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

impl Rebar {
    fn draw_workspaces(&self, ui: &mut egui::Ui) {
        let config = get_config();
        for ws in &self.state.workspaces {
            let color = match (ws.active, ws.focused) {
                (true, true) => config.focused_ws_color,
                (true, false) => config.active_ws_color,
                (false, _) => config.inactive_ws_color,
            };
            ui.colored_label(color, ws.icon.clone());
        }
    }

    fn draw_focused_title(&self, ui: &mut egui::Ui) {
        let config = get_config();
        ui.colored_label(config.title_color, self.state.focused_title.clone());
    }

    fn draw_time(&self, ui: &mut egui::Ui) {
        let config = get_config();
        ui.colored_label(config.time_color, self.state.time.clone());
    }

    fn draw_status_indicators(&self, ui: &mut egui::Ui) {
        let config = get_config();
        ui.colored_label(config.direction_color, if self.state.horizontal { "H" } else { "V" });
        ui.separator();
        ui.colored_label(config.paused_color, if self.state.paused { "PAUSED" } else { "" });
    }
}

impl Rebar {
    fn update_time(&mut self) {
        let config = get_config();
        let now = Instant::now();
        if now.duration_since(self.state.last_tick) >= Duration::from_secs(60) {
            if config.fuzzy_time {
                self.state.time = fuzzydate::get_fuzzy_date();
            } else {
                self.state.time = chrono::Local::now().format(&config.datetime_format).to_string();
            }
            self.state.last_tick = now;
        }
    }
}

// State definitions

struct Rebar {
    state: RebarState,
    rx: Receiver<IPCEvent>,
}

struct RebarState {
    workspaces: Vec<Workspace>,
    focused_title: String,
    paused: bool,
    horizontal: bool,
    time: String,
    last_tick: Instant,
}

impl Default for RebarState {
    fn default() -> Self {
        let workspaces = (1..=9)
            .map(|id| Workspace {
                id,
                icon: format!(" {} ", id),
                active: false,
                focused: false,
            })
            .collect();
        Self {
            workspaces,
            focused_title: "GlazeWM - Rebar".to_string(),
            paused: false,
            horizontal: true,
            time: chrono::Local::now().format("%m/%d/%Y %H:%M").to_string(),
            last_tick: Instant::now(),
        }
    }
}

#[derive(Clone)]
struct Workspace {
    id: u8,
    icon: String,
    // windows: Vec<String>, <- maybe later
    active: bool,
    focused: bool,
}

fn update_workspace(state: &mut RebarState, id: u8, active: bool, focused: bool) {
    if id < 1 || id > 9 {
        return;
    }
    state.workspaces[id as usize - 1].active = active;
    state.workspaces[id as usize - 1].focused = focused;
}

fn set_focused_workspace(state: &mut RebarState, id: u8, focused: bool) {
    if id < 1 || id > 9 {
        return;
    }
    for ws in &mut state.workspaces {
        ws.focused = focused && ws.id == id;
    }
}

fn update_workspace_icons(state: &mut RebarState) {
    for ws in &mut state.workspaces {
        if ws.focused {
            ws.icon = format!("[{}]", ws.id);
        } else if ws.active {
            ws.icon = format!(" {}*", ws.id);
        } else {
            ws.icon = format!(" {} ", ws.id);
        }
    }
}

// IPC GlazeWM
enum IPCEvent {
    WorkspaceUpdate { id: u8, active: Option<bool>, focused: Option<bool> },
    FocusedTitle(String),
    Pause(bool),
    TilingDirection(bool),
}

impl Rebar {
    fn handle_ipc(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                IPCEvent::WorkspaceUpdate { id, active, focused } => {
                    if let Some(active) = active {
                        update_workspace(&mut self.state, id, active, focused.unwrap_or(false));
                    }
                    if let Some(focused) = focused {
                        set_focused_workspace(&mut self.state, id, focused);
                    }
                    update_workspace_icons(&mut self.state);
                }
                IPCEvent::FocusedTitle(title) => {
                    self.state.focused_title = title;
                }
                IPCEvent::Pause(paused) => {
                    self.state.paused = paused;
                }
                IPCEvent::TilingDirection(horizontal) => {
                    self.state.horizontal = horizontal;
                }
            }
        }
    }
}

fn spawn_ipc_thread(tx: Sender<IPCEvent>) {
    std::thread::spawn(move || {
        glazewm_event_loop(tx);
    });
}

fn glazewm_event_loop(tx: Sender<IPCEvent>) {
    let args = ["sub", "-e", "focus_changed", "focused_container_moved", "tiling_direction_changed",
        "workspace_activated", "workspace_deactivated", "pause_changed"];

    let mut workspace_ids: HashMap<String, u8> = HashMap::new();

    loop {
        query_workspaces(&tx, &mut workspace_ids);

        let mut cmd = std::process::Command::new("glazewm");
        cmd.args(&args)
            .stdout(std::process::Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = match cmd.spawn() { 
            Ok(child) => child,
            Err(_) => {
                std::thread::sleep(Duration::from_secs(5));
                continue;
            }
        };

        if let Some(stdout) = child.stdout.take() {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(_) => break,
                };
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                    continue;
                };
                for event in apply_event(value, &mut workspace_ids) {
                    if tx.send(event).is_err() {
                        return;
                    }
                }
            }
        }
    }
}

fn query_workspaces(tx: &Sender<IPCEvent>, workspace_ids: &mut HashMap<String, u8>) {
    let mut cmd = std::process::Command::new("glazewm");
    cmd.args(["query", "workspaces"])
        .stdout(std::process::Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(_) => return,
    };

    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut output);
    }
    let _ = child.wait();

    let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) else {
        return;
    };

    let workspaces = value.pointer("/data/workspaces").and_then(|val| val.as_array());
    let Some(workspaces) = workspaces else {
        return;
    };

    workspace_ids.clear();
    for workspace in workspaces {
        let id = workspace
            .pointer("/name")
            .and_then(|val| val.as_str())
            .and_then(|s| s.parse::<u8>().ok());
        let Some(id) = id else {
            continue;
        };
        if let Some(workspace_id) = workspace.pointer("/id").and_then(|val| val.as_str()) {
            workspace_ids.insert(workspace_id.to_string(), id);
        }
        let active = workspace
            .pointer("/isDisplayed")
            .and_then(|val| val.as_bool())
            .unwrap_or(false);
        let focused = workspace
            .pointer("/hasFocus")
            .and_then(|val| val.as_bool())
            .unwrap_or(false);
        let event = IPCEvent::WorkspaceUpdate {
            id,
            active: Some(active),
            focused: if focused { Some(true) } else { None },
        };
        if tx.send(event).is_err() {
            return;
        }
    }
}

fn apply_event(value: serde_json::Value, workspace_ids: &mut HashMap<String, u8>) -> Vec<IPCEvent> {
    let mut events = Vec::new();
    let event_type = value.pointer("/data/eventType").and_then(|val| val.as_str());
    let Some(event_type) = event_type else {
        return events;
    };
    match event_type {
        "focus_changed" => {
            let focused = value.pointer("/data/focusedContainer");
            let focused_type = focused
                .and_then(|val| val.get("type"))
                .and_then(|val| val.as_str())
                .unwrap_or("");
            match focused_type {
                "workspace" => {
                    let id = focused
                        .and_then(|val| val.get("name"))
                        .and_then(|val| val.as_str())
                        .and_then(|s| s.parse::<u8>().ok());
                    if let Some(id) = id {
                        if let Some(workspace_id) = focused
                            .and_then(|val| val.get("id"))
                            .and_then(|val| val.as_str())
                        {
                            workspace_ids.insert(workspace_id.to_string(), id);
                        }
                        events.push(IPCEvent::WorkspaceUpdate {
                            id,
                            active: Some(true),
                            focused: Some(true),
                        });
                    }
                }
                "window" => {
                    let title = focused
                        .and_then(|val| val.get("title"))
                        .and_then(|val| val.as_str())
                        .unwrap_or("")
                        .to_string();
                    events.push(IPCEvent::FocusedTitle(title));
                    if let Some(parent_id) = focused
                        .and_then(|val| val.get("parentId"))
                        .and_then(|val| val.as_str())
                    {
                        if let Some(&id) = workspace_ids.get(parent_id) {
                            events.push(IPCEvent::WorkspaceUpdate {
                                id,
                                active: Some(true),
                                focused: Some(true),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        "focused_container_moved" => {
            let focused = value.pointer("/data/focusedContainer");
            if focused
                .and_then(|val| val.get("type"))
                .and_then(|val| val.as_str())
                == Some("window")
            {
                if let Some(parent_id) = focused
                    .and_then(|val| val.get("parentId"))
                    .and_then(|val| val.as_str())
                {
                    if let Some(&id) = workspace_ids.get(parent_id) {
                        events.push(IPCEvent::WorkspaceUpdate {
                            id,
                            active: Some(true),
                            focused: Some(true),
                        });
                    }
                }
            }
        }
        "tiling_direction_changed" => {
            let horizontal = value.pointer("/data/newTilingDirection").and_then(|val| val.as_str()).unwrap_or("horizontal");
            events.push(IPCEvent::TilingDirection(horizontal == "horizontal"));
        }
        "workspace_activated" => {
            let id = value
                .pointer("/data/activatedWorkspace/name")
                .and_then(|val| val.as_str())
                .and_then(|s| s.parse::<u8>().ok());
            if let Some(id) = id {
                if let Some(workspace_id) = value
                    .pointer("/data/activatedWorkspace/id")
                    .and_then(|val| val.as_str())
                {
                    workspace_ids.insert(workspace_id.to_string(), id);
                }
                let focused = value
                    .pointer("/data/activatedWorkspace/hasFocus")
                    .and_then(|val| val.as_bool())
                    .unwrap_or(false);
                events.push(IPCEvent::WorkspaceUpdate {
                    id,
                    active: Some(true),
                    focused: if focused { Some(true) } else { None },
                });
            }
        }
        "workspace_deactivated" => {
            if let Some(workspace_id) = value
                .pointer("/data/deactivatedId")
                .and_then(|val| val.as_str())
            {
                workspace_ids.remove(workspace_id);
            }
            let id = value
                .pointer("/data/deactivatedName")
                .and_then(|val| val.as_str())
                .and_then(|s| s.parse::<u8>().ok());
            if let Some(id) = id {
                events.push(IPCEvent::WorkspaceUpdate {
                    id,
                    active: Some(false),
                    focused: Some(false),
                });
            }
        }
        "pause_changed" => {
            let paused = value.pointer("/data/isPaused").and_then(|val| val.as_bool()).unwrap_or(false);
            events.push(IPCEvent::Pause(paused));
        }
        _ => {}
    }
    events
}
