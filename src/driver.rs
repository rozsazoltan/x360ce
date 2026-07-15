use anyhow::{Context, Result};
use std::{env, fs, os::windows::ffi::OsStrExt, path::PathBuf, process, ptr};
use windows_sys::Win32::{
    System::Registry::{
        RegCloseKey, RegOpenKeyExW, HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE,
    },
    UI::{
        Shell::ShellExecuteW,
        WindowsAndMessaging::SW_SHOWNORMAL,
    },
};

use crate::wide::str_wide_null;

const SERVICE_KEY: &str = r"SYSTEM\CurrentControlSet\Services\ViGEmBus";
const INSTALLER_FILE_NAME: &str = "ViGEmBus_1.22.0_x64_x86_arm64.exe";
const INSTALLER_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/third-party/ViGEmBus_1.22.0_x64_x86_arm64.bin"
));

pub fn is_installed() -> bool {
    let mut key: HKEY = ptr::null_mut();
    let path = str_wide_null(SERVICE_KEY);
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if status == 0 && !key.is_null() {
        unsafe {
            RegCloseKey(key);
        }
        true
    } else {
        false
    }
}

pub fn launch_installer_elevated() -> Result<PathBuf> {
    let installer = extract_installer()?;
    let operation = str_wide_null("runas");
    let file = installer.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            file.as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;

    if result <= 32 {
        anyhow::bail!("failed to launch ViGEmBus installer with elevation (ShellExecuteW code {result})");
    }

    Ok(installer)
}

fn extract_installer() -> Result<PathBuf> {
    let base = env::temp_dir()
        .join("x360ce")
        .join(format!("driver-{}", process::id()));
    fs::create_dir_all(&base)
        .with_context(|| format!("failed to create driver temp folder: {}", base.display()))?;
    let path = base.join(INSTALLER_FILE_NAME);
    fs::write(&path, INSTALLER_BYTES)
        .with_context(|| format!("failed to extract embedded driver installer: {}", path.display()))?;
    Ok(path)
}
