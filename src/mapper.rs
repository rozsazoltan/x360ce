use crate::model::{
    ControllerProfile, HatDirection, InputBinding, MappingEntry, OutputControl, RawState,
};
use vigem_rust::{X360Button, X360Report};

const LEARN_AXIS_THRESHOLD: i16 = 16_000;
const BUTTON_AXIS_THRESHOLD: f32 = 0.55;

pub fn detect_binding(previous: &RawState, current: &RawState) -> Option<InputBinding> {
    for (index, pressed) in current.buttons.iter().copied().enumerate() {
        let was_pressed = previous.buttons.get(index).copied().unwrap_or(false);
        if pressed && !was_pressed {
            return Some(InputBinding::Button {
                index: index as u32,
            });
        }
    }

    for (index, value) in current.axes.iter().copied().enumerate() {
        let previous_value = previous.axes.get(index).copied().unwrap_or_default();
        if value >= LEARN_AXIS_THRESHOLD && previous_value < LEARN_AXIS_THRESHOLD {
            return Some(InputBinding::AxisPositive {
                index: index as u32,
            });
        }
        if value <= -LEARN_AXIS_THRESHOLD && previous_value > -LEARN_AXIS_THRESHOLD {
            return Some(InputBinding::AxisNegative {
                index: index as u32,
            });
        }
    }

    for (index, direction) in current.hats.iter().copied().enumerate() {
        let previous_direction = previous
            .hats
            .get(index)
            .copied()
            .unwrap_or(HatDirection::Centered);
        if direction != HatDirection::Centered && direction != previous_direction {
            return Some(InputBinding::Hat {
                index: index as u32,
                direction,
            });
        }
    }

    None
}

pub fn map_report(profile: &ControllerProfile, raw: &RawState) -> X360Report {
    let mut report = X360Report::default();

    for control in OutputControl::BUTTONS {
        let Some(entry) = profile.entry(control) else {
            continue;
        };
        if binding_pressed(entry, raw) {
            report.buttons.insert(button_flag(control));
        }
    }

    report.left_trigger = profile
        .entry(OutputControl::LeftTrigger)
        .map(|entry| trigger_value(entry, raw))
        .unwrap_or_default();
    report.right_trigger = profile
        .entry(OutputControl::RightTrigger)
        .map(|entry| trigger_value(entry, raw))
        .unwrap_or_default();
    report.thumb_lx = profile
        .entry(OutputControl::LeftStickX)
        .map(|entry| axis_value(entry, raw))
        .unwrap_or_default();
    report.thumb_ly = profile
        .entry(OutputControl::LeftStickY)
        .map(|entry| axis_value(entry, raw))
        .unwrap_or_default();
    report.thumb_rx = profile
        .entry(OutputControl::RightStickX)
        .map(|entry| axis_value(entry, raw))
        .unwrap_or_default();
    report.thumb_ry = profile
        .entry(OutputControl::RightStickY)
        .map(|entry| axis_value(entry, raw))
        .unwrap_or_default();

    report
}

pub fn binding_pressed(entry: &MappingEntry, raw: &RawState) -> bool {
    match entry.binding {
        InputBinding::None => false,
        InputBinding::Button { index } => raw
            .buttons
            .get(index as usize)
            .copied()
            .unwrap_or(false),
        InputBinding::AxisPositive { index } => {
            normalized_axis(raw, index) > BUTTON_AXIS_THRESHOLD
        }
        InputBinding::AxisNegative { index } => {
            normalized_axis(raw, index) < -BUTTON_AXIS_THRESHOLD
        }
        InputBinding::Hat { index, direction } => raw
            .hats
            .get(index as usize)
            .copied()
            .unwrap_or(HatDirection::Centered)
            .contains(direction),
    }
}

pub fn axis_preview(entry: &MappingEntry, raw: &RawState) -> f32 {
    let mut value = match entry.binding {
        InputBinding::None => 0.0,
        InputBinding::Button { index } => {
            if raw
                .buttons
                .get(index as usize)
                .copied()
                .unwrap_or(false)
            {
                1.0
            } else {
                0.0
            }
        }
        InputBinding::AxisPositive { index } => normalized_axis(raw, index),
        InputBinding::AxisNegative { index } => -normalized_axis(raw, index),
        InputBinding::Hat { index, direction } => {
            if raw
                .hats
                .get(index as usize)
                .copied()
                .unwrap_or(HatDirection::Centered)
                .contains(direction)
            {
                1.0
            } else {
                0.0
            }
        }
    };

    if entry.invert {
        value = -value;
    }

    apply_deadzone(value, entry.deadzone, entry.saturation)
}

fn axis_value(entry: &MappingEntry, raw: &RawState) -> i16 {
    let value = axis_preview(entry, raw).clamp(-1.0, 1.0);
    if value < 0.0 {
        (value * 32_768.0).round().clamp(-32_768.0, 0.0) as i16
    } else {
        (value * 32_767.0).round().clamp(0.0, 32_767.0) as i16
    }
}

pub fn trigger_preview(entry: &MappingEntry, raw: &RawState) -> f32 {
    let mut value = match entry.binding {
        InputBinding::None => 0.0,
        InputBinding::Button { index } => {
            if raw
                .buttons
                .get(index as usize)
                .copied()
                .unwrap_or(false)
            {
                1.0
            } else {
                0.0
            }
        }
        InputBinding::AxisPositive { index } => {
            let value = normalized_axis(raw, index);
            if entry.centered_axis {
                value.max(0.0)
            } else {
                ((value + 1.0) * 0.5).clamp(0.0, 1.0)
            }
        }
        InputBinding::AxisNegative { index } => {
            let value = -normalized_axis(raw, index);
            if entry.centered_axis {
                value.max(0.0)
            } else {
                ((value + 1.0) * 0.5).clamp(0.0, 1.0)
            }
        }
        InputBinding::Hat { index, direction } => {
            if raw
                .hats
                .get(index as usize)
                .copied()
                .unwrap_or(HatDirection::Centered)
                .contains(direction)
            {
                1.0
            } else {
                0.0
            }
        }
    };

    if entry.invert {
        value = 1.0 - value;
    }
    apply_trigger_deadzone(value, entry.deadzone, entry.saturation)
}

fn trigger_value(entry: &MappingEntry, raw: &RawState) -> u8 {
    (trigger_preview(entry, raw) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn normalized_axis(raw: &RawState, index: u32) -> f32 {
    let value = raw.axes.get(index as usize).copied().unwrap_or_default();
    if value < 0 {
        value as f32 / 32_768.0
    } else {
        value as f32 / 32_767.0
    }
}

fn apply_deadzone(value: f32, deadzone: f32, saturation: f32) -> f32 {
    let deadzone = deadzone.clamp(0.0, 0.95);
    let saturation = saturation.clamp(deadzone + 0.01, 1.0);
    let magnitude = value.abs();
    if magnitude <= deadzone {
        return 0.0;
    }
    let scaled = ((magnitude - deadzone) / (saturation - deadzone)).clamp(0.0, 1.0);
    scaled.copysign(value)
}

fn apply_trigger_deadzone(value: f32, deadzone: f32, saturation: f32) -> f32 {
    let deadzone = deadzone.clamp(0.0, 0.95);
    let saturation = saturation.clamp(deadzone + 0.01, 1.0);
    if value <= deadzone {
        0.0
    } else {
        ((value - deadzone) / (saturation - deadzone)).clamp(0.0, 1.0)
    }
}

fn button_flag(control: OutputControl) -> X360Button {
    match control {
        OutputControl::A => X360Button::A,
        OutputControl::B => X360Button::B,
        OutputControl::X => X360Button::X,
        OutputControl::Y => X360Button::Y,
        OutputControl::LeftShoulder => X360Button::LEFT_SHOULDER,
        OutputControl::RightShoulder => X360Button::RIGHT_SHOULDER,
        OutputControl::Back => X360Button::BACK,
        OutputControl::Start => X360Button::START,
        OutputControl::Guide => X360Button::GUIDE,
        OutputControl::LeftThumb => X360Button::LEFT_THUMB,
        OutputControl::RightThumb => X360Button::RIGHT_THUMB,
        OutputControl::DpadUp => X360Button::DPAD_UP,
        OutputControl::DpadRight => X360Button::DPAD_RIGHT,
        OutputControl::DpadDown => X360Button::DPAD_DOWN,
        OutputControl::DpadLeft => X360Button::DPAD_LEFT,
        _ => X360Button::empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_new_button_press() {
        let previous = RawState {
            buttons: vec![false],
            ..Default::default()
        };
        let current = RawState {
            buttons: vec![true],
            ..Default::default()
        };
        assert_eq!(
            detect_binding(&previous, &current),
            Some(InputBinding::Button { index: 0 })
        );
    }

    #[test]
    fn deadzone_removes_center_noise() {
        assert_eq!(apply_deadzone(0.05, 0.12, 1.0), 0.0);
        assert!(apply_deadzone(0.75, 0.12, 1.0) > 0.6);
    }

    #[test]
    fn bipolar_trigger_is_zero_at_negative_rest() {
        let mut entry = MappingEntry::new(
            OutputControl::LeftTrigger,
            InputBinding::AxisPositive { index: 0 },
        );
        entry.deadzone = 0.0;
        let raw = RawState {
            axes: vec![i16::MIN],
            ..Default::default()
        };
        assert_eq!(trigger_preview(&entry, &raw), 0.0);
    }

    #[test]
    fn centered_trigger_uses_one_axis_half() {
        let mut entry = MappingEntry::new(
            OutputControl::LeftTrigger,
            InputBinding::AxisNegative { index: 0 },
        );
        entry.centered_axis = true;
        entry.deadzone = 0.0;
        let raw = RawState {
            axes: vec![-16_384],
            ..Default::default()
        };
        assert!(trigger_preview(&entry, &raw) > 0.49);
    }
}
