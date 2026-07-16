use anyhow::Result;
use std::{thread, time::Duration};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, SetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    System::Threading::CreateMutexW,
    UI::WindowsAndMessaging::{
        FindWindowW, MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    },
};

use crate::{config, wide::str_wide_null};

const INSTANCE_MUTEX_NAME: &str = "Local\\x360ce.RozsaZoltan.SingleInstance";
const DEVELOPMENT_MUTEX_NAME: &str = "Local\\x360ce.RozsaZoltan.DevelopmentBuild";
const PRODUCTION_MUTEX_NAME: &str = "Local\\x360ce.RozsaZoltan.ProductionBuild";
const BUILD_DETECTION_RETRIES: usize = 10;
const BUILD_DETECTION_RETRY_DELAY: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildKind {
    Development,
    Production,
    Unknown,
}

impl BuildKind {
    fn current() -> Self {
        if config::is_dev_build() {
            Self::Development
        } else {
            Self::Production
        }
    }

    fn mutex_name(self) -> Option<&'static str> {
        match self {
            Self::Development => Some(DEVELOPMENT_MUTEX_NAME),
            Self::Production => Some(PRODUCTION_MUTEX_NAME),
            Self::Unknown => None,
        }
    }

    pub fn window_title(self) -> Option<String> {
        match self {
            Self::Development => Some(format!(
                "{} v{}-dev",
                config::app_name(),
                env!("CARGO_PKG_VERSION")
            )),
            Self::Production => Some(format!(
                "{} v{}",
                config::app_name(),
                env!("CARGO_PKG_VERSION")
            )),
            Self::Unknown => None,
        }
    }
}

pub enum AcquireOutcome {
    Acquired(SingleInstanceGuard),
    AlreadyRunning(BuildKind),
}

pub struct SingleInstanceGuard {
    instance_handle: HANDLE,
    build_handle: HANDLE,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        for handle in [self.build_handle, self.instance_handle] {
            if !handle.is_null() {
                unsafe {
                    CloseHandle(handle);
                }
            }
        }
    }
}

pub fn acquire() -> Result<AcquireOutcome> {
    let (instance_handle, already_exists) = create_named_mutex(INSTANCE_MUTEX_NAME)?;
    if already_exists {
        unsafe {
            CloseHandle(instance_handle);
        }
        return Ok(AcquireOutcome::AlreadyRunning(detect_existing_build()));
    }

    let build_kind = BuildKind::current();
    let build_mutex_name = build_kind
        .mutex_name()
        .expect("current build kind always has a mutex name");
    let (build_handle, _) = match create_named_mutex(build_mutex_name) {
        Ok(result) => result,
        Err(error) => {
            unsafe {
                CloseHandle(instance_handle);
            }
            return Err(error);
        }
    };

    Ok(AcquireOutcome::Acquired(SingleInstanceGuard {
        instance_handle,
        build_handle,
    }))
}

pub fn show_already_running_notice(existing_build: BuildKind) {
    let current_build = BuildKind::current();
    let message = already_running_message(existing_build, current_build);
    let message = str_wide_null(message);
    let title = str_wide_null("x360ce is already running");

    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

fn create_named_mutex(name: &str) -> Result<(HANDLE, bool)> {
    let name = str_wide_null(name);
    unsafe {
        SetLastError(0);
    }
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        anyhow::bail!("failed to create a x360ce single-instance mutex");
    }

    let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    Ok((handle, already_exists))
}

fn detect_existing_build() -> BuildKind {
    for _ in 0..BUILD_DETECTION_RETRIES {
        let development = named_mutex_exists(DEVELOPMENT_MUTEX_NAME).unwrap_or(false)
            || build_window_exists(BuildKind::Development);
        let production = named_mutex_exists(PRODUCTION_MUTEX_NAME).unwrap_or(false)
            || build_window_exists(BuildKind::Production);

        match (development, production) {
            (true, false) => return BuildKind::Development,
            (false, true) => return BuildKind::Production,
            _ => thread::sleep(BUILD_DETECTION_RETRY_DELAY),
        }
    }

    BuildKind::Unknown
}

fn named_mutex_exists(name: &str) -> Result<bool> {
    let (handle, already_exists) = create_named_mutex(name)?;
    unsafe {
        CloseHandle(handle);
    }
    Ok(already_exists)
}

fn build_window_exists(build_kind: BuildKind) -> bool {
    let Some(title) = build_kind.window_title() else {
        return false;
    };
    let title = str_wide_null(&title);

    !unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }.is_null()
}

fn already_running_message(existing_build: BuildKind, current_build: BuildKind) -> &'static str {
    match (existing_build, current_build) {
        (BuildKind::Development, BuildKind::Production) => {
            "A x360ce development build is already running.\n\nRight-click the x360ce tray icon and choose \"Quit x360ce\" before starting the production build."
        }
        (BuildKind::Production, BuildKind::Development) => {
            "A x360ce production build is already running.\n\nRight-click the x360ce tray icon and choose \"Quit x360ce\" before starting the development build."
        }
        (BuildKind::Development, BuildKind::Development) => {
            "x360ce development build is already running.\n\nRight-click the x360ce tray icon and choose \"Quit x360ce\" before starting it again."
        }
        (BuildKind::Production, BuildKind::Production) => {
            "x360ce is already running.\n\nRight-click the x360ce tray icon and choose \"Quit x360ce\" before starting it again."
        }
        _ => {
            "Another x360ce instance is already running.\n\nRight-click the x360ce tray icon and choose \"Quit x360ce\" before starting this build."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{already_running_message, BuildKind};

    #[test]
    fn explains_development_to_production_conflict() {
        let message = already_running_message(BuildKind::Development, BuildKind::Production);

        assert!(message.contains("development build is already running"));
        assert!(message.contains("production build"));
        assert!(message.contains("Quit x360ce"));
    }

    #[test]
    fn explains_production_to_development_conflict() {
        let message = already_running_message(BuildKind::Production, BuildKind::Development);

        assert!(message.contains("production build is already running"));
        assert!(message.contains("development build"));
        assert!(message.contains("Quit x360ce"));
    }
}
