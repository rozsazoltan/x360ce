use anyhow::{Context, Result};
use std::{env, ptr};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::wide::{str_wide_null, wide_null};

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "x360ce";

pub fn is_enabled() -> Result<bool> {
    let key = open_run_key(KEY_READ)?;
    let name = str_wide_null(VALUE_NAME);
    let mut value_type = 0u32;
    let status = unsafe {
        RegQueryValueExW(
            key.raw,
            name.as_ptr(),
            ptr::null_mut(),
            &mut value_type,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };

    drop(key);

    if status == ERROR_SUCCESS {
        Ok(value_type == REG_SZ)
    } else if status == ERROR_FILE_NOT_FOUND {
        Ok(false)
    } else {
        anyhow::bail!("failed to query Windows startup value: {status}")
    }
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    if enabled {
        enable()
    } else {
        disable()
    }
}

fn enable() -> Result<()> {
    let key = create_run_key()?;
    let name = str_wide_null(VALUE_NAME);
    let command = startup_command()?;
    let bytes = command
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect::<Vec<_>>();

    let status = unsafe {
        RegSetValueExW(
            key.raw,
            name.as_ptr(),
            0,
            REG_SZ,
            bytes.as_ptr(),
            bytes.len() as u32,
        )
    };

    if status != ERROR_SUCCESS {
        anyhow::bail!("failed to write Windows startup value: {status}");
    }

    Ok(())
}

fn disable() -> Result<()> {
    let key = open_run_key(KEY_SET_VALUE)?;
    let name = str_wide_null(VALUE_NAME);
    let status = unsafe { RegDeleteValueW(key.raw, name.as_ptr()) };

    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        anyhow::bail!("failed to delete Windows startup value: {status}")
    }
}

fn startup_command() -> Result<Vec<u16>> {
    let exe = env::current_exe().context("failed to resolve current executable path")?;
    let command = format!("\"{}\" --startup", exe.display());
    Ok(wide_null(command))
}

fn open_run_key(access: u32) -> Result<RegKey> {
    let path = str_wide_null(RUN_KEY);
    let mut raw: HKEY = ptr::null_mut();
    let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut raw) };
    if status != ERROR_SUCCESS {
        anyhow::bail!("failed to open Windows startup registry key: {status}");
    }
    Ok(RegKey { raw })
}

fn create_run_key() -> Result<RegKey> {
    let path = str_wide_null(RUN_KEY);
    let mut raw: HKEY = ptr::null_mut();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            ptr::null(),
            &mut raw,
            ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        anyhow::bail!("failed to create Windows startup registry key: {status}");
    }
    Ok(RegKey { raw })
}

struct RegKey {
    raw: HKEY,
}

impl Drop for RegKey {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            unsafe {
                RegCloseKey(self.raw);
            }
        }
    }
}
