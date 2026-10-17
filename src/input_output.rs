use crate::{
    mapper,
    model::{ControllerProfile, MouseButton, OutputAction, RawState},
};
use std::collections::HashSet;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
};

const MOUSE_SPEED: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HeldOutput {
    Key(u16),
    Mouse(MouseButton),
}

#[derive(Default)]
pub struct InputOutput {
    held: HashSet<HeldOutput>,
    fractional_x: f32,
    fractional_y: f32,
}

impl InputOutput {
    pub fn update(&mut self, profile: &ControllerProfile, raw: &RawState, active: bool) {
        let mut next_held = HashSet::new();
        let mut dx = 0.0;
        let mut dy = 0.0;

        if active {
            for entry in &profile.entries {
                match entry.output {
                    OutputAction::None => {}
                    OutputAction::Key { virtual_key } if mapper::binding_pressed(entry, raw) => {
                        next_held.insert(HeldOutput::Key(virtual_key));
                    }
                    OutputAction::MouseButton { button } if mapper::binding_pressed(entry, raw) => {
                        next_held.insert(HeldOutput::Mouse(button));
                    }
                    OutputAction::MouseX => {
                        dx += mapper::axis_preview(entry, raw) * MOUSE_SPEED;
                    }
                    OutputAction::MouseY => {
                        dy += mapper::axis_preview(entry, raw) * MOUSE_SPEED;
                    }
                    _ => {}
                }
            }
        }

        for output in self
            .held
            .difference(&next_held)
            .copied()
            .collect::<Vec<_>>()
        {
            send_held(output, false);
        }
        for output in next_held
            .difference(&self.held)
            .copied()
            .collect::<Vec<_>>()
        {
            send_held(output, true);
        }
        self.held = next_held;

        if active {
            self.fractional_x += dx;
            self.fractional_y += dy;
            let move_x = self.fractional_x.trunc() as i32;
            let move_y = self.fractional_y.trunc() as i32;
            self.fractional_x -= move_x as f32;
            self.fractional_y -= move_y as f32;
            if move_x != 0 || move_y != 0 {
                send_mouse_move(move_x, move_y);
            }
        } else {
            self.fractional_x = 0.0;
            self.fractional_y = 0.0;
        }
    }
}

impl Drop for InputOutput {
    fn drop(&mut self) {
        for output in self.held.drain() {
            send_held(output, false);
        }
    }
}

fn send_held(output: HeldOutput, pressed: bool) {
    let input = match output {
        HeldOutput::Key(virtual_key) => INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: virtual_key,
                    dwFlags: if pressed { 0 } else { KEYEVENTF_KEYUP },
                    ..Default::default()
                },
            },
        },
        HeldOutput::Mouse(button) => INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dwFlags: match (button, pressed) {
                        (MouseButton::Left, true) => MOUSEEVENTF_LEFTDOWN,
                        (MouseButton::Left, false) => MOUSEEVENTF_LEFTUP,
                        (MouseButton::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                        (MouseButton::Right, false) => MOUSEEVENTF_RIGHTUP,
                        (MouseButton::Middle, true) => MOUSEEVENTF_MIDDLEDOWN,
                        (MouseButton::Middle, false) => MOUSEEVENTF_MIDDLEUP,
                    },
                    ..Default::default()
                },
            },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
}

fn send_mouse_move(dx: i32, dy: i32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                dwFlags: MOUSEEVENTF_MOVE,
                ..Default::default()
            },
        },
    };
    unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
}
