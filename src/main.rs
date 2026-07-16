#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
mod model;
mod updater;

#[cfg(windows)]
mod driver;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod game_bar;
#[cfg(windows)]
mod mapper;
#[cfg(windows)]
mod profile_io;
#[cfg(windows)]
mod single_instance;
#[cfg(windows)]
mod startup;
#[cfg(windows)]
mod virtual_gamepad;
#[cfg(windows)]
mod wide;
#[cfg(windows)]
mod win_app;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    if std::env::args_os().any(|argument| argument == std::ffi::OsStr::new("--gamebar-noop")) {
        return Ok(());
    }

    let _guard = match single_instance::acquire()? {
        single_instance::AcquireOutcome::Acquired(guard) => guard,
        single_instance::AcquireOutcome::AlreadyRunning(existing_build) => {
            win_app::activate_existing_instance(existing_build);
            single_instance::show_already_running_notice(existing_build);
            return Ok(());
        }
    };

    win_app::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("x360ce is a Windows-only controller mapper.");
}
