use anyhow::{Context as _, Result};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Layout, RichText, Sense, Stroke, TextureHandle,
    TextureOptions,
};
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
    game_bar,
    mapper,
    model::{ControllerProfile, InputBinding, OutputControl, RawState, RuntimeSnapshot},
    profile_io,
    single_instance::BuildKind,
    startup, updater,
    wide::str_wide_null,
};

const DEFAULT_WINDOW_WIDTH: f32 = 1180.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 800.0;
const MIN_WINDOW_WIDTH: f32 = 960.0;
const MIN_WINDOW_HEIGHT: f32 = 680.0;
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
    controller_texture: Option<TextureHandle>,
    selected_control: OutputControl,
    selected_axis_negative: Option<bool>,
    learning_control: Option<OutputControl>,
    learning_output_negative: bool,
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
        let forwarding_active = !window_visible || !state.forward_only_in_tray;
        let engine = ControllerEngine::start(
            state.selected_device_guid.clone(),
            initial_profile,
            state.emulation_enabled,
            forwarding_active,
        );
        let game_bar_result = game_bar::set_controller_button_enabled(
            !state.block_game_bar_controller_button,
        );
        let mut status = if state.last_status.is_empty() {
            "Ready.".to_owned()
        } else {
            state.last_status.clone()
        };
        if let Err(error) = game_bar_result {
            status = format!("Xbox Game Bar shortcut update failed: {error}");
        }
        let tray = TrayState::new(&state, false).ok();
        let controller_texture = load_controller_texture(&creation_context.egui_ctx);

        Self {
            state,
            engine,
            runtime: RuntimeSnapshot::default(),
            tray,
            status,
            controller_texture,
            selected_control: OutputControl::A,
            selected_axis_negative: None,
            learning_control: None,
            learning_output_negative: false,
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

    fn sync_forwarding_mode(&self) {
        let forwarding_active = !self.window_visible || !self.state.forward_only_in_tray;
        self.engine
            .send(EngineCommand::SetForwardingActive(forwarding_active));
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.window_visible = true;
        self.countdown_started = None;
        self.last_activity = Instant::now();
        self.sync_forwarding_mode();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.window_visible = false;
        self.countdown_started = None;
        self.sync_forwarding_mode();
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
        let Some(binding) = mapper::detect_binding(&self.learn_baseline, &self.runtime.raw_state)
        else {
            return;
        };

        let centered_axis = if control.is_trigger() {
            match binding {
                InputBinding::AxisPositive { index } | InputBinding::AxisNegative { index } => self
                    .learn_baseline
                    .axes
                    .get(index as usize)
                    .copied()
                    .unwrap_or_default()
                    .unsigned_abs()
                    < 8_000,
                _ => false,
            }
        } else {
            false
        };
        let output_negative = self.learning_output_negative;
        {
            let mapping = self.current_profile_mut().entry_mut(control);
            mapping.binding = binding;
            mapping.centered_axis = centered_axis;
            if control.is_axis() {
                mapping.invert = output_negative;
            } else if control.is_trigger() {
                mapping.invert = false;
            }
        }

        self.learning_control = None;
        self.learning_output_negative = false;
        self.status = format!("{} mapped to {}.", control.label(), binding.label());
        self.push_profile();
        self.save();
    }

    fn begin_learning(&mut self, control: OutputControl) {
        self.begin_learning_with_direction(control, false);
    }

    fn begin_learning_with_direction(&mut self, control: OutputControl, output_negative: bool) {
        self.selected_control = control;
        self.selected_axis_negative = control.is_axis().then_some(output_negative);
        self.learning_control = Some(control);
        self.learning_output_negative = output_negative;
        self.learn_baseline = self.runtime.raw_state.clone();
        let direction = if control.is_axis() {
            if output_negative {
                "negative output"
            } else {
                "positive output"
            }
        } else {
            "input"
        };
        self.status = format!("Move or press {direction} for {}.", control.label());
        self.mark_activity();
    }

    fn clear_mapping(&mut self, control: OutputControl) {
        self.current_profile_mut().entry_mut(control).binding = InputBinding::None;
        self.learning_control = None;
        self.learning_output_negative = false;
        self.selected_axis_negative = None;
        self.status = format!("{} mapping cleared.", control.label());
        self.push_profile();
        self.save();
    }

    fn set_axis_binding_direction(&mut self, control: OutputControl, positive: bool) {
        let changed = {
            let mapping = self.current_profile_mut().entry_mut(control);
            let next = match mapping.binding {
                InputBinding::AxisPositive { index } | InputBinding::AxisNegative { index } => {
                    if positive {
                        InputBinding::AxisPositive { index }
                    } else {
                        InputBinding::AxisNegative { index }
                    }
                }
                _ => return,
            };
            if mapping.binding == next {
                false
            } else {
                mapping.binding = next;
                true
            }
        };
        if changed {
            self.status = format!(
                "{} source direction set to {}.",
                control.label(),
                if positive { "+" } else { "-" }
            );
            self.push_profile();
            self.save();
            self.mark_activity();
        }
    }

    fn export_current_mapping(&mut self) {
        match profile_io::export_profile(&self.current_profile()) {
            Ok(Some(path)) => {
                self.status = format!("Mapping exported: {}", path.display());
            }
            Ok(None) => {
                self.status = "Mapping export canceled.".to_owned();
            }
            Err(error) => {
                self.status = format!("Mapping export failed: {error}");
            }
        }
    }

    fn import_mapping(&mut self) {
        match profile_io::import_profile() {
            Ok(Some((path, mut profile))) => {
                let guid = self.state.selected_device_guid.clone();
                profile.device_guid = guid.clone();
                if profile.name.trim().is_empty() {
                    profile.name = "Imported profile".to_owned();
                }
                self.state.profiles.insert(guid, profile.clone());
                self.engine.send(EngineCommand::SetProfile(profile));
                self.learning_control = None;
                self.learning_output_negative = false;
                self.selected_axis_negative = None;
                self.status = format!("Mapping imported: {}", path.display());
                self.save();
                self.mark_activity();
            }
            Ok(None) => {
                self.status = "Mapping import canceled.".to_owned();
            }
            Err(error) => {
                self.status = format!("Mapping import failed: {error}");
            }
        }
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

        if self.learning_control.is_some()
            && ctx.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.learning_control = None;
            self.learning_output_negative = false;
            self.status = "Mapping capture canceled.".to_owned();
            self.mark_activity();
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
            ("driver missing", WARNING)
        } else if self.window_visible && self.state.forward_only_in_tray {
            ("configuring", ACCENT)
        } else if self.runtime.virtual_connected {
            ("forwarding", SUCCESS)
        } else if !self.state.emulation_enabled {
            ("paused", MUTED)
        } else {
            ("waiting", WARNING)
        };

        let runtime_error = self.runtime.last_error.clone();
        compact_surface(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                status_chip(ui, label, color, true);
                status_chip(
                    ui,
                    if self.window_visible && self.state.forward_only_in_tray {
                        "output isolated"
                    } else if self.state.emulation_enabled {
                        "virtual on"
                    } else {
                        "virtual off"
                    },
                    ACCENT_SOFT,
                    false,
                );
                status_chip(
                    ui,
                    &format!("{} detected", self.runtime.devices.len()),
                    SURFACE_MUTED,
                    false,
                );
                if self
                    .last_update_check
                    .as_ref()
                    .map(|check| check.is_update_available)
                    .unwrap_or(false)
                {
                    status_chip(ui, "update available", WARNING, true);
                }

                ui.add_space(6.0);
                ui.label(RichText::new(&self.status).size(11.0).color(MUTED));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if !self.runtime.driver_installed && ui.button("Install driver").clicked() {
                        match driver::launch_installer_elevated() {
                            Ok(_) => {
                                self.status =
                                    "Driver installer opened. Restart x360ce after install."
                                        .to_owned()
                            }
                            Err(error) => {
                                self.status = format!("Driver installer failed: {error}")
                            }
                        }
                    }
                    if ui.button("Refresh").clicked() {
                        self.engine.send(EngineCommand::Refresh);
                        self.status = "Refreshing controllers…".to_owned();
                        self.mark_activity();
                    }
                    if ui
                        .button(if self.state.emulation_enabled {
                            "Disable"
                        } else {
                            "Enable"
                        })
                        .clicked()
                    {
                        self.set_emulation_enabled(!self.state.emulation_enabled);
                    }
                });
            });

            if let Some(error) = runtime_error {
                ui.add_space(4.0);
                ui.label(RichText::new(error).size(11.0).color(DANGER));
            }
        });
    }

    fn render_device_bar(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Input controller").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{} detected", self.runtime.devices.len()))
                            .size(11.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(6.0);

            let selected_name = self
                .runtime
                .devices
                .iter()
                .find(|device| device.guid == self.state.selected_device_guid)
                .map(|device| ellipsize(&device.name, 34))
                .unwrap_or_else(|| "No controller detected".to_owned());
            let combo_width = (ui.available_width() - 76.0).clamp(160.0, 280.0);
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("controller-selector")
                    .selected_text(selected_name)
                    .width(combo_width)
                    .show_ui(ui, |ui| {
                        let devices = self.runtime.devices.clone();
                        for device in devices {
                            let label = format!(
                                "{} · {} axes · {} buttons · {} hats",
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
            });
        });
    }


    fn render_controller(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Virtual Xbox 360 layout").strong());
                    ui.label(
                        RichText::new("Click to select · double-click to learn · Esc to cancel")
                            .size(11.0)
                            .color(MUTED),
                    );
                });
                if let Some(control) = self.learning_control {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        status_chip(
                            ui,
                            &format!("learning {}", control.label()),
                            ACCENT,
                            true,
                        );
                    });
                }
            });

            ui.add_space(4.0);
            let width = ui.available_width();
            let height = (width * 0.62).clamp(240.0, 310.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
            let texture_id = self.controller_texture.as_ref().map(|texture| texture.id());
            if let Some(texture_id) = texture_id {
                ui.painter().image(
                    texture_id,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.13), egui::pos2(1.0, 0.88)),
                    Color32::WHITE,
                );
            } else {
                draw_controller_body(ui, rect);
            }

            let profile = self.current_profile();
            for (control, x, y) in controller_points() {
                let center = egui::pos2(
                    rect.left() + rect.width() * x,
                    rect.top() + rect.height() * y,
                );
                let radius = if control.is_trigger() { 13.0 } else { 11.0 };
                controller_control(
                    self,
                    ui,
                    &profile,
                    control,
                    center,
                    radius,
                    short_control_label(control),
                );
            }

            draw_stick_controls(
                self,
                ui,
                &profile,
                true,
                egui::pos2(rect.left() + rect.width() * 0.365, rect.top() + rect.height() * 0.67),
            );
            draw_stick_controls(
                self,
                ui,
                &profile,
                false,
                egui::pos2(rect.left() + rect.width() * 0.635, rect.top() + rect.height() * 0.67),
            );
        });
    }

    fn render_mapping_editor(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            let control = self.selected_control;
            let profile = self.current_profile();
            let selected_entry = profile.entry(control).cloned();
            let selected_mapped = selected_entry
                .as_ref()
                .map(|entry| entry.binding != InputBinding::None)
                .unwrap_or(false);

            let title = ui.add(
                egui::Label::new(RichText::new(control.label()).size(18.0).strong())
                    .sense(Sense::click()),
            );
            if title.double_clicked() {
                self.begin_learning(control);
            }
            ui.label(
                RichText::new(entry_label_for(&profile, control))
                    .color(if selected_mapped { MUTED } else { WARNING }),
            );

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Learn").clicked() {
                    self.begin_learning(control);
                }
                if ui
                    .add_enabled(selected_mapped, egui::Button::new("Clear"))
                    .clicked()
                {
                    self.clear_mapping(control);
                }
                if ui.button("Export").clicked() {
                    self.export_current_mapping();
                }
                if ui.button("Import").clicked() {
                    self.import_mapping();
                }
            });
            ui.label(
                RichText::new(format!(
                    "Detected: {}",
                    primary_live_input_label(&self.runtime.raw_state)
                ))
                .size(11.0)
                .color(MUTED),
            );

            if control.is_axis() || control.is_trigger() {
                ui.add_space(8.0);
                if let Some(binding) = selected_entry.as_ref().map(|entry| entry.binding) {
                    if let Some(positive) = axis_binding_positive(binding) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Source direction").size(11.0).color(MUTED));
                            if ui.selectable_label(positive, "+").clicked() {
                                self.set_axis_binding_direction(control, true);
                            }
                            if ui.selectable_label(!positive, "−").clicked() {
                                self.set_axis_binding_direction(control, false);
                            }
                        });
                    }
                }

                let mut changed = false;
                {
                    let mapping = self.current_profile_mut().entry_mut(control);
                    ui.horizontal_wrapped(|ui| {
                        changed |= ui
                            .checkbox(&mut mapping.invert, "Invert output")
                            .changed();
                        if control.is_trigger() {
                            changed |= ui
                                .checkbox(&mut mapping.centered_axis, "Split axis")
                                .changed();
                        }
                    });
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
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mappings").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new("double-click to learn · × to clear")
                            .size(10.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(4.0);

            let controls: Vec<OutputControl> = OutputControl::BUTTONS
                .into_iter()
                .chain(OutputControl::ANALOGS)
                .collect();
            let split = (controls.len() + 1) / 2;
            ui.columns(2, |columns| {
                for (column_index, chunk) in controls.chunks(split).enumerate() {
                    columns[column_index].vertical(|ui| {
                        for item in chunk {
                            let entry = profile.entry(*item);
                            let mapped = entry
                                .map(|entry| entry.binding != InputBinding::None)
                                .unwrap_or(false);
                            let active = entry
                                .map(|entry| {
                                    control_preview_active(*item, entry, &self.runtime.raw_state)
                                })
                                .unwrap_or(false);
                            let label = if mapped {
                                format!(
                                    "{} · {}",
                                    compact_control_label(*item),
                                    entry_label_for(&profile, *item)
                                )
                            } else {
                                format!("! {} · Not mapped", compact_control_label(*item))
                            };
                            let rich = if active {
                                RichText::new(label).strong().color(SUCCESS)
                            } else if mapped {
                                RichText::new(label).size(11.0).color(TEXT)
                            } else {
                                RichText::new(label)
                                    .size(11.0)
                                    .color(Color32::from_rgba_unmultiplied(154, 112, 45, 170))
                            };

                            let mut clear_clicked = false;
                            let response = ui.horizontal(|ui| {
                                let row_width = (ui.available_width() - 30.0).max(110.0);
                                let response = ui.add_sized(
                                    [row_width, 22.0],
                                    egui::SelectableLabel::new(
                                        self.selected_control == *item,
                                        rich,
                                    ),
                                );
                                clear_clicked = ui
                                    .add_enabled(mapped, egui::Button::new("×"))
                                    .on_hover_text("Clear mapping")
                                    .clicked();
                                response
                            }).inner;

                            if clear_clicked {
                                self.clear_mapping(*item);
                            } else if response.double_clicked() {
                                self.begin_learning(*item);
                            } else if response.clicked() {
                                self.selected_control = *item;
                                self.selected_axis_negative = None;
                                self.mark_activity();
                            }
                        }
                    });
                }
            });
        });
    }

    fn render_raw_inputs(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Live physical input").strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(primary_live_input_label(&self.runtime.raw_state))
                            .size(11.0)
                            .color(MUTED),
                    );
                });
            });
            ui.add_space(6.0);

            for (index, value) in self.runtime.raw_state.axes.iter().enumerate() {
                let normalized = normalize_axis(*value);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("A{}", index + 1)).size(11.0));
                    let bar_width = (ui.available_width() - 58.0).max(100.0);
                    ui.add(
                        egui::ProgressBar::new(((normalized + 1.0) * 0.5).clamp(0.0, 1.0))
                            .desired_width(bar_width),
                    );
                    ui.label(RichText::new(format!("{normalized:.2}")).size(10.0));
                });
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for (index, pressed) in self.runtime.raw_state.buttons.iter().enumerate() {
                    status_chip(
                        ui,
                        &format!("B{}", index + 1),
                        if *pressed { SUCCESS } else { SURFACE_MUTED },
                        *pressed,
                    );
                }
            });
            ui.add_space(5.0);
            ui.horizontal_wrapped(|ui| {
                for (index, hat) in self.runtime.raw_state.hats.iter().enumerate() {
                    status_chip(
                        ui,
                        &format!("H{} {}", index + 1, hat.label()),
                        if *hat == crate::model::HatDirection::Centered {
                            SURFACE_MUTED
                        } else {
                            WARNING
                        },
                        *hat != crate::model::HatDirection::Centered,
                    );
                }
            });
        });
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        surface(ui, |ui| {
            ui.label(RichText::new("Settings and updates").strong());
            ui.add_space(6.0);

            let mut changed = false;
            ui.columns(2, |columns| {
                changed |= columns[0]
                    .checkbox(&mut self.state.auto_tray_enabled, "Auto tray")
                    .changed();
                changed |= columns[0]
                    .checkbox(&mut self.state.start_in_tray, "Start in tray")
                    .changed();
                changed |= columns[0]
                    .checkbox(&mut self.state.forward_only_in_tray, "Forward only in tray")
                    .changed();
                changed |= columns[1]
                    .checkbox(
                        &mut self.state.block_game_bar_controller_button,
                        "Block Game Bar button",
                    )
                    .changed();
                changed |= columns[1]
                    .checkbox(&mut self.state.include_prereleases, "Prereleases")
                    .changed();
            });

            if changed {
                self.sync_forwarding_mode();
                if let Err(error) = game_bar::set_controller_button_enabled(
                    !self.state.block_game_bar_controller_button,
                ) {
                    self.status = format!("Xbox Game Bar shortcut update failed: {error}");
                }
                self.mark_activity();
                self.save();
                self.sync_tray();
            }

            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !config::is_dev_build(),
                        egui::Button::new(if self.state.startup_enabled {
                            "Disable startup"
                        } else {
                            "Enable startup"
                        }),
                    )
                    .clicked()
                {
                    self.toggle_startup();
                }
                if ui
                    .add_enabled(
                        !config::is_dev_build() && self.update_rx.is_none(),
                        egui::Button::new("Check updates"),
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
                                check.is_update_available && check.asset_download_url.is_some()
                            })
                            .unwrap_or(false),
                        egui::Button::new("Install update"),
                    )
                    .clicked()
                {
                    self.install_update();
                }
                if ui.button("Open releases").clicked() {
                    if let Err(error) = updater::open_releases_page() {
                        self.status = format!("Could not open releases: {error}");
                    }
                }
            });

            ui.add_space(5.0);
            if config::is_dev_build() {
                ui.label(
                    RichText::new("Update checks disabled in dev builds.")
                        .size(11.0)
                        .color(MUTED),
                );
            } else if let Some(check) = &self.last_update_check {
                ui.label(RichText::new(update_status(check)).size(11.0).color(MUTED));
            } else {
                ui.label(RichText::new("No update check yet.").size(11.0).color(MUTED));
            }
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
    let shell = Color32::from_rgb(38, 42, 50);
    let shell_dark = Color32::from_rgb(25, 28, 34);
    let shell_mid = Color32::from_rgb(67, 73, 84);
    let silver = Color32::from_rgb(223, 227, 232);
    let silver_dark = Color32::from_rgb(180, 187, 196);

    let body = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.center().y + 5.0),
        egui::vec2(rect.width() * 0.74, rect.height() * 0.70),
    );

    painter.circle_filled(
        egui::pos2(body.left() + body.width() * 0.12, body.bottom() - 4.0),
        body.height() * 0.34,
        shell,
    );
    painter.circle_filled(
        egui::pos2(body.right() - body.width() * 0.12, body.bottom() - 4.0),
        body.height() * 0.34,
        shell,
    );
    painter.rect_filled(body, 76, shell);
    painter.rect_stroke(
        body,
        76,
        Stroke::new(2.0, shell_mid),
        egui::StrokeKind::Inside,
    );

    painter.rect_filled(
        egui::Rect::from_center_size(
            egui::pos2(body.center().x, body.top() + 30.0),
            egui::vec2(body.width() * 0.35, 16.0),
        ),
        8,
        shell_mid,
    );

    for x in [0.24, 0.76] {
        painter.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(body.left() + body.width() * x, body.top() + 5.0),
                egui::vec2(72.0, 15.0),
            ),
            7,
            shell_dark,
        );
    }

    let dpad = egui::pos2(body.left() + body.width() * 0.22, body.center().y + 4.0);
    painter.rect_filled(
        egui::Rect::from_center_size(dpad, egui::vec2(72.0, 22.0)),
        8,
        silver,
    );
    painter.rect_filled(
        egui::Rect::from_center_size(dpad, egui::vec2(22.0, 72.0)),
        8,
        silver,
    );
    painter.circle_stroke(dpad, 39.0, Stroke::new(1.5, silver_dark));

    for (center, color) in [
        (
            egui::pos2(body.right() - body.width() * 0.18, body.center().y - 25.0),
            Color32::from_rgb(234, 194, 42),
        ),
        (
            egui::pos2(body.right() - body.width() * 0.11, body.center().y + 14.0),
            Color32::from_rgb(222, 76, 72),
        ),
        (
            egui::pos2(body.right() - body.width() * 0.18, body.center().y + 53.0),
            Color32::from_rgb(83, 161, 55),
        ),
        (
            egui::pos2(body.right() - body.width() * 0.25, body.center().y + 14.0),
            Color32::from_rgb(58, 115, 215),
        ),
    ] {
        painter.circle_filled(center, 18.0, color);
        painter.circle_stroke(center, 18.0, Stroke::new(1.5, Color32::WHITE));
    }

    painter.circle_filled(
        egui::pos2(body.center().x, body.center().y - 17.0),
        18.0,
        silver,
    );
    painter.circle_stroke(
        egui::pos2(body.center().x, body.center().y - 17.0),
        18.0,
        Stroke::new(2.0, Color32::from_rgb(111, 187, 58)),
    );

    for center in [
        egui::pos2(body.center().x - 38.0, body.center().y + 3.0),
        egui::pos2(body.center().x + 38.0, body.center().y + 3.0),
    ] {
        painter.circle_filled(center, 12.0, silver);
        painter.circle_stroke(center, 12.0, Stroke::new(1.0, silver_dark));
    }
}

fn controller_points() -> [(OutputControl, f32, f32); 15] {
    [
        (OutputControl::LeftTrigger, 0.19, 0.08),
        (OutputControl::RightTrigger, 0.81, 0.08),
        (OutputControl::LeftShoulder, 0.28, 0.17),
        (OutputControl::RightShoulder, 0.72, 0.17),
        (OutputControl::Guide, 0.50, 0.34),
        (OutputControl::Back, 0.43, 0.40),
        (OutputControl::Start, 0.57, 0.40),
        (OutputControl::Y, 0.76, 0.28),
        (OutputControl::B, 0.83, 0.40),
        (OutputControl::A, 0.76, 0.52),
        (OutputControl::X, 0.69, 0.40),
        (OutputControl::DpadUp, 0.25, 0.38),
        (OutputControl::DpadRight, 0.31, 0.49),
        (OutputControl::DpadDown, 0.25, 0.60),
        (OutputControl::DpadLeft, 0.19, 0.49),
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


fn ellipsize(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_owned();
    }
    let keep = max_chars.saturating_sub(1);
    let mut shortened = value.chars().take(keep).collect::<String>();
    shortened.push('…');
    shortened
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
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.vertical(|ui| {
                    ui.set_max_width((ui.available_width() - 16.0).max(820.0));
                    app.render_header(ui);
                    ui.add_space(8.0);
                    app.render_status(ui);
                    ui.add_space(8.0);

                    ui.columns(2, |columns| {
                        let (left, right) = columns.split_at_mut(1);
                        left[0].vertical(|ui| {
                            app.render_controller(ui);
                            ui.add_space(8.0);
                            app.render_raw_inputs(ui);
                        });
                        right[0].vertical(|ui| {
                            app.render_device_bar(ui);
                            ui.add_space(8.0);
                            app.render_mapping_editor(ui);
                            ui.add_space(8.0);
                            app.render_settings(ui);
                        });
                    });
                    ui.add_space(12.0);
                });
            });
        });
    app.render_countdown(ui.ctx());
}


fn compact_surface(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, add_contents);
}

fn controller_control(
    app: &mut X360ceApp,
    ui: &mut egui::Ui,
    profile: &ControllerProfile,
    control: OutputControl,
    center: egui::Pos2,
    radius: f32,
    label: &str,
) {
    let hit = egui::Rect::from_center_size(center, egui::vec2(radius * 2.8, radius * 2.8));
    let response = ui.interact(
        hit,
        ui.make_persistent_id(("controller-control", control)),
        Sense::click(),
    );
    if response.double_clicked() {
        app.begin_learning(control);
    } else if response.clicked() {
        app.selected_control = control;
        app.selected_axis_negative = None;
        app.mark_activity();
    }

    let entry = profile.entry(control);
    let mapped = entry
        .map(|entry| entry.binding != InputBinding::None)
        .unwrap_or(false);
    let active = entry
        .map(|entry| control_preview_active(control, entry, &app.runtime.raw_state))
        .unwrap_or(false);
    let selected = app.selected_control == control && app.selected_axis_negative.is_none();
    let fill = if active {
        Color32::from_rgba_unmultiplied(37, 145, 83, 220)
    } else if selected {
        Color32::from_rgba_unmultiplied(52, 111, 224, 210)
    } else if mapped {
        Color32::from_rgba_unmultiplied(255, 255, 255, 32)
    } else {
        Color32::from_rgba_unmultiplied(205, 125, 21, 28)
    };
    let stroke = if active {
        Stroke::new(2.0, Color32::from_rgb(207, 245, 222))
    } else if selected {
        Stroke::new(2.5, Color32::WHITE)
    } else if mapped {
        Stroke::new(1.2, Color32::from_rgba_unmultiplied(255, 255, 255, 170))
    } else {
        Stroke::new(1.2, Color32::from_rgba_unmultiplied(205, 125, 21, 180))
    };
    ui.painter().circle_filled(center, radius, fill);
    ui.painter().circle_stroke(center, radius, stroke);
    ui.painter().text(
        center,
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(9.0),
        if active || selected {
            Color32::WHITE
        } else if mapped {
            Color32::from_rgba_unmultiplied(255, 255, 255, 215)
        } else {
            WARNING
        },
    );
    if !mapped {
        ui.painter().text(
            center + egui::vec2(radius * 0.8, -radius * 0.8),
            Align2::CENTER_CENTER,
            "!",
            FontId::proportional(8.0),
            WARNING,
        );
    }
    response.on_hover_text(format!(
        "{}
{}
Double-click to learn",
        control.label(),
        entry_label_for(profile, control)
    ));
}

fn draw_stick_controls(
    app: &mut X360ceApp,
    ui: &mut egui::Ui,
    profile: &ControllerProfile,
    left: bool,
    center: egui::Pos2,
) {
    let (x_control, y_control, click_control, name) = if left {
        (
            OutputControl::LeftStickX,
            OutputControl::LeftStickY,
            OutputControl::LeftThumb,
            "L",
        )
    } else {
        (
            OutputControl::RightStickX,
            OutputControl::RightStickY,
            OutputControl::RightThumb,
            "R",
        )
    };

    let x_value = profile
        .entry(x_control)
        .map(|entry| mapper::axis_preview(entry, &app.runtime.raw_state))
        .unwrap_or_default();
    let y_value = profile
        .entry(y_control)
        .map(|entry| mapper::axis_preview(entry, &app.runtime.raw_state))
        .unwrap_or_default();

    controller_control(
        app,
        ui,
        profile,
        click_control,
        center,
        10.0,
        &format!("{name}3"),
    );

    for (control, output_negative, offset, label, active) in [
        (y_control, false, egui::vec2(0.0, -28.0), "↑", y_value > 0.15),
        (x_control, false, egui::vec2(28.0, 0.0), "→", x_value > 0.15),
        (y_control, true, egui::vec2(0.0, 28.0), "↓", y_value < -0.15),
        (x_control, true, egui::vec2(-28.0, 0.0), "←", x_value < -0.15),
    ] {
        let marker_center = center + offset;
        let hit = egui::Rect::from_center_size(marker_center, egui::vec2(20.0, 20.0));
        let response = ui.interact(
            hit,
            ui.make_persistent_id(("stick-direction", left, label)),
            Sense::click(),
        );
        if response.double_clicked() {
            app.begin_learning_with_direction(control, output_negative);
        } else if response.clicked() {
            app.selected_control = control;
            app.selected_axis_negative = Some(output_negative);
            app.mark_activity();
        }

        let entry = profile.entry(control);
        let mapped = entry
            .map(|entry| entry.binding != InputBinding::None)
            .unwrap_or(false);
        let selected = app.selected_control == control
            && app.selected_axis_negative == Some(output_negative);
        let fill = if active {
            Color32::from_rgba_unmultiplied(37, 145, 83, 225)
        } else if selected {
            Color32::from_rgba_unmultiplied(52, 111, 224, 220)
        } else if mapped {
            Color32::from_rgba_unmultiplied(255, 255, 255, 34)
        } else {
            Color32::from_rgba_unmultiplied(205, 125, 21, 24)
        };
        ui.painter().circle_filled(marker_center, 8.5, fill);
        ui.painter().circle_stroke(
            marker_center,
            8.5,
            Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected || active {
                    Color32::WHITE
                } else if mapped {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 155)
                } else {
                    Color32::from_rgba_unmultiplied(205, 125, 21, 175)
                },
            ),
        );
        ui.painter().text(
            marker_center,
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(10.0),
            if active || selected {
                Color32::WHITE
            } else if mapped {
                Color32::from_rgba_unmultiplied(255, 255, 255, 220)
            } else {
                WARNING
            },
        );
        response.on_hover_text(format!(
            "{} {} output
Double-click to learn",
            control.label(),
            if output_negative { "negative" } else { "positive" }
        ));
    }
}

fn compact_control_label(control: OutputControl) -> &'static str {
    match control {
        OutputControl::LeftShoulder => "LB",
        OutputControl::RightShoulder => "RB",
        OutputControl::LeftThumb => "L3",
        OutputControl::RightThumb => "R3",
        OutputControl::DpadUp => "D-pad ↑",
        OutputControl::DpadRight => "D-pad →",
        OutputControl::DpadDown => "D-pad ↓",
        OutputControl::DpadLeft => "D-pad ←",
        OutputControl::LeftTrigger => "LT",
        OutputControl::RightTrigger => "RT",
        OutputControl::LeftStickX => "Left X",
        OutputControl::LeftStickY => "Left Y",
        OutputControl::RightStickX => "Right X",
        OutputControl::RightStickY => "Right Y",
        _ => control.label(),
    }
}

fn status_chip(ui: &mut egui::Ui, text: &str, fill: Color32, strong: bool) {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(14)
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            let text_color = if strong && fill != SURFACE_MUTED && fill != ACCENT_SOFT {
                Color32::WHITE
            } else {
                TEXT
            };
            ui.label(RichText::new(text).size(11.0).strong().color(text_color));
        });
}

fn control_preview_active(control: OutputControl, entry: &crate::model::MappingEntry, raw: &RawState) -> bool {
    if control.is_trigger() {
        mapper::trigger_preview(entry, raw) > 0.15
    } else if control.is_axis() {
        mapper::axis_preview(entry, raw).abs() > 0.15
    } else {
        mapper::binding_pressed(entry, raw)
    }
}

fn primary_live_input_label(raw: &RawState) -> String {
    for (index, pressed) in raw.buttons.iter().enumerate() {
        if *pressed {
            return format!("Button {}", index + 1);
        }
    }
    for (index, value) in raw.axes.iter().enumerate() {
        let normalized = normalize_axis(*value);
        if normalized >= 0.18 {
            return format!("Axis {} +", index + 1);
        }
        if normalized <= -0.18 {
            return format!("Axis {} -", index + 1);
        }
    }
    for (index, hat) in raw.hats.iter().enumerate() {
        if *hat != crate::model::HatDirection::Centered {
            return format!("Hat {} {}", index + 1, hat.label());
        }
    }
    "none".to_owned()
}




fn axis_binding_positive(binding: InputBinding) -> Option<bool> {
    match binding {
        InputBinding::AxisPositive { .. } => Some(true),
        InputBinding::AxisNegative { .. } => Some(false),
        _ => None,
    }
}

fn load_controller_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    let image = image::load_from_memory(include_bytes!("../assets/x360ce.png"))
        .ok()?
        .into_rgba8();
    let (width, height) = image.dimensions();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        image.as_raw(),
    );
    Some(ctx.load_texture(
        "x360ce-controller-layout",
        color_image,
        TextureOptions::LINEAR,
    ))
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
