use anyhow::{Context as _, Result};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{self, Align, Align2, Color32, FontId, Layout, RichText, Sense, Stroke};
use std::{
    env, ptr, thread,
    time::{Duration, Instant},
};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon as TrayImage, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
};

use crate::{
    config::{self, SavedState},
    driver,
    engine::{ControllerEngine, EngineCommand},
    mapper,
    model::{ControllerProfile, InputBinding, OutputControl, RawState, RuntimeSnapshot},
    single_instance::BuildKind,
    startup, updater,
    wide::str_wide_null,
};

const DEFAULT_WINDOW_WIDTH: f32 = 1040.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 760.0;
const MIN_WINDOW_WIDTH: f32 = 820.0;
const MIN_WINDOW_HEIGHT: f32 = 620.0;
const VISIBLE_POLL: Duration = Duration::from_millis(16);
const HIDDEN_POLL: Duration = Duration::from_millis(120);
const AUTO_UPDATE_INTERVAL_SECONDS: u64 = 60 * 60;

const BACKGROUND: Color32 = Color32::from_rgb(242, 244, 247);
const SURFACE: Color32 = Color32::from_rgb(255, 255, 255);
const SURFACE_MUTED: Color32 = Color32::from_rgb(248, 249, 251);
const BORDER: Color32 = Color32::from_rgb(218, 222, 229);
const TEXT: Color32 = Color32::from_rgb(28, 31, 38);
const MUTED: Color32 = Color32::from_rgb(102, 108, 120);
const ACCENT: Color32 = Color32::from_rgb(52, 111, 224);
const ACCENT_SOFT: Color32 = Color32::from_rgb(224, 234, 255);
const SUCCESS: Color32 = Color32::from_rgb(37, 145, 83);
const WARNING: Color32 = Color32::from_rgb(205, 125, 21);
const DANGER: Color32 = Color32::from_rgb(199, 58, 58);
const CONTROLLER: Color32 = Color32::from_rgb(43, 47, 56);

const MENU_OPEN: &str = "open";
const MENU_ENABLED: &str = "enabled";
const MENU_AUTO_TRAY: &str = "auto-tray";
const MENU_STARTUP: &str = "startup";
const MENU_CHECK: &str = "check";
const MENU_INSTALL: &str = "install";
const MENU_QUIT: &str = "quit";

pub fn started_from_startup() -> bool {
    env::args_os().any(|argument| argument == "--startup")
}

pub fn activate_existing_instance(existing_build: BuildKind) {
    let title = existing_build
        .window_title()
        .unwrap_or_else(config::window_title);
    let title = str_wide_null(&title);

    for _ in 0..20 {
        let hwnd = unsafe { FindWindowW(ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            unsafe {
                ShowWindow(
                    hwnd,
                    if IsIconic(hwnd) != 0 {
                        SW_RESTORE
                    } else {
                        SW_SHOW
                    },
                );
                SetForegroundWindow(hwnd);
            }
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn run() -> Result<()> {
    let icon = load_window_icon(include_bytes!("../assets/x360ce.png"))?;
    let state = config::load_state();
    let visible = config::is_dev_build()
        || (!started_from_startup() && !state.start_in_tray);
    let viewport = egui::ViewportBuilder::default()
        .with_title(config::window_title())
        .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
        .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT])
        .with_resizable(true)
        .with_visible(visible)
        .with_icon(icon);

    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        config::app_name(),
        options,
        Box::new(move |creation_context| {
            Ok(Box::new(X360ceApp::new(
                creation_context,
                state,
                visible,
            )))
        }),
    )
    .map_err(|error| anyhow::anyhow!("failed to run x360ce UI: {error}"))
}

struct TrayState {
    icon: TrayIcon,
    enabled: CheckMenuItem,
    auto_tray: CheckMenuItem,
    startup: CheckMenuItem,
    install: MenuItem,
}

impl TrayState {
    fn new(state: &SavedState, update_installable: bool) -> Result<Self> {
        let menu = Menu::new();
        let open = MenuItem::with_id(MENU_OPEN, "Open x360ce", true, None);
        let enabled = CheckMenuItem::with_id(
            MENU_ENABLED,
            "Virtual controller enabled",
            true,
            state.emulation_enabled,
            None,
        );
        let auto_tray = CheckMenuItem::with_id(
            MENU_AUTO_TRAY,
            "Auto tray after inactivity",
            true,
            state.auto_tray_enabled,
            None,
        );
        let startup = CheckMenuItem::with_id(
            MENU_STARTUP,
            "Start with Windows",
            !config::is_dev_build(),
            state.startup_enabled && !config::is_dev_build(),
            None,
        );
        let check = MenuItem::with_id(
            MENU_CHECK,
            "Check for updates",
            !config::is_dev_build(),
            None,
        );
        let install = MenuItem::with_id(
            MENU_INSTALL,
            "Install available update",
            update_installable && !config::is_dev_build(),
            None,
        );
        let quit = MenuItem::with_id(MENU_QUIT, "Quit x360ce", true, None);
        let separator = PredefinedMenuItem::separator();
        let separator_updates = PredefinedMenuItem::separator();
        let separator_quit = PredefinedMenuItem::separator();
        menu.append_items(&[
            &open,
            &separator,
            &enabled,
            &auto_tray,
            &startup,
            &separator_updates,
            &check,
            &install,
            &separator_quit,
            &quit,
        ])?;

        let icon = TrayIconBuilder::new()
            .with_id("x360ce")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .with_icon(load_tray_image(include_bytes!("../assets/x360ce-tray.png"))?)
            .with_tooltip(tray_tooltip(state, false))
            .build()?;

        Ok(Self {
            icon,
            enabled,
            auto_tray,
            startup,
            install,
        })
    }

    fn sync(&self, state: &SavedState, virtual_connected: bool, update_installable: bool) {
        self.enabled.set_checked(state.emulation_enabled);
        self.auto_tray.set_checked(state.auto_tray_enabled);
        self.startup
            .set_checked(state.startup_enabled && !config::is_dev_build());
        self.install
            .set_enabled(update_installable && !config::is_dev_build());
        let _ = self
            .icon
            .set_tooltip(Some(tray_tooltip(state, virtual_connected)));
    }
}

struct X360ceApp {
    state: SavedState,
    engine: ControllerEngine,
    runtime: RuntimeSnapshot,
    tray: Option<TrayState>,
    status: String,
    selected_control: OutputControl,
    learning_control: Option<OutputControl>,
    learn_baseline: RawState,
    window_visible: bool,
    quit_requested: bool,
    last_activity: Instant,
    last_controller_sequence: u64,
    countdown_started: Option<Instant>,
    update_rx: Option<Receiver<anyhow::Result<updater::UpdateCheck>>>,
    last_update_check: Option<updater::UpdateCheck>,
}

impl X360ceApp {
    fn new(
        creation_context: &eframe::CreationContext<'_>,
        mut state: SavedState,
        window_visible: bool,
    ) -> Self {
        configure_egui(&creation_context.egui_ctx);

        if let Ok(enabled) = startup::is_enabled() {
            state.startup_enabled = enabled;
        }

        let initial_profile = if state.selected_device_guid.is_empty() {
            ControllerProfile::default_for(String::new())
        } else {
            state
                .profiles
                .get(&state.selected_device_guid)
                .cloned()
                .unwrap_or_else(|| {
                    ControllerProfile::default_for(state.selected_device_guid.clone())
                })
        };
        let engine = ControllerEngine::start(
            state.selected_device_guid.clone(),
            initial_profile,
            state.emulation_enabled,
        );
        let status = if state.last_status.is_empty() {
            "Ready.".to_owned()
        } else {
            state.last_status.clone()
        };
        let tray = TrayState::new(&state, false).ok();

        Self {
            state,
            engine,
            runtime: RuntimeSnapshot::default(),
            tray,
            status,
            selected_control: OutputControl::A,
            learning_control: None,
            learn_baseline: RawState::default(),
            window_visible,
            quit_requested: false,
            last_activity: Instant::now(),
            last_controller_sequence: 0,
            countdown_started: None,
            update_rx: None,
            last_update_check: None,
        }
    }

    fn current_profile(&self) -> ControllerProfile {
        let guid = self.state.selected_device_guid.clone();
        self.state
            .profiles
            .get(&guid)
            .cloned()
            .unwrap_or_else(|| ControllerProfile::default_for(guid))
    }

    fn current_profile_mut(&mut self) -> &mut ControllerProfile {
        let guid = self.state.selected_device_guid.clone();
        self.state
            .profiles
            .entry(guid.clone())
            .or_insert_with(|| ControllerProfile::default_for(guid))
    }

    fn push_profile(&self) {
        self.engine
            .send(EngineCommand::SetProfile(self.current_profile()));
    }

    fn save(&mut self) {
        self.state.last_status = self.status.clone();
        if let Err(error) = config::save_state(&self.state) {
            self.status = format!("Save failed: {error}");
        }
    }

    fn sync_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.sync(
                &self.state,
                self.runtime.virtual_connected,
                self.last_update_check
                    .as_ref()
                    .map(|check| check.is_update_available && check.asset_download_url.is_some())
                    .unwrap_or(false),
            );
        }
    }

    fn select_device(&mut self, guid: String) {
        if self.state.selected_device_guid == guid {
            return;
        }
        self.state.selected_device_guid = guid.clone();
        self.current_profile_mut();
        self.engine.send(EngineCommand::SelectDevice(guid));
        self.push_profile();
        self.status = "Controller selected.".to_owned();
        self.mark_activity();
        self.save();
    }

    fn set_emulation_enabled(&mut self, enabled: bool) {
        self.state.emulation_enabled = enabled;
        self.engine.send(EngineCommand::SetEnabled(enabled));
        self.status = if enabled {
            "Virtual controller enabled.".to_owned()
        } else {
            "Virtual controller paused.".to_owned()
        };
        self.mark_activity();
        self.save();
        self.sync_tray();
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.window_visible = true;
        self.countdown_started = None;
        self.last_activity = Instant::now();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.window_visible = false;
        self.countdown_started = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn mark_activity(&mut self) {
        self.last_activity = Instant::now();
        self.countdown_started = None;
    }

    fn poll_activity(&mut self, ctx: &egui::Context) {
        let ui_active = ctx.input(|input| {
            !input.events.is_empty()
                || input.pointer.any_down()
                || input.pointer.delta() != egui::Vec2::ZERO
        });
        if ui_active {
            self.mark_activity();
        }

        if self.runtime.last_controller_activity_sequence != self.last_controller_sequence {
            self.last_controller_sequence = self.runtime.last_controller_activity_sequence;
            self.mark_activity();
        }
    }

    fn poll_auto_tray(&mut self, ctx: &egui::Context) {
        if !self.window_visible || !self.state.auto_tray_enabled {
            self.countdown_started = None;
            return;
        }

        let idle = Duration::from_secs(self.state.auto_tray_idle_seconds.max(1));
        if self.last_activity.elapsed() < idle {
            self.countdown_started = None;
            return;
        }

        let started = self.countdown_started.get_or_insert_with(Instant::now);
        let countdown = Duration::from_secs(self.state.auto_tray_countdown_seconds.max(1));
        if started.elapsed() >= countdown {
            self.hide_window(ctx);
            self.status = "Moved to tray after inactivity.".to_owned();
            self.save();
        }
    }

    fn countdown_remaining(&self) -> Option<u64> {
        let started = self.countdown_started?;
        let total = self.state.auto_tray_countdown_seconds.max(1);
        Some(total.saturating_sub(started.elapsed().as_secs()).max(1))
    }

    fn poll_learning(&mut self) {
        let Some(control) = self.learning_control else {
            return;
        };
        if let Some(binding) = mapper::detect_binding(&self.learn_baseline, &self.runtime.raw_state) {
            let binding = match binding {
                InputBinding::AxisNegative { index } if control.is_axis() => {
                    InputBinding::AxisPositive { index }
                }
                binding => binding,
            };
            let centered_axis = if control.is_trigger() {
                match binding {
                    InputBinding::AxisPositive { index } | InputBinding::AxisNegative { index } => {
                        self.learn_baseline
                            .axes
                            .get(index as usize)
                            .copied()
                            .unwrap_or_default()
                            .unsigned_abs()
                            < 8_000
                    }
                    _ => false,
                }
            } else {
                false
            };
            let mapping = self.current_profile_mut().entry_mut(control);
            mapping.binding = binding;
            mapping.centered_axis = centered_axis;
            self.learning_control = None;
            self.status = format!("{} mapped to {}.", control.label(), binding.label());
            self.push_profile();
            self.save();
        }
    }

    fn begin_learning(&mut self, control: OutputControl) {
        self.selected_control = control;
        self.learning_control = Some(control);
        self.learn_baseline = self.runtime.raw_state.clone();
        self.status = format!("Move axis or press input for {}.", control.label());
        self.mark_activity();
    }

    fn clear_mapping(&mut self, control: OutputControl) {
        self.current_profile_mut().entry_mut(control).binding = InputBinding::None;
        self.learning_control = None;
        self.status = format!("{} mapping cleared.", control.label());
        self.push_profile();
        self.save();
    }

    fn poll_tray(&mut self, ctx: &egui::Context) {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => self.show_window(ctx),
                _ => {}
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            match event.id.as_ref() {
                MENU_OPEN => self.show_window(ctx),
                MENU_ENABLED => self.set_emulation_enabled(!self.state.emulation_enabled),
                MENU_AUTO_TRAY => {
                    self.state.auto_tray_enabled = !self.state.auto_tray_enabled;
                    self.mark_activity();
                    self.save();
                    self.sync_tray();
                }
                MENU_STARTUP => self.toggle_startup(),
                MENU_CHECK => self.start_update_check(),
                MENU_INSTALL => self.install_update(),
                MENU_QUIT => {
                    self.quit_requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }
    }

    fn toggle_startup(&mut self) {
        if config::is_dev_build() {
            self.status = "Startup disabled in development builds.".to_owned();
            return;
        }
        let enabled = !self.state.startup_enabled;
        match startup::set_enabled(enabled) {
            Ok(()) => {
                self.state.startup_enabled = enabled;
                self.status = if enabled {
                    "Windows startup enabled.".to_owned()
                } else {
                    "Windows startup disabled.".to_owned()
                };
                self.save();
                self.sync_tray();
            }
            Err(error) => self.status = format!("Startup update failed: {error}"),
        }
    }

    fn maybe_start_automatic_update_check(&mut self) {
        if config::is_dev_build() || self.update_rx.is_some() {
            return;
        }
        let now = config::seconds_since_unix_epoch();
        if now.saturating_sub(self.state.last_auto_update_check_unix_seconds)
            >= AUTO_UPDATE_INTERVAL_SECONDS
        {
            self.start_update_check();
        }
    }

    fn start_update_check(&mut self) {
        if config::is_dev_build() || self.update_rx.is_some() {
            return;
        }
        let include_prereleases = self.state.include_prereleases;
        let (sender, receiver) = unbounded();
        thread::spawn(move || {
            let result = updater::check_for_update(include_prereleases);
            let _ = sender.send(result);
        });
        self.update_rx = Some(receiver);
        self.status = "Checking GitHub releases…".to_owned();
    }

    fn poll_update_check(&mut self) {
        let result = self
            .update_rx
            .as_ref()
            .and_then(|receiver| receiver.try_recv().ok());
        let Some(result) = result else {
            return;
        };
        self.update_rx = None;
        self.state.last_auto_update_check_unix_seconds = config::seconds_since_unix_epoch();
        match result {
            Ok(check) => {
                self.status = update_status(&check);
                self.last_update_check = Some(check);
            }
            Err(error) => self.status = format!("Update check failed: {error}"),
        }
        self.save();
        self.sync_tray();
    }

    fn install_update(&mut self) {
        let Some(check) = self.last_update_check.clone() else {
            self.status = "No checked update available.".to_owned();
            return;
        };
        if let Err(error) = updater::install_update(&check) {
            self.status = format!("Update install failed: {error}");
        }
    }

    fn poll_runtime(&mut self) {
        self.runtime = self.engine.snapshot();
        if self.state.selected_device_guid.is_empty() {
            let first_guid = self.runtime.devices.first().map(|device| device.guid.clone());
            if let Some(guid) = first_guid {
                self.select_device(guid);
            }
        }
    }

    fn render_countdown(&mut self, ctx: &egui::Context) {
        let Some(remaining) = self.countdown_remaining() else {
            return;
        };
        egui::Window::new("Moving to tray")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_width(320.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(remaining.to_string())
                            .size(48.0)
                            .strong()
                            .color(ACCENT),
                    );
                    ui.label("No activity. x360ce keeps forwarding in tray.");
                    ui.add_space(8.0);
                    if ui.button("Stay open").clicked() {
                        self.mark_activity();
                    }
                });
            });
    }
}

impl eframe::App for X360ceApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested()) && !self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(ctx);
        }

        self.poll_runtime();
        self.poll_tray(ctx);
        self.poll_activity(ctx);
        self.poll_learning();
        self.poll_auto_tray(ctx);
        self.maybe_start_automatic_update_check();
        self.poll_update_check();
        self.sync_tray();

        ctx.request_repaint_after(if self.window_visible {
            VISIBLE_POLL
        } else {
            HIDDEN_POLL
        });
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        ui_root(self, ui, frame);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        BACKGROUND.to_normalized_gamma_f32()
    }
}

impl X360ceApp {
    fn render_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            draw_embedded_logo(ui);
            ui.vertical(|ui| {
                ui.label(RichText::new("x360ce").size(26.0).strong().color(TEXT));
                ui.label(
                    RichText::new("controller mapper and virtual xbox 360 output")
                        .size(12.0)
                        .color(MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("tray").clicked() {
                    self.hide_window(ui.ctx());
                }
                ui.label(RichText::new(config::app_version_label()).color(MUTED));
            });
        });
    }

    fn render_status(&mut self, ui: &mut egui::Ui) {
        let (label, color) = if !self.runtime.driver_installed {
            ("Driver missing", WARNING)
        } else if self.runtime.virtual_connected {
            ("Forwarding", SUCCESS)
        } else if !self.state.emulation_enabled {
            ("Paused", MUTED)
        } else {
            ("Waiting", WARNING)
        };

        let runtime_error = self.runtime.last_error.clone();
        surface(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                status_chip(ui, label, color, true);
                status_chip(
                    ui,
                    if self.state.emulation_enabled {
                        "virtual on"
                    } else {
                        "virtual off"
                    },
                    if self.state.emulation_enabled {
                        ACCENT_SOFT
                    } else {
                        SURFACE_MUTED
                    },
                    false,
                );
                status_chip(
                    ui,
                    &format!("{} detected", self.runtime.devices.len()),
                    SURFACE_MUTED,
                    false,
                );
            });
            ui.add_space(8.0);
            ui.label(RichText::new(&self.status).color(MUTED));
            if let Some(error) = &runtime_error {
                ui.add_space(6.0);
                ui.label(RichText::new(error).color(DANGER));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui
                    .button(if self.state.emulation_enabled {
                        "Disable output"
                    } else {
                        "Enable output"
                    })
                    .clicked()
                {
                    self.set_emulation_enabled(!self.state.emulation_enabled);
                }
                if ui.button("Refresh controllers").clicked() {
                    self.engine.send(EngineCommand::Refresh);
                    self.status = "Refreshing controllers…".to_owned();
                    self.mark_activity();
                }
                if !self.runtime.driver_installed && ui.button("Install ViGEmBus").clicked() {
                    match driver::launch_installer_elevated() {
                        Ok(_) => {
                            self.status =
                                "Driver installer opened. Restart x360ce after install.".to_owned()
                        }
                        Err(error) => self.status = format!("Driver installer failed: {error}"),
                    }
                }
            });
        });
    }
    fn render_device_bar(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Input controller").strong());
                let selected_name = self
                    .runtime
                    .devices
                    .iter()
                    .find(|device| device.guid == self.state.selected_device_guid)
                    .map(|device| device.name.clone())
                    .unwrap_or_else(|| "No controller detected".to_owned());
                egui::ComboBox::from_id_salt("controller-selector")
                    .selected_text(selected_name)
                    .width(360.0)
                    .show_ui(ui, |ui| {
                        let devices = self.runtime.devices.clone();
                        for device in devices {
                            let label = format!(
                                "{}  ·  {} axes / {} buttons / {} hats",
                                device.name, device.axes, device.buttons, device.hats
                            );
                            if ui
                                .selectable_label(
                                    self.state.selected_device_guid == device.guid,
                                    label,
                                )
                                .clicked()
                            {
                                self.select_device(device.guid);
                            }
                        }
                    });
                if ui.button("Refresh").clicked() {
                    self.engine.send(EngineCommand::Refresh);
                    self.status = "Refreshing controllers…".to_owned();
                    self.mark_activity();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(instance_id) = self.runtime.selected_instance_id {
                        ui.label(format!("ID {instance_id}"));
                        ui.separator();
                    }
                    ui.label(format!("{} detected", self.runtime.devices.len()));
                });
            });
        });
    }

    fn render_controller(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.label(RichText::new("Virtual Xbox 360 layout").strong());
            ui.label(RichText::new("Click control, then map physical input.").color(MUTED));
            ui.add_space(8.0);
            let desired = egui::vec2(ui.available_width(), 360.0);
            let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
            draw_controller_body(ui, rect);

            let profile = self.current_profile();
            for (control, x, y) in controller_points() {
                let center = egui::pos2(
                    rect.left() + rect.width() * x,
                    rect.top() + rect.height() * y,
                );
                let radius = if control.is_axis() { 23.0 } else { 16.0 };
                let hit = egui::Rect::from_center_size(
                    center,
                    egui::vec2(radius * 2.0, radius * 2.0),
                );
                let response = ui.interact(
                    hit,
                    ui.make_persistent_id(("controller-control", control)),
                    Sense::click(),
                );
                if response.clicked() {
                    self.selected_control = control;
                    self.mark_activity();
                }

                let entry = profile.entry(control);
                let active = entry
                    .map(|entry| {
                        if control.is_trigger() {
                            mapper::trigger_preview(entry, &self.runtime.raw_state) > 0.15
                        } else if control.is_axis() {
                            mapper::axis_preview(entry, &self.runtime.raw_state).abs() > 0.15
                        } else {
                            mapper::binding_pressed(entry, &self.runtime.raw_state)
                        }
                    })
                    .unwrap_or(false);
                let selected = self.selected_control == control;
                let color = if active {
                    SUCCESS
                } else if selected {
                    ACCENT
                } else {
                    SURFACE
                };
                ui.painter().circle_filled(center, radius, color);
                ui.painter().circle_stroke(
                    center,
                    radius,
                    Stroke::new(if selected { 3.0 } else { 1.0 }, BORDER),
                );
                ui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    short_control_label(control),
                    FontId::proportional(if control.is_axis() { 9.0 } else { 11.0 }),
                    if active || selected { Color32::WHITE } else { TEXT },
                );
                response.on_hover_text(control.label());
            }
        });
    }

    fn render_mapping_editor(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            let control = self.selected_control;
            let profile = self.current_profile();
            ui.label(RichText::new(control.label()).size(18.0).strong());
            ui.label(RichText::new(entry_label_for(&profile, control)).color(MUTED));
            ui.add_space(10.0);

            let learning = self.learning_control == Some(control);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!learning, egui::Button::new("Learn"))
                    .clicked()
                {
                    self.begin_learning(control);
                }
                if learning {
                    ui.label(RichText::new("waiting for input…").color(ACCENT));
                    if ui.button("Cancel").clicked() {
                        self.learning_control = None;
                        self.status = "Mapping capture canceled.".to_owned();
                    }
                }
                if ui.button("Clear").clicked() {
                    self.clear_mapping(control);
                }
            });

            if control.is_axis() || control.is_trigger() {
                ui.add_space(12.0);
                let mut changed = false;
                {
                    let mapping = self.current_profile_mut().entry_mut(control);
                    changed |= ui.checkbox(&mut mapping.invert, "Invert").changed();
                    if control.is_trigger() {
                        changed |= ui
                            .checkbox(&mut mapping.centered_axis, "Split axis")
                            .on_hover_text(
                                "Enable when both triggers share one centered axis.",
                            )
                            .changed();
                    }
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut mapping.deadzone, 0.0..=0.5)
                                .text("Deadzone")
                                .fixed_decimals(2),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::Slider::new(&mut mapping.saturation, 0.5..=1.0)
                                .text("Max")
                                .fixed_decimals(2),
                        )
                        .changed();
                }
                if changed {
                    self.push_profile();
                    self.save();
                    self.mark_activity();
                }

                let value = self
                    .current_profile()
                    .entry(control)
                    .map(|entry| {
                        if control.is_trigger() {
                            mapper::trigger_preview(entry, &self.runtime.raw_state)
                        } else {
                            mapper::axis_preview(entry, &self.runtime.raw_state)
                        }
                    })
                    .unwrap_or_default();
                let progress = if control.is_trigger() {
                    value.clamp(0.0, 1.0)
                } else {
                    ((value + 1.0) * 0.5).clamp(0.0, 1.0)
                };
                ui.add_space(8.0);
                ui.add(egui::ProgressBar::new(progress).show_percentage());
            }

            ui.add_space(12.0);
            egui::CollapsingHeader::new("all mappings")
                .default_open(true)
                .show(ui, |ui| {
                    let profile = self.current_profile();
                    for control in OutputControl::BUTTONS
                        .into_iter()
                        .chain(OutputControl::ANALOGS)
                    {
                        if ui
                            .selectable_label(
                                self.selected_control == control,
                                format!(
                                    "{}  ·  {}",
                                    control.label(),
                                    entry_label_for(&profile, control)
                                ),
                            )
                            .clicked()
                        {
                            self.selected_control = control;
                            self.mark_activity();
                        }
                    }
                });
        });
    }
    fn render_raw_inputs(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.label(RichText::new("Live physical input").strong());
            ui.horizontal_wrapped(|ui| {
                for (index, value) in self.runtime.raw_state.axes.iter().enumerate() {
                    ui.label(format!("A{}: {:.2}", index + 1, normalize_axis(*value)));
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (index, pressed) in self.runtime.raw_state.buttons.iter().enumerate() {
                    let text = format!("B{}", index + 1);
                    ui.label(
                        RichText::new(text).color(if *pressed { SUCCESS } else { MUTED }),
                    );
                }
                for (index, hat) in self.runtime.raw_state.hats.iter().enumerate() {
                    ui.label(format!("H{}: {}", index + 1, hat.label()));
                }
            });
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.label(RichText::new("Settings").strong());

            let mut changed = false;
            changed |= ui
                .checkbox(&mut self.state.auto_tray_enabled, "Auto tray after inactivity")
                .changed();
            changed |= ui
                .checkbox(&mut self.state.start_in_tray, "Start in tray")
                .changed();
            if changed {
                self.mark_activity();
                self.save();
                self.sync_tray();
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Idle").color(MUTED));
                if ui
                    .add(egui::Slider::new(
                        &mut self.state.auto_tray_idle_seconds,
                        10..=600,
                    ))
                    .changed()
                {
                    self.mark_activity();
                    self.save();
                }
                ui.label(RichText::new("Countdown").color(MUTED));
                if ui
                    .add(egui::Slider::new(
                        &mut self.state.auto_tray_countdown_seconds,
                        3..=30,
                    ))
                    .changed()
                {
                    self.mark_activity();
                    self.save();
                }
            });

            ui.add_space(8.0);
            if ui
                .add_enabled(
                    !config::is_dev_build(),
                    egui::Button::new(if self.state.startup_enabled {
                        "Disable Windows startup"
                    } else {
                        "Enable Windows startup"
                    }),
                )
                .clicked()
            {
                self.toggle_startup();
            }

            ui.add_space(8.0);
            egui::CollapsingHeader::new("updates")
                .default_open(false)
                .show(ui, |ui| {
                    if ui
                        .checkbox(&mut self.state.include_prereleases, "Include prereleases")
                        .changed()
                    {
                        self.save();
                        self.sync_tray();
                    }
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !config::is_dev_build() && self.update_rx.is_none(),
                                egui::Button::new("Check now"),
                            )
                            .clicked()
                        {
                            self.save();
                            self.start_update_check();
                        }
                        if ui
                            .add_enabled(
                                self.last_update_check
                                    .as_ref()
                                    .map(|check| {
                                        check.is_update_available
                                            && check.asset_download_url.is_some()
                                    })
                                    .unwrap_or(false),
                                egui::Button::new("Install update"),
                            )
                            .clicked()
                        {
                            self.install_update();
                        }
                    });
                });

            egui::CollapsingHeader::new("portable data")
                .default_open(false)
                .show(ui, |ui| match config::state_path() {
                    Ok(path) => {
                        ui.label(
                            RichText::new(path.display().to_string())
                                .size(10.0)
                                .color(MUTED),
                        );
                        ui.label(
                            RichText::new(
                                "profiles and settings live beside executable in .x360ce-data",
                            )
                            .size(11.0)
                            .color(MUTED),
                        );
                    }
                    Err(error) => {
                        ui.label(RichText::new(error.to_string()).color(DANGER));
                    }
                });
        });
    }
}
fn surface(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(14))
        .show(ui, add_contents);
}

fn draw_controller_body(ui: &egui::Ui, rect: egui::Rect) {
    let painter = ui.painter();
    let body = rect.shrink2(egui::vec2(rect.width() * 0.08, rect.height() * 0.14));
    painter.rect_filled(body, 72, CONTROLLER);
    painter.circle_filled(
        egui::pos2(body.left() + body.width() * 0.16, body.bottom() - 12.0),
        72.0,
        CONTROLLER,
    );
    painter.circle_filled(
        egui::pos2(body.right() - body.width() * 0.16, body.bottom() - 12.0),
        72.0,
        CONTROLLER,
    );
    painter.rect_filled(
        egui::Rect::from_center_size(
            egui::pos2(body.center().x, body.top() + 32.0),
            egui::vec2(body.width() * 0.34, 18.0),
        ),
        9,
        Color32::from_rgb(72, 77, 88),
    );
}

fn controller_points() -> [(OutputControl, f32, f32); 21] {
    [
        (OutputControl::LeftShoulder, 0.28, 0.16),
        (OutputControl::RightShoulder, 0.72, 0.16),
        (OutputControl::LeftTrigger, 0.18, 0.10),
        (OutputControl::RightTrigger, 0.82, 0.10),
        (OutputControl::DpadUp, 0.27, 0.48),
        (OutputControl::DpadRight, 0.32, 0.55),
        (OutputControl::DpadDown, 0.27, 0.62),
        (OutputControl::DpadLeft, 0.22, 0.55),
        (OutputControl::Y, 0.77, 0.39),
        (OutputControl::B, 0.82, 0.47),
        (OutputControl::A, 0.77, 0.55),
        (OutputControl::X, 0.72, 0.47),
        (OutputControl::Back, 0.44, 0.45),
        (OutputControl::Guide, 0.50, 0.39),
        (OutputControl::Start, 0.56, 0.45),
        (OutputControl::LeftStickX, 0.35, 0.72),
        (OutputControl::LeftStickY, 0.41, 0.72),
        (OutputControl::RightStickX, 0.59, 0.72),
        (OutputControl::RightStickY, 0.65, 0.72),
        (OutputControl::LeftThumb, 0.38, 0.81),
        (OutputControl::RightThumb, 0.62, 0.81),
    ]
}

fn short_control_label(control: OutputControl) -> &'static str {
    match control {
        OutputControl::A => "A",
        OutputControl::B => "B",
        OutputControl::X => "X",
        OutputControl::Y => "Y",
        OutputControl::LeftShoulder => "LB",
        OutputControl::RightShoulder => "RB",
        OutputControl::Back => "◀",
        OutputControl::Start => "▶",
        OutputControl::Guide => "X",
        OutputControl::LeftThumb => "L3",
        OutputControl::RightThumb => "R3",
        OutputControl::DpadUp => "↑",
        OutputControl::DpadRight => "→",
        OutputControl::DpadDown => "↓",
        OutputControl::DpadLeft => "←",
        OutputControl::LeftTrigger => "LT",
        OutputControl::RightTrigger => "RT",
        OutputControl::LeftStickX => "LX",
        OutputControl::LeftStickY => "LY",
        OutputControl::RightStickX => "RX",
        OutputControl::RightStickY => "RY",
    }
}

fn normalize_axis(value: i16) -> f32 {
    if value < 0 {
        value as f32 / 32_768.0
    } else {
        value as f32 / 32_767.0
    }
}

fn ui_root(app: &mut X360ceApp, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, BACKGROUND);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 22.0).max(760.0));
                    app.render_header(ui);
                    ui.add_space(14.0);
                    app.render_status(ui);
                    ui.add_space(14.0);
                    ui.columns(2, |columns| {
                        let (left, right) = columns.split_at_mut(1);
                        left[0].set_width(left[0].available_width());
                        app.render_controller(&mut left[0]);
                        right[0].vertical(|ui| {
                            app.render_device_bar(ui);
                            ui.add_space(12.0);
                            app.render_mapping_editor(ui);
                            ui.add_space(12.0);
                            app.render_settings(ui);
                        });
                    });
                    ui.add_space(12.0);
                    egui::CollapsingHeader::new("raw input monitor")
                        .default_open(false)
                        .show(ui, |ui| {
                            app.render_raw_inputs(ui);
                        });
                    ui.add_space(18.0);
                });
            });
        });
    app.render_countdown(ui.ctx());
}

fn status_chip(ui: &mut egui::Ui, text: &str, fill: Color32, strong: bool) {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(16)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            let text_color = if strong {
                Color32::WHITE
            } else {
                TEXT
            };
            ui.label(RichText::new(text).size(11.0).strong().color(text_color));
        });
}

fn entry_label_for(profile: &ControllerProfile, control: OutputControl) -> String {
    profile
        .entry(control)
        .map(|entry| entry.binding.label())
        .unwrap_or_else(|| "Not mapped".to_owned())
}

fn draw_embedded_logo(ui: &mut egui::Ui) {
    let image = image::load_from_memory(include_bytes!("../assets/x360ce.png"))
        .ok()
        .map(|image| image.into_rgba8());

    if let Some(image) = image {
        let (width, height) = image.dimensions();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [width as usize, height as usize],
            image.as_raw(),
        );
        let texture = ui.ctx().load_texture(
            "x360ce-embedded-logo",
            color_image,
            egui::TextureOptions::LINEAR,
        );
        ui.add(
            egui::widgets::Image::new(&texture)
                .fit_to_exact_size(egui::vec2(56.0, 56.0)),
        );
    } else {
        draw_header_logo(ui);
    }
}
fn configure_egui(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Light);
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.faint_bg_color = SURFACE_MUTED;
    visuals.selection.bg_fill = ACCENT_SOFT;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.hovered.bg_fill = ACCENT_SOFT;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    ctx.set_visuals(visuals);
}

fn draw_header_logo(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(52.0, 52.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 12, CONTROLLER);
    let center = rect.center();
    painter.line_segment(
        [egui::pos2(center.x - 13.0, center.y), egui::pos2(center.x - 3.0, center.y)],
        Stroke::new(4.0, Color32::WHITE),
    );
    painter.line_segment(
        [egui::pos2(center.x - 8.0, center.y - 5.0), egui::pos2(center.x - 8.0, center.y + 5.0)],
        Stroke::new(4.0, Color32::WHITE),
    );
    painter.circle_filled(egui::pos2(center.x + 10.0, center.y - 5.0), 3.5, WARNING);
    painter.circle_filled(egui::pos2(center.x + 16.0, center.y + 1.0), 3.5, DANGER);
    painter.circle_filled(egui::pos2(center.x + 10.0, center.y + 7.0), 3.5, SUCCESS);
    painter.circle_filled(egui::pos2(center.x + 4.0, center.y + 1.0), 3.5, ACCENT);
}

fn load_window_icon(bytes: &[u8]) -> Result<egui::IconData> {
    let image = image::load_from_memory(bytes)
        .context("failed to decode embedded window icon")?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Ok(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn load_tray_image(bytes: &[u8]) -> Result<TrayImage> {
    let image = image::load_from_memory(bytes)
        .context("failed to decode embedded tray icon")?
        .into_rgba8();
    let (width, height) = image.dimensions();
    TrayImage::from_rgba(image.into_raw(), width, height)
        .map_err(|error| anyhow::anyhow!("invalid tray icon: {error}"))
}

fn tray_tooltip(state: &SavedState, virtual_connected: bool) -> String {
    format!(
        "x360ce {} — {}",
        config::app_version_label(),
        if virtual_connected {
            "forwarding"
        } else if state.emulation_enabled {
            "waiting for controller"
        } else {
            "paused"
        }
    )
}

fn update_status(check: &updater::UpdateCheck) -> String {
    let channel = if check.prerelease {
        "prerelease"
    } else {
        "stable"
    };
    if check.is_update_available {
        match &check.asset_name {
            Some(asset) => format!(
                "Update available: v{} ({channel}, {asset}).",
                check.latest_version
            ),
            None => format!(
                "Update available: v{} ({channel}), but no Windows asset was found.",
                check.latest_version
            ),
        }
    } else {
        format!(
            "Current version is latest: v{} ({channel}).",
            check.current_version
        )
    }
}
