# Contributing to x360ce

Keep changes small, focused, testable, and easy to review.

## Requirements

- Windows 10/11 x64
- Rust 1.92 or newer
- MSVC build tools
- ViGEmBus installed for virtual-controller runtime tests
- physical controller for end-to-end mapping tests

## Setup

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --bin x360ce
```

Generate and commit `Cargo.lock` after dependency changes:

```powershell
cargo generate-lockfile
```

## Development

```powershell
cargo dev
```

Or:

```powershell
scripts\dev.ps1
```

Development builds open window automatically and disable update/startup side effects.

## Checks

Run before opening pull request:

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release --target x86_64-pc-windows-msvc --bin x360ce
```

Manual checks:

1. app starts without controller
2. physical controller appears and reconnects
3. learn mode detects buttons, axes, and hats
4. mappings persist after restart
5. deadzone, saturation, and invert change output
6. virtual Xbox 360 controller appears when driver is installed
7. forwarding continues after hiding window
8. inactivity countdown cancels on UI or controller activity
9. countdown hides window and keeps forwarding
10. tray open, enable, startup, update, and quit actions work
11. second instance activates existing instance
12. config remains beside executable

## Architecture rules

- Keep SDL objects on controller engine thread.
- Keep UI free from blocking controller and network calls.
- Send immutable profile snapshots to engine.
- Persist only serializable domain state.
- Keep Windows integration behind Windows-only modules.
- Avoid copied source from legacy x360ce projects.
- Treat driver installation as privileged external system change.
- Validate all update assets before replacement when signing support is added.

## Commit messages

Use Conventional Commits 1.0.0.

Examples:

```text
feat(mapper): add hat switch learning
fix(engine): reconnect selected controller after USB reset
refactor(ui): isolate controller layout rendering
perf(engine): avoid redundant virtual reports
chore(release): prepare v0.2.0
```

## Release notes

Follow Keep a Changelog. Use human-readable sections:

```text
Added
Changed
Removed
Fixed
Security
```
