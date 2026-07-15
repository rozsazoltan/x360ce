@echo off
setlocal
cd /d "%~dp0.."
echo x360ce dev launcher
echo Using project-local dev runner; cargo-watch is not required.
cargo run --bin x360ce-dev --
