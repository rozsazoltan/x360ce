use crate::{
    driver,
    mapper,
    model::{ControllerProfile, DeviceDescriptor, HatDirection, RawState, RuntimeSnapshot},
    virtual_gamepad::VirtualGamepad,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use sdl2::{joystick::HatState, JoystickSubsystem};
use std::{
    sync::{Arc, RwLock},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_millis(8);
const ENUMERATION_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub enum EngineCommand {
    SelectDevice(String),
    SetProfile(ControllerProfile),
    SetEnabled(bool),
    Refresh,
    Shutdown,
}

pub struct ControllerEngine {
    commands: Sender<EngineCommand>,
    snapshot: Arc<RwLock<RuntimeSnapshot>>,
    worker: Option<JoinHandle<()>>,
}

impl ControllerEngine {
    pub fn start(
        selected_guid: String,
        profile: ControllerProfile,
        enabled: bool,
    ) -> Self {
        let (commands, receiver) = unbounded();
        let snapshot = Arc::new(RwLock::new(RuntimeSnapshot {
            driver_installed: driver::is_installed(),
            ..Default::default()
        }));
        let worker_snapshot = Arc::clone(&snapshot);
        let worker = thread::Builder::new()
            .name("x360ce-controller-engine".to_owned())
            .spawn(move || {
                if let Err(error) = run_worker(
                    receiver,
                    worker_snapshot.clone(),
                    selected_guid,
                    profile,
                    enabled,
                ) {
                    update_snapshot(&worker_snapshot, |state| {
                        state.last_error = Some(format!("controller engine stopped: {error:#}"));
                        state.virtual_connected = false;
                    });
                }
            })
            .ok();

        Self {
            commands,
            snapshot,
            worker,
        }
    }

    pub fn send(&self, command: EngineCommand) {
        let _ = self.commands.send(command);
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.snapshot
            .read()
            .map(|state| state.clone())
            .unwrap_or_default()
    }
}

impl Drop for ControllerEngine {
    fn drop(&mut self) {
        let _ = self.commands.send(EngineCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(
    commands: Receiver<EngineCommand>,
    snapshot: Arc<RwLock<RuntimeSnapshot>>,
    mut selected_guid: String,
    mut profile: ControllerProfile,
    mut enabled: bool,
) -> anyhow::Result<()> {
    sdl2::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    sdl2::hint::set("SDL_JOYSTICK_THREAD", "1");

    let sdl = sdl2::init().map_err(|error| anyhow::anyhow!(error))?;
    let joystick_subsystem = sdl
        .joystick()
        .map_err(|error| anyhow::anyhow!(error))?;
    joystick_subsystem.set_event_state(false);

    let mut devices = enumerate_devices(&joystick_subsystem);
    let mut joystick = open_selected(&joystick_subsystem, &devices, &selected_guid);
    let mut virtual_gamepad: Option<VirtualGamepad> = None;
    let mut previous_raw = RawState::default();
    let mut sequence = 0_u64;
    let mut last_enumeration = Instant::now();
    let mut force_refresh = true;
    let mut driver_installed = driver::is_installed();
    let mut next_virtual_connect_attempt = Instant::now();
    let mut last_virtual_error: Option<String> = None;

    loop {
        while let Ok(command) = commands.try_recv() {
            match command {
                EngineCommand::SelectDevice(guid) => {
                    selected_guid = guid;
                    joystick = open_selected(&joystick_subsystem, &devices, &selected_guid);
                    previous_raw = RawState::default();
                    virtual_gamepad = None;
                    next_virtual_connect_attempt = Instant::now();
                    last_virtual_error = None;
                }
                EngineCommand::SetProfile(next_profile) => profile = next_profile,
                EngineCommand::SetEnabled(next_enabled) => {
                    enabled = next_enabled;
                    virtual_gamepad = None;
                    next_virtual_connect_attempt = Instant::now();
                    last_virtual_error = None;
                }
                EngineCommand::Refresh => force_refresh = true,
                EngineCommand::Shutdown => return Ok(()),
            }
        }

        if force_refresh || last_enumeration.elapsed() >= ENUMERATION_INTERVAL {
            joystick_subsystem.update();
            devices = enumerate_devices(&joystick_subsystem);
            driver_installed = driver::is_installed();
            let selected_is_attached = joystick
                .as_ref()
                .map(|device| device.attached())
                .unwrap_or(false);
            if !selected_is_attached {
                joystick = open_selected(&joystick_subsystem, &devices, &selected_guid);
                virtual_gamepad = None;
                next_virtual_connect_attempt = Instant::now();
                last_virtual_error = None;
            }
            force_refresh = false;
            last_enumeration = Instant::now();
        }

        joystick_subsystem.update();
        let raw = joystick
            .as_ref()
            .filter(|device| device.attached())
            .map(read_raw_state)
            .unwrap_or_default();
        let active = has_activity(&previous_raw, &raw);
        if active {
            sequence = sequence.wrapping_add(1);
        }
        if enabled && joystick.is_some() && driver_installed {
            if virtual_gamepad.is_none() && Instant::now() >= next_virtual_connect_attempt {
                match VirtualGamepad::connect() {
                    Ok(gamepad) => {
                        virtual_gamepad = Some(gamepad);
                        last_virtual_error = None;
                    }
                    Err(error) => {
                        last_virtual_error = Some(error.to_string());
                        next_virtual_connect_attempt = Instant::now() + Duration::from_secs(2);
                    }
                }
            }
            if let Some(gamepad) = &virtual_gamepad {
                let report = mapper::map_report(&profile, &raw);
                if let Err(error) = gamepad.update(&report) {
                    last_virtual_error = Some(error.to_string());
                    virtual_gamepad = None;
                    next_virtual_connect_attempt = Instant::now() + Duration::from_secs(2);
                }
            }
        } else {
            virtual_gamepad = None;
            last_virtual_error = if enabled && joystick.is_some() && !driver_installed {
                Some("ViGEmBus driver is not installed.".to_owned())
            } else {
                None
            };
        }
        let last_error = last_virtual_error.clone();

        let selected_instance_id = joystick.as_ref().map(|device| device.instance_id());
        update_snapshot(&snapshot, |state| {
            state.devices = devices.clone();
            state.selected_instance_id = selected_instance_id;
            state.raw_state = raw.clone();
            state.virtual_connected = virtual_gamepad.is_some();
            state.driver_installed = driver_installed;
            state.last_error = last_error;
            if active {
                state.last_controller_activity_sequence = sequence;
            }
        });

        previous_raw = raw;
        thread::sleep(POLL_INTERVAL);
    }
}

fn enumerate_devices(subsystem: &JoystickSubsystem) -> Vec<DeviceDescriptor> {
    let count = subsystem.num_joysticks().unwrap_or_default();
    let mut devices = Vec::new();
    for index in 0..count {
        let Ok(joystick) = subsystem.open(index) else {
            continue;
        };
        let name = joystick.name();
        if is_virtual_output(&name) {
            continue;
        }
        devices.push(DeviceDescriptor {
            instance_id: joystick.instance_id(),
            name,
            guid: joystick.guid().string(),
            axes: joystick.num_axes(),
            buttons: joystick.num_buttons(),
            hats: joystick.num_hats(),
        });
    }
    devices
}

fn open_selected(
    subsystem: &JoystickSubsystem,
    devices: &[DeviceDescriptor],
    selected_guid: &str,
) -> Option<sdl2::joystick::Joystick> {
    let target_guid = if selected_guid.is_empty() {
        devices.first().map(|device| device.guid.as_str())?
    } else {
        selected_guid
    };

    let count = subsystem.num_joysticks().ok()?;
    for index in 0..count {
        let Ok(joystick) = subsystem.open(index) else {
            continue;
        };
        if is_virtual_output(&joystick.name()) {
            continue;
        }
        if joystick.guid().string().eq_ignore_ascii_case(target_guid) {
            return Some(joystick);
        }
    }
    None
}

fn read_raw_state(joystick: &sdl2::joystick::Joystick) -> RawState {
    let axes = (0..joystick.num_axes())
        .map(|index| joystick.axis(index).unwrap_or_default())
        .collect();
    let buttons = (0..joystick.num_buttons())
        .map(|index| joystick.button(index).unwrap_or(false))
        .collect();
    let hats = (0..joystick.num_hats())
        .map(|index| {
            joystick
                .hat(index)
                .map(convert_hat)
                .unwrap_or(HatDirection::Centered)
        })
        .collect();

    RawState {
        axes,
        buttons,
        hats,
    }
}

fn convert_hat(value: HatState) -> HatDirection {
    match value {
        HatState::Centered => HatDirection::Centered,
        HatState::Up => HatDirection::Up,
        HatState::Right => HatDirection::Right,
        HatState::Down => HatDirection::Down,
        HatState::Left => HatDirection::Left,
        HatState::RightUp => HatDirection::RightUp,
        HatState::RightDown => HatDirection::RightDown,
        HatState::LeftUp => HatDirection::LeftUp,
        HatState::LeftDown => HatDirection::LeftDown,
    }
}

fn has_activity(previous: &RawState, current: &RawState) -> bool {
    if current.buttons != previous.buttons || current.hats != previous.hats {
        return true;
    }
    let max_axes = current.axes.len().max(previous.axes.len());
    (0..max_axes).any(|index| {
        let before = previous.axes.get(index).copied().unwrap_or_default() as i32;
        let now = current.axes.get(index).copied().unwrap_or_default() as i32;
        (now - before).abs() > 1_500
    })
}

fn is_virtual_output(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("xbox 360 controller for windows")
        || lower.contains("vigem")
        || lower.contains("virtual xbox")
}

fn update_snapshot(
    snapshot: &Arc<RwLock<RuntimeSnapshot>>,
    update: impl FnOnce(&mut RuntimeSnapshot),
) {
    if let Ok(mut state) = snapshot.write() {
        update(&mut state);
    }
}
