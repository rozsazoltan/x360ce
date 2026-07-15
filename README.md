# x360ce

`x360ce` is a minimal Windows controller mapper written in Rust. It reads physical DirectInput/XInput/HID-compatible controllers through SDL2, applies a saved visual mapping profile, and forwards input to one virtual Xbox 360 controller through ViGEmBus.

- [What it does](#what-it-does)
  - [Controller input](#controller-input)
  - [Visual mapping](#visual-mapping)
  - [Virtual Xbox 360 output](#virtual-xbox-360-output)
  - [Tray mode](#tray-mode)
  - [Portable profiles](#portable-profiles)
  - [Startup](#startup)
  - [Updates](#updates)
  - [Design assets](#design-assets)
- [Get started](#get-started)
- [Usage](#usage)
  - [Install virtual controller driver](#install-virtual-controller-driver)
  - [Select controller](#select-controller)
  - [Map controls](#map-controls)
  - [Tune axes and triggers](#tune-axes-and-triggers)
  - [Use tray mode](#use-tray-mode)
  - [Enable startup](#enable-startup)
  - [Check for updates](#check-for-updates)
- [Window behavior](#window-behavior)
- [Architecture](#architecture)
- [Repository layout](#repository-layout)
- [Data location](#data-location)
- [Build](#build)
- [Known limitations](#known-limitations)
- [Contributing](#contributing)
- [License and acknowledgments](#license-and-acknowledgments)
- [Development runner](#development-runner)
  - [WSL source and Windows runtime](#wsl-source-and-windows-runtime)

## What it does

x360ce runs as one portable Windows executable. Application code, SDL2 runtime, controller icon, update client, and ViGEmBus installer are embedded in or statically linked into that executable. ViGEmBus itself remains a Windows kernel driver and must be installed once with administrator approval.

### Controller input

x360ce uses SDL2 raw joystick APIs. This keeps arbitrary axes, buttons, and hat switches visible instead of forcing every physical device into a predefined gamepad layout.

Detected device metadata includes:

- stable SDL device GUID
- display name
- axis count
- button count
- hat count
- runtime instance ID

Known virtual Xbox output names are filtered from input discovery to avoid feeding x360ce output back into itself.

### Visual mapping

Main view draws Xbox 360 control layout. Click output control, choose **Learn input**, then move physical axis, press button, or move hat switch.

Supported output controls:

```text
A B X Y
Left / right shoulder
Left / right trigger
Back Start Guide
Left / right stick click
D-pad up / right / down / left
Left stick X / Y
Right stick X / Y
```

Each analog mapping stores:

- source input
- inversion
- deadzone
- saturation
- centered/split-axis mode for shared trigger axes

Live physical values and mapped output previews remain visible while editing.

### Virtual Xbox 360 output

Controller engine runs in dedicated polling thread. It:

1. enumerates physical controllers
2. polls selected device every 8 ms
3. applies active profile
4. creates virtual Xbox 360 target through ViGEmBus
5. submits updated XUSB report
6. keeps forwarding while window is visible or hidden in tray

Virtual target is removed when emulation is disabled, selected controller disconnects, driver becomes unavailable, or app exits.

### Tray mode

Closing window hides it instead of stopping controller engine. Tray icon remains active and offers:

- open window
- enable or pause virtual controller
- enable or disable automatic tray mode
- enable or disable Windows startup
- check and install updates
- quit app

By default, visible window starts inactivity countdown after 60 seconds without UI or controller activity. Countdown shows `10…9…8…`; any activity cancels it. When countdown finishes, only UI hides. Controller polling and virtual forwarding continue.

### Portable profiles

Profiles and settings stay beside executable:

```text
.x360ce-data/config.json
```

Config uses atomic replacement and temporary backup. Device profiles are keyed by SDL GUID. Moving executable together with `.x360ce-data` preserves mappings.

### Startup

Startup is optional. Production build writes current-user registry value:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
```

Command points to current executable and adds:

```text
--startup
```

Startup launch opens directly in tray. Move `x360ce.exe` to final folder before enabling startup.

### Updates

Production builds check GitHub releases for `rozsazoltan/x360ce`. Stable channel is default; prerelease channel can be enabled in settings.

Automatic checks run at most once per hour. Updater downloads Windows executable asset, replaces current executable from hidden PowerShell helper, restarts app, then removes temporary files.

Development builds disable update checks and executable replacement.

### Design assets

Canonical icon:

```text
assets/x360ce.svg
```

Generated Windows assets:

```text
assets/x360ce.png
assets/x360ce-tray.png
assets/x360ce.ico
```

Icon uses minimal controller silhouette with four face buttons. Same visual identity is used for executable, title bar, taskbar, and tray.

## Get started

Recommended layout:

```text
x360ce/
├─ x360ce.exe
└─ .x360ce-data/
   └─ config.json
```

1. Move `x360ce.exe` to final writable folder.
2. Run it.
3. Select **Install ViGEmBus** when driver is missing.
4. Approve administrator prompt and finish driver install.
5. Restart x360ce.
6. Connect physical controller.
7. Select controller and map controls.
8. Keep window open or hide it to tray.

## Usage

### Install virtual controller driver

x360ce embeds official ViGEmBus `1.22.0` bootstrapper:

```text
assets/third-party/ViGEmBus_1.22.0_x64_x86_arm64.exe
```

**Install ViGEmBus** extracts fresh bootstrapper bytes to process-specific temporary folder and launches it with Windows elevation prompt. Driver installation is system-level and cannot be made portable inside user-mode executable.

Restart x360ce after install so it can reconnect and create virtual target.

### Select controller

Use **Input controller** selector. Device list refreshes automatically every two seconds. **Refresh** forces immediate enumeration.

Current selection is stored by device GUID. When controller reconnects, engine opens first matching non-virtual device.

### Map controls

1. Click output control in controller graphic or mapping list.
2. Click **Learn input**.
3. Press button, move axis beyond threshold, or move hat.
4. Mapping is saved immediately.

Use **Clear** to remove selected mapping.

Default profile assumes common SDL order:

```text
Axes 1–4: left X/Y, right X/Y
Axes 5–6: left/right trigger
Buttons 1–4: A/B/X/Y
Buttons 5–6: shoulders
Buttons 7–11: Back/Start/Guide/L3/R3
Hat 1: D-pad
```

Devices differ. Learn mode is preferred over relying on defaults.

### Tune axes and triggers

Analog controls expose:

```text
Invert
Deadzone: 0.00–0.50
Saturation: 0.50–1.00
```

Deadzone removes center noise. Saturation reaches full virtual value before physical maximum. Invert reverses direction.

SDL commonly reports standalone triggers as `-32768` at rest and `32767` fully pressed. Trigger mapper converts this bipolar range to Xbox `0–255`. For DirectInput devices where two triggers share one axis centered at zero, enable **Centered/split source axis**. Learn mode detects this from resting value when possible.

### Use tray mode

Left click tray icon to open window. Right click for actions.

Closing window or choosing **Hide to tray** keeps process running. Use **Quit x360ce** to remove virtual controller and stop process.

Automatic tray behavior defaults:

```text
Idle: 60 seconds
Countdown: 10 seconds
```

Both values are configurable.

### Enable startup

Use settings panel or tray menu. Development builds do not write startup registry entry.

Startup command:

```text
"C:\path\to\x360ce.exe" --startup
```

### Check for updates

Production only. Use settings panel or tray menu.

Expected release asset:

```text
x360ce-v<version>-windows-x64.exe
```

Updater accepts only `x360ce.exe`, `x360ce-windows-x64.exe`, or versioned `x360ce-*-windows-x64.exe` assets and rejects downloads without Windows PE `MZ` header.

## Window behavior

UI uses `egui`/`eframe` with OpenGL glow renderer. Window is DPI-aware and resizable.

Development title:

```text
x360ce v0.1.0-dev
```

Production title:

```text
x360ce v0.1.0
```

Only one instance can run. Starting another copy activates existing window when possible and shows native notice describing development/production conflict.

Closing window hides app to tray. Controller engine is independent from window visibility.

## Architecture

UI and desktop integration:

```text
eframe 0.35.0
egui 0.35.0
tray-icon 0.24.1
windows-sys 0.61.2
```

Controller pipeline:

```text
SDL2 0.38.0 raw joystick input
mapper.rs profile transform
vigem-rust 0.1.1 Xbox 360 target
ViGEmBus 1.22.0 kernel driver
```

Other components:

```text
serde / serde_json portable config
crossbeam-channel engine and updater messages
reqwest 0.13.4 GitHub release client
semver release comparison
winresource 0.1.31 Windows resources
```

Thread boundaries:

```text
UI thread
├─ egui rendering
├─ tray menu
├─ settings persistence
├─ update orchestration
└─ inactivity countdown

Controller engine thread
├─ SDL initialization
├─ device enumeration
├─ selected joystick ownership
├─ raw input polling
├─ profile mapping
└─ ViGEm virtual target ownership
```

SDL joystick types remain on engine thread because SDL subsystem and open joystick handles are not `Send` or `Sync`.

## Repository layout

```text
x360ce/
├─ .cargo/
│  └─ config.toml
├─ .github/workflows/
│  ├─ ci.yml
│  └─ release.yml
├─ assets/
│  ├─ third-party/
│  │  └─ ViGEmBus_1.22.0_x64_x86_arm64.exe
│  ├─ x360ce.svg
│  ├─ x360ce.png
│  ├─ x360ce-tray.png
│  └─ x360ce.ico
├─ scripts/
│  ├─ dev.cmd
│  ├─ dev.ps1
│  ├─ dev.sh
│  ├─ dev-win.ps1
│  ├─ release.ps1
│  ├─ setup-mutagen-wsl-dev.cmd
│  └─ setup-mutagen-wsl-dev.ps1
├─ src/
│  ├─ bin/x360ce-dev.rs
│  ├─ config.rs
│  ├─ driver.rs
│  ├─ engine.rs
│  ├─ main.rs
│  ├─ mapper.rs
│  ├─ model.rs
│  ├─ single_instance.rs
│  ├─ startup.rs
│  ├─ updater.rs
│  ├─ virtual_gamepad.rs
│  ├─ wide.rs
│  ├─ win_app.rs
│  └─ x360ce.exe.manifest
├─ build.rs
├─ Cargo.toml
├─ CONTRIBUTING.md
├─ LICENSE
├─ README.md
└─ THIRD_PARTY_NOTICES.md
```

## Data location

Default:

```text
<executable-folder>\.x360ce-data\config.json
```

Development or portable test override:

```powershell
$env:X360CE_APP_DATA_DIR = "C:\path\to\x360ce-data"
```

Saved config includes:

- emulation enabled state
- selected device GUID
- profile per device GUID
- mapping inputs
- axis inversion, deadzone, saturation
- automatic tray timing
- start-in-tray preference
- startup state
- update channel and last check timestamp
- last status

## Build

Requirements:

```text
Windows 10/11 x64
Rust 1.92 or newer
MSVC build tools
PowerShell 7 recommended for release script
```

Development build:

```powershell
cargo build --bin x360ce
```

Run once:

```powershell
cargo run --bin x360ce
```

Polling dev runner:

```powershell
cargo dev
```

Release build:

```powershell
cargo build --release --target x86_64-pc-windows-msvc --bin x360ce
```

Repository release helper:

```powershell
# Validate and build current Cargo.toml version locally.
scripts\release.ps1 -Version 0.1.0

# Dispatch stable GitHub release workflow.
scripts\release.ps1 -Version 0.2.0 -Publish

# Dispatch prerelease workflow. Version may use SemVer prerelease suffix.
scripts\release.ps1 -Version 0.2.0-beta.1 -Prerelease -Publish
```

GitHub Actions `release.yml` exposes required `version` and `prerelease` inputs. It updates Cargo and manifest versions, regenerates `Cargo.lock`, builds merged default-branch commit, creates tagged Windows asset, and marks GitHub release as prerelease when selected.

Generated executable:

```text
target\x86_64-pc-windows-msvc\release\x360ce.exe
```

Because `sdl2` uses `bundled` and `static-link`, SDL2 is built into executable. ViGEm client implementation is pure Rust. Driver installer bytes are embedded by `include_bytes!`.

## Known limitations

x360ce is Windows-only.

ViGEmBus driver must be installed separately with administrator approval. One user-mode executable cannot itself replace signed kernel driver installation.

ViGEm project is archived. Included installer is retained for compatibility. Future Windows changes may require migration to another virtual gamepad driver.

Current implementation creates one virtual Xbox 360 controller and maps one selected physical controller.

Profiles use SDL GUID. Two identical controllers can share same GUID and therefore share profile; first matching device is selected after reconnect.

Rumble output is not forwarded back to physical controller yet.

Some games enumerate controllers only at startup. Restart game after enabling virtual controller.

Physical controller remains visible to games. Games that read both physical and virtual devices can receive duplicate input. HidHide integration is not bundled.

Self-update replaces executable in place. Executable folder must be writable by current user.

Published executable and embedded driver installer should be code-signed. Release pipeline does not include signing secrets by default.

## Contributing

Issues and pull requests are welcome. Keep changes small, focused, testable, and easy to review.

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License and acknowledgments

Copyright (C) 2026–present [Zoltán Rózsa](https://github.com/rozsazoltan)

x360ce Rust application is licensed under GNU Affero General Public License v3.0 or later (`AGPL-3.0-or-later`). See [LICENSE](LICENSE).

ViGEmBus installer is third-party software under BSD 3-Clause license. `vigem-rust` is MIT OR Apache-2.0. SDL2 and Rust bindings have their own licenses. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

This repository is clean-room Rust implementation inspired by controller-mapping behavior. It does not copy source code from `x360ce/x360ce` or `hifihedgehog/x360ce`.

## Development runner

Run project-local watcher:

```powershell
scripts\dev.ps1
```

Or:

```cmd
scripts\dev.cmd
```

Unix-like shell:

```sh
scripts/dev.sh
```

Dev runner builds `x360ce`, opens app window, watches source and assets, stops previous child process, and restarts after changes. No external `cargo-watch` dependency.

Development builds:

- always open main window
- use `v<version>-dev` title
- disable startup writes
- disable GitHub update checks
- use same `.x360ce-data` model unless `X360CE_APP_DATA_DIR` overrides it

### WSL source and Windows runtime

Keep source in WSL and mirror it to Windows with Mutagen. Run from Windows PowerShell through WSL UNC checkout:

```powershell
scripts\setup-mutagen-wsl-dev.ps1
```

Setup expects `mutagen.exe` on Windows `PATH`, starts the daemon, and creates the sync session. It does not download or install Mutagen.

Then run Windows-side watcher:

```powershell
scripts\dev-win.ps1
```

This preserves Linux-native source workflow while compiling and running Windows UI, SDL2, tray, registry, and ViGEm integrations on Windows.
