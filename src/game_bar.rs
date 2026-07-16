use anyhow::{bail, Context, Result};
use std::{os::windows::process::CommandExt, process::Command};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GAME_BAR_KEY: &str = r"HKCU\Software\Microsoft\GameBar";
const CONTROLLER_BUTTON_VALUE: &str = "UseNexusForGameBarEnabled";

pub fn set_controller_button_enabled(enabled: bool) -> Result<()> {
    let status = Command::new("reg.exe")
        .args([
            "ADD",
            GAME_BAR_KEY,
            "/v",
            CONTROLLER_BUTTON_VALUE,
            "/t",
            "REG_DWORD",
            "/d",
            if enabled { "1" } else { "0" },
            "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .context("failed to start Windows registry command")?;

    if !status.success() {
        bail!("Windows registry command failed with exit code {status}");
    }

    Ok(())
}
