#![windows_subsystem = "windows"]

use eframe::egui;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use std::sync::mpsc::{Receiver, Sender};
use std::io::{BufRead, Read};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        window_builder: Some(Box::new(|builder| {
            builder
                .with_taskbar(false)
                .with_decorations(false)
                .with_always_on_top()
                .with_transparent(true)
                .with_inner_size(eframe::egui::vec2(1920.0, 26.0))
                .with_position(eframe::egui::pos2(0.0, 0.0))
        })),
        ..Default::default()
    };

    eframe::run_native(
        "rebar",
        options,
        Box::new(|_cc| {
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
            .exact_height(26.0)
            .show(ctx, |ui| {
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
        for ws in &self.state.workspaces {
            let color = match (ws.active, ws.focused) {
                (true, true) => egui::Color32::WHITE,
                (true, false) => egui::Color32::GRAY,
                (false, _) => egui::Color32::DARK_GRAY,
            };
            ui.colored_label(color, ws.icon.clone());
        }
    }

    fn draw_focused_title(&self, ui: &mut egui::Ui) {
        ui.label(self.state.focused_title.clone());
    }

    fn draw_time(&self, ui: &mut egui::Ui) {
        ui.label(self.state.time.clone());
    }

    fn draw_status_indicators(&self, ui: &mut egui::Ui) {
        ui.label(if self.state.horizontal { "[--]" } else { "[||]" });
        ui.separator();
        ui.label(if self.state.paused { "[PAUSED]" } else { "" });
    }
}

impl Rebar {
    fn update_time(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.state.last_tick) >= Duration::from_secs(60) {
            self.state.time = chrono::Local::now().format("%d/%m/%Y %H:%M").to_string();
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
            time: chrono::Local::now().format("%d/%m/%Y %H:%M").to_string(),
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
