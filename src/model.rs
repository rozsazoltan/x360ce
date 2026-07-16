use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    pub instance_id: u32,
    pub name: String,
    pub guid: String,
    pub axes: u32,
    pub buttons: u32,
    pub hats: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HatDirection {
    #[default]
    Centered,
    Up,
    Right,
    Down,
    Left,
    RightUp,
    RightDown,
    LeftUp,
    LeftDown,
}

impl HatDirection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Centered => "Centered",
            Self::Up => "Up",
            Self::Right => "Right",
            Self::Down => "Down",
            Self::Left => "Left",
            Self::RightUp => "Up + Right",
            Self::RightDown => "Down + Right",
            Self::LeftUp => "Up + Left",
            Self::LeftDown => "Down + Left",
        }
    }

    pub fn contains(self, expected: Self) -> bool {
        match expected {
            Self::Up => matches!(self, Self::Up | Self::RightUp | Self::LeftUp),
            Self::Right => matches!(self, Self::Right | Self::RightUp | Self::RightDown),
            Self::Down => matches!(self, Self::Down | Self::RightDown | Self::LeftDown),
            Self::Left => matches!(self, Self::Left | Self::LeftUp | Self::LeftDown),
            _ => self == expected,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawState {
    #[serde(default)]
    pub axes: Vec<i16>,
    #[serde(default)]
    pub buttons: Vec<bool>,
    #[serde(default)]
    pub hats: Vec<HatDirection>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum InputBinding {
    #[default]
    None,
    Button {
        index: u32,
    },
    AxisPositive {
        index: u32,
    },
    AxisNegative {
        index: u32,
    },
    Hat {
        index: u32,
        direction: HatDirection,
    },
}

impl InputBinding {
    pub fn label(&self) -> String {
        match self {
            Self::None => "Not mapped".to_owned(),
            Self::Button { index } => format!("Button {}", index + 1),
            Self::AxisPositive { index } => format!("Axis {} +", index + 1),
            Self::AxisNegative { index } => format!("Axis {} -", index + 1),
            Self::Hat { index, direction } => {
                format!("Hat {} {}", index + 1, direction.label())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputControl {
    #[default]
    A,
    B,
    X,
    Y,
    LeftShoulder,
    RightShoulder,
    Back,
    Start,
    Guide,
    LeftThumb,
    RightThumb,
    DpadUp,
    DpadRight,
    DpadDown,
    DpadLeft,
    LeftTrigger,
    RightTrigger,
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
}

impl OutputControl {
    pub const BUTTONS: [Self; 15] = [
        Self::A,
        Self::B,
        Self::X,
        Self::Y,
        Self::LeftShoulder,
        Self::RightShoulder,
        Self::Back,
        Self::Start,
        Self::Guide,
        Self::LeftThumb,
        Self::RightThumb,
        Self::DpadUp,
        Self::DpadRight,
        Self::DpadDown,
        Self::DpadLeft,
    ];

    pub const ANALOGS: [Self; 6] = [
        Self::LeftTrigger,
        Self::RightTrigger,
        Self::LeftStickX,
        Self::LeftStickY,
        Self::RightStickX,
        Self::RightStickY,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::X => "X",
            Self::Y => "Y",
            Self::LeftShoulder => "Left shoulder",
            Self::RightShoulder => "Right shoulder",
            Self::Back => "Back",
            Self::Start => "Start",
            Self::Guide => "Guide",
            Self::LeftThumb => "Left stick click",
            Self::RightThumb => "Right stick click",
            Self::DpadUp => "D-pad up",
            Self::DpadRight => "D-pad right",
            Self::DpadDown => "D-pad down",
            Self::DpadLeft => "D-pad left",
            Self::LeftTrigger => "Left trigger",
            Self::RightTrigger => "Right trigger",
            Self::LeftStickX => "Left stick X",
            Self::LeftStickY => "Left stick Y",
            Self::RightStickX => "Right stick X",
            Self::RightStickY => "Right stick Y",
        }
    }

    pub fn is_axis(self) -> bool {
        matches!(
            self,
            Self::LeftStickX | Self::LeftStickY | Self::RightStickX | Self::RightStickY
        )
    }

    pub fn is_trigger(self) -> bool {
        matches!(self, Self::LeftTrigger | Self::RightTrigger)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MappingEntry {
    pub control: OutputControl,
    #[serde(default)]
    pub binding: InputBinding,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub centered_axis: bool,
    #[serde(default = "default_deadzone")]
    pub deadzone: f32,
    #[serde(default = "default_saturation")]
    pub saturation: f32,
}

fn default_deadzone() -> f32 {
    0.12
}

fn default_saturation() -> f32 {
    1.0
}

impl MappingEntry {
    pub fn new(control: OutputControl, binding: InputBinding) -> Self {
        Self {
            control,
            binding,
            invert: matches!(
                control,
                OutputControl::LeftStickY | OutputControl::RightStickY
            ),
            centered_axis: false,
            deadzone: default_deadzone(),
            saturation: default_saturation(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControllerProfile {
    pub name: String,
    pub device_guid: String,
    #[serde(default)]
    pub entries: Vec<MappingEntry>,
}

impl ControllerProfile {
    pub fn default_for(device_guid: String) -> Self {
        let entries = vec![
            MappingEntry::new(OutputControl::A, InputBinding::Button { index: 0 }),
            MappingEntry::new(OutputControl::B, InputBinding::Button { index: 1 }),
            MappingEntry::new(OutputControl::X, InputBinding::Button { index: 2 }),
            MappingEntry::new(OutputControl::Y, InputBinding::Button { index: 3 }),
            MappingEntry::new(
                OutputControl::LeftShoulder,
                InputBinding::Button { index: 4 },
            ),
            MappingEntry::new(
                OutputControl::RightShoulder,
                InputBinding::Button { index: 5 },
            ),
            MappingEntry::new(OutputControl::Back, InputBinding::Button { index: 6 }),
            MappingEntry::new(OutputControl::Start, InputBinding::Button { index: 7 }),
            MappingEntry::new(OutputControl::Guide, InputBinding::Button { index: 8 }),
            MappingEntry::new(OutputControl::LeftThumb, InputBinding::Button { index: 9 }),
            MappingEntry::new(
                OutputControl::RightThumb,
                InputBinding::Button { index: 10 },
            ),
            MappingEntry::new(
                OutputControl::DpadUp,
                InputBinding::Hat {
                    index: 0,
                    direction: HatDirection::Up,
                },
            ),
            MappingEntry::new(
                OutputControl::DpadRight,
                InputBinding::Hat {
                    index: 0,
                    direction: HatDirection::Right,
                },
            ),
            MappingEntry::new(
                OutputControl::DpadDown,
                InputBinding::Hat {
                    index: 0,
                    direction: HatDirection::Down,
                },
            ),
            MappingEntry::new(
                OutputControl::DpadLeft,
                InputBinding::Hat {
                    index: 0,
                    direction: HatDirection::Left,
                },
            ),
            MappingEntry::new(
                OutputControl::LeftTrigger,
                InputBinding::AxisPositive { index: 4 },
            ),
            MappingEntry::new(
                OutputControl::RightTrigger,
                InputBinding::AxisPositive { index: 5 },
            ),
            MappingEntry::new(
                OutputControl::LeftStickX,
                InputBinding::AxisPositive { index: 0 },
            ),
            MappingEntry::new(
                OutputControl::LeftStickY,
                InputBinding::AxisPositive { index: 1 },
            ),
            MappingEntry::new(
                OutputControl::RightStickX,
                InputBinding::AxisPositive { index: 2 },
            ),
            MappingEntry::new(
                OutputControl::RightStickY,
                InputBinding::AxisPositive { index: 3 },
            ),
        ];

        Self {
            name: "Default profile".to_owned(),
            device_guid,
            entries,
        }
    }

    pub fn entry(&self, control: OutputControl) -> Option<&MappingEntry> {
        self.entries.iter().find(|entry| entry.control == control)
    }

    pub fn entry_mut(&mut self, control: OutputControl) -> &mut MappingEntry {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.control == control)
        {
            return &mut self.entries[index];
        }
        self.entries
            .push(MappingEntry::new(control, InputBinding::None));
        self.entries.last_mut().expect("mapping entry was inserted")
    }
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeSnapshot {
    pub devices: Vec<DeviceDescriptor>,
    pub selected_instance_id: Option<u32>,
    pub raw_state: RawState,
    pub virtual_connected: bool,
    pub forwarding_active: bool,
    pub driver_installed: bool,
    pub last_error: Option<String>,
    pub last_controller_activity_sequence: u64,
}
