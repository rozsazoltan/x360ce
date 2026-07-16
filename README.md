# x360ce

x360ce lets a Windows controller behave like an Xbox 360 controller. It detects physical gamepads, lets you map their buttons and axes visually, then forwards the result to a virtual Xbox 360 controller.

## Features

- Visual Xbox 360 controller layout
- Button, trigger, D-pad, stick-axis, and stick-click mapping
- Double-click learning from the controller layout or mapping list
- Positive and negative axis-direction detection
- Axis inversion, deadzone, and saturation controls
- Live physical input preview
- Per-controller profiles
- Individual mapping removal
- JSON profile import and export
- Tray mode with optional forwarding only while hidden
- Automatic hide-to-tray countdown
- Optional Windows startup
- Embedded ViGEmBus installer
- In-app update controls
- Xbox Game Bar Guide/Nexus suppression

## Requirements

- Windows 10 or Windows 11, 64-bit
- A supported physical controller
- ViGEmBus for virtual Xbox 360 output

ViGEmBus installation requires administrator approval once. x360ce can start the bundled installer when the driver is missing.

## Installation

1. Download the latest Windows executable from the Releases page.
2. Move `x360ce.exe` to its final folder.
3. Run `x360ce.exe`.
4. Select **Install ViGEmBus** when prompted.
5. Complete the driver installation and restart x360ce.
6. Connect the physical controller.

Keep the application in a writable folder so settings, profiles, and updates can be stored beside the executable.

## Quick start

1. Choose a device under **Input controller**.
2. Select a control on the virtual Xbox 360 layout or in the mapping list.
3. Double-click it, or press **Learn**.
4. Press the physical button or move the physical axis.
5. Repeat for the remaining controls.
6. Hide x360ce to the tray to enable normal virtual-controller forwarding when tray-only isolation is enabled.

Press **Esc** at any time to cancel learning.

## Mapping controls

### Buttons and D-pad

Double-click a virtual control or mapping row, then press the corresponding physical button or D-pad direction.

Mapped controls appear with their assigned input. Unmapped controls remain available, appear dimmed, and show a warning marker. An unmapped control is allowed; controllers do not need to expose every Xbox 360 input.

Use **Clear** or the row-level remove action to delete one mapping.

### Sticks and axes

Stick axes are mapped by direction. Learning detects whether the physical movement came from the positive or negative side of an axis.

Available tuning options:

- **Source direction** — choose positive or negative physical movement
- **Invert output** — reverse output direction
- **Deadzone** — ignore small movement near center
- **Saturation** — reach full output before physical maximum
- **Centered/split source axis** — support devices where two triggers share one centered axis

The virtual layout shows stick directions separately and indicates stick-click input as `L3` or `R3`.

### Triggers

Triggers normally map from a dedicated physical axis. Some DirectInput controllers expose both triggers through one shared axis. Enable **Centered/split source axis** when required.

## Live physical input

The **Live physical input** panel shows current axes, buttons, and hats from the selected controller. Use it to identify device numbering before or during mapping.

Opening the configuration window can pause virtual output when tray-only forwarding is enabled. This prevents test presses from controlling Windows or another application while mappings are being edited.

## Tray mode

Closing the window hides x360ce instead of stopping it.

The tray menu provides access to:

- Open x360ce
- Enable or pause the virtual controller
- Enable or disable automatic tray mode
- Enable or disable Windows startup
- Check for updates
- Install an available update
- Quit x360ce

When **Forward only in tray** is enabled:

- Physical input remains visible for testing while the window is open.
- Virtual Xbox 360 output is paused during configuration.
- Virtual forwarding resumes after x360ce is hidden to the tray.

Use **Quit x360ce** when the virtual controller should be removed completely.

## Automatic tray mode

x360ce can hide itself after a configurable idle period. A countdown appears before the window is hidden. Mouse, keyboard, or controller activity cancels the countdown.

Default behavior:

- Idle period: 60 seconds
- Countdown: 10 seconds

Both values can be changed in Settings.

## Profiles, export, and import

Settings and controller profiles are stored beside the executable:

```text
.x360ce-data\config.json
```

Profiles are associated with the controller device GUID. Identical controllers can share the same GUID and therefore the same profile.

Use **Export** to save the selected controller profile as JSON. Use **Import** to load a previously exported profile.

Back up both `x360ce.exe` and `.x360ce-data` when moving the application to another folder or computer.

## Start with Windows

Enable **Start with Windows** in Settings or from the tray menu. Startup launches x360ce directly into the tray.

Move `x360ce.exe` to its final location before enabling startup. Moving the executable afterward can leave the old startup path registered.

## Updates

Production builds can check the Releases page for updates. Stable releases are used by default; prereleases can be enabled in Settings.

The executable folder must be writable for in-place updates. Development builds intentionally disable update checking and replacement.

## Xbox Game Bar behavior

x360ce suppresses virtual Guide/Nexus output and installs silent handlers for the Windows `ms-gamebar` and `ms-gamebarservices` protocols. This prevents Xbox Game Bar or the missing-application dialog from opening when the Guide button is pressed.

The silent handler may start a background x360ce process that exits immediately. It does not open a window or run a command script.

## Troubleshooting

### Virtual controller does not appear

- Install ViGEmBus from the x360ce Settings panel.
- Restart x360ce after installation.
- Confirm that virtual-controller emulation is enabled.
- Hide x360ce to the tray when **Forward only in tray** is enabled.
- Restart games that were already open; some games enumerate controllers only at startup.

### Physical controller is not listed

- Reconnect the controller.
- Press **Refresh** beside the controller selector.
- Close other controller tools that may hold exclusive access.
- Try another USB port or Bluetooth reconnection.

### Axis moves in the wrong direction

Change **Source direction** or enable **Invert output** for that axis mapping.

### Trigger behaves like a centered axis

Enable **Centered/split source axis** for the trigger mapping.

### Inputs are duplicated in games

The physical controller remains visible to Windows while x360ce creates a second virtual controller. Some games read both devices. HidHide or another device-hiding solution may be required for complete physical-device isolation.

### Xbox Game Bar dialog still appears

1. Quit all running x360ce instances.
2. Start the latest x360ce build once so the silent protocol handlers are registered.
3. Hide the app to the tray and test the Guide/Nexus button again.

### Settings do not persist

Place x360ce in a folder writable by the current user. Avoid protected locations such as `C:\Program Files` unless permissions are configured appropriately.

## Current limitations

- Windows only
- One selected physical controller
- One virtual Xbox 360 controller
- No rumble forwarding to the physical controller yet
- Physical-device hiding requires a separate solution such as HidHide
- ViGEmBus is a separate Windows kernel driver and requires administrator approval

## Original project

This Rust implementation is inspired by the original [`x360ce/x360ce`](https://github.com/x360ce/x360ce) project. No source code is copied from the original implementation.

## License

x360ce is licensed under the GNU Affero General Public License v3.0 or later. See [LICENSE](LICENSE).

Third-party components keep their respective licenses. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Development and contribution documentation is available in [CONTRIBUTING.md](CONTRIBUTING.md).
