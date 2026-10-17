# Contributing to x360ce

Thank you for contributing. Keep changes focused, testable, and easy to review.

User-facing installation and usage documentation belongs in [README.md](README.md). Build instructions, architecture notes, repository internals, release procedures, and engineering decisions belong here.

## Project principles

- Preserve a simple user workflow.
- Keep controller polling deterministic and responsive.
- Avoid blocking work on the UI thread.
- Keep mappings portable and backward compatible.
- Keep Windows-specific behavior isolated behind Windows modules.
- Prefer small changes with explicit manual validation.
- Do not copy source code from the original [`x360ce/x360ce`](https://github.com/x360ce/x360ce) project.

## Development requirements

- Windows 10 or Windows 11, 64-bit
- [mise](https://mise.jdx.dev/) to install the tools declared in `mise.toml`
- Visual Studio 2022 Build Tools with the **Desktop development with C++** workload and a Windows SDK
- ViGEmBus, installed separately, for virtual-controller runtime testing; mise does not install this external driver
- A physical controller for end-to-end mapping testing

## Build

Install the configured tools, then run the application:

```powershell
mise install
cargo run
```

Activate mise in your shell or enable its shims on `PATH` before running Cargo. On a fresh checkout, run `mise trust` if mise requests approval for the project configuration.

`cargo run` builds and starts the app; it does not watch files or restart automatically. Debug builds open the main window, disable startup registry writes and update checks, and use portable data unless `X360CE_APP_DATA_DIR` is set.

Build an optimized release executable:

```powershell
cargo build --release
```

Generated release executable:

```text
target\release\x360ce.exe
```

The project uses the default Windows MSVC target. SDL2 uses bundled static linking. The ViGEm client is implemented through `vigem-rust`; ViGEmBus itself remains an external Windows kernel driver.

After dependency changes, regenerate and commit the lockfile:

```powershell
cargo generate-lockfile
```

## Repository layout

```text
x360ce/
├─ .cargo/
│  └─ config.toml
├─ .github/workflows/
│  ├─ ci.yml
│  └─ release.yml
├─ mise.toml
├─ assets/
│  ├─ third-party/
│  │  └─ ViGEmBus_1.22.0_x64_x86_arm64.bin
│  ├─ x360ce.png
│  ├─ x360ce-tray.png
│  └─ x360ce.ico
├─ src/
│  ├─ config.rs
│  ├─ driver.rs
│  ├─ engine.rs
│  ├─ game_bar.rs
│  ├─ input_output.rs
│  ├─ main.rs
│  ├─ mapper.rs
│  ├─ model.rs
│  ├─ profile_io.rs
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

## Architecture

### UI thread

The UI thread owns:

- `egui` and `eframe` rendering
- tray menu and tray icon
- settings persistence
- update orchestration
- inactivity countdown
- Windows shell and registry integration initiated by the UI

Do not perform blocking controller polling or long network operations directly in UI callbacks.

### Controller engine thread

The controller engine thread owns:

- SDL initialization
- physical-device enumeration
- selected joystick handle
- raw input polling
- mapping transformation
- ViGEm virtual target
- forwarding enable/disable state

SDL subsystem and joystick handles must stay on the engine thread. Do not move SDL-owned types across threads.

### State flow

The UI sends commands and immutable profile snapshots to the engine. The engine returns runtime snapshots through channels. Persist only serializable domain state.

Core flow:

```text
physical controller
    ↓ SDL2 raw joystick input
RawState
    ↓ profile mapping
virtual Xbox 360 report
    ↓ ViGEm client
ViGEmBus virtual controller
```

## Mapping model

Mappings support:

- physical buttons
- positive axis directions
- negative axis directions
- hat directions
- output inversion
- deadzone
- saturation
- centered/split trigger axes

Axis learning must retain direction. A positive movement and a negative movement on the same physical axis are distinct mapping inputs.

Unmapped controls are valid. UI should show them as incomplete without treating the profile as invalid.

Profile import and export must preserve all mapping fields and reject malformed or incompatible input safely.

## UI rules

- Keep text readable at Windows scaling levels.
- Avoid overlapping labels and controls.
- Use stable panel dimensions; live input must not cause layout jumping.
- Keep the complete mapping editor reachable without unnecessary nested scrolling.
- Single click selects a mapping target.
- Double click starts learning.
- `Esc` cancels learning.
- Blue selection state is reserved for active mapping/learning interaction.
- Active physical input uses green feedback.
- Mapped inactive controls use high-contrast neutral styling.
- Unmapped controls use dim warning styling without blocking normal use.
- Direction icons should be vector-drawn rather than relying on font-specific Unicode glyphs.
- The virtual controller layout and mapping list must stay synchronized.

## Tray and forwarding isolation

Default configuration can pause virtual output while the window is visible and resume forwarding in tray mode. This allows physical inputs to be tested without controlling applications through the virtual device.

When changing window or tray behavior, validate:

- opening the window pauses forwarding when configured
- hiding to tray resumes forwarding
- closing the window does not terminate the engine
- quitting removes the virtual controller
- inactivity countdown can be cancelled by UI or controller activity

## Xbox Game Bar handling

Windows can handle a physical Guide/Nexus button independently from the virtual report. The application therefore uses multiple protections:

- virtual Guide/Nexus output suppression
- Game Bar controller-shortcut registry configuration
- silent `ms-gamebar` protocol handler
- silent `ms-gamebarservices` protocol handler
- immediate no-op exit when launched as the protocol sink

The application uses the Windows GUI subsystem in both development and release builds so protocol-sink launches do not display a console window.

Do not replace the protocol sink with shell scripts, PowerShell windows, or visible helper processes.

## Portable data

Default data path:

```text
<executable-folder>\.x360ce-data\config.json
```

Development override:

```powershell
$env:X360CE_APP_DATA_DIR = "C:\path\to\x360ce-data"
```

Configuration writes should use atomic replacement where possible. New fields must have serde defaults so old profiles remain loadable.

## Updater

Production builds may check GitHub Releases, compare semantic versions, download a Windows executable asset, validate the PE header, replace the current executable, and restart.

Development builds must not perform update checks or executable replacement.

Accepted release asset naming patterns:

```text
x360ce.exe
x360ce-windows-x64.exe
x360ce-v<version>-windows-x64.exe
```

Updater changes must preserve:

- TLS validation
- semantic-version comparison
- Windows PE `MZ` validation
- temporary-file cleanup
- safe restart behavior
- clear user-visible errors

Code signing is recommended for public releases but is not configured by default.

## Release process

Dispatch the **Release** workflow from GitHub Actions with the requested semantic version and prerelease setting. The workflow handles version metadata, lockfile generation, build, tag, and GitHub release publication.

Release workflow responsibilities:

- validate requested semantic version
- update Cargo and Windows manifest versions
- regenerate `Cargo.lock`
- build the default-branch commit
- produce the Windows executable asset
- create the Git tag and GitHub release
- mark prereleases correctly

## Automated checks

Run before opening a pull request:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release --target x86_64-pc-windows-msvc --bin x360ce
```

When changing dependencies, also review licenses:

```powershell
cargo install cargo-deny
cargo deny check licenses
```

## Manual validation

At minimum, validate relevant items from this list:

1. Application starts without a controller.
2. Physical controller appears and reconnects.
3. Device selector remains usable with long controller names.
4. Learn mode detects buttons, axes, and hats.
5. Double-click starts learning from layout and mapping rows.
6. `Esc` cancels learning without changing the profile.
7. Positive and negative axis directions are stored correctly.
8. Invert, deadzone, saturation, and centered-axis settings affect output.
9. Individual mappings can be cleared.
10. Import and export preserve the complete profile.
11. Profiles persist after restart.
12. Virtual Xbox 360 controller appears when ViGEmBus is installed.
13. Visible-window isolation pauses output when configured.
14. Hiding to tray resumes forwarding.
15. Automatic tray countdown cancels on activity.
16. Tray actions open, enable, pause, update, startup, and quit correctly.
17. Second instance activates the existing instance.
18. Guide/Nexus input does not open Xbox Game Bar or a missing-app dialog.
19. Protocol-sink launches do not show a console window.
20. Settings remain beside the executable.
21. UI remains readable without overlapping at common DPI scales.

## Pull requests

A pull request should include:

- clear problem statement
- focused implementation summary
- screenshots only when visual review truly requires them
- automated checks run
- manual checks performed
- known limitations or follow-up work

Avoid unrelated refactors in feature or bug-fix pull requests.

## Commit messages

Use Conventional Commits 1.0.0.

Examples:

```text
feat(mapper): add hat switch learning
fix(engine): reconnect selected controller after USB reset
fix(game-bar): silence protocol sink launch
refactor(ui): isolate controller layout rendering
perf(engine): avoid redundant virtual reports
chore(release): prepare v0.2.0
```

## Release notes

Use human-readable sections based on Keep a Changelog:

```text
Added
Changed
Deprecated
Removed
Fixed
Security
```

Describe user-visible behavior rather than internal implementation details whenever possible.

## Licensing

Contributions are accepted under the project license, GNU Affero General Public License v3.0 or later.

Third-party additions must have compatible licenses and must be documented in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
