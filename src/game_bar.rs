use anyhow::{bail, Context, Result};
use std::{env, os::windows::process::CommandExt, process::Command};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const GAME_BAR_KEY: &str = r"HKCU\Software\Microsoft\GameBar";
const CONTROLLER_BUTTON_VALUE: &str = "UseNexusForGameBarEnabled";
const GAME_DVR_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR";
const GAME_DVR_STORE_KEY: &str = r"HKCU\System\GameConfigStore";
const PROTOCOLS: [&str; 2] = ["ms-gamebar", "ms-gamebarservices"];

pub fn install_hard_block() -> Result<()> {
    set_dword(GAME_BAR_KEY, CONTROLLER_BUTTON_VALUE, 0)?;
    set_dword(GAME_DVR_KEY, "AppCaptureEnabled", 0)?;
    set_dword(GAME_DVR_STORE_KEY, "GameDVR_Enabled", 0)?;

    let executable = env::current_exe().context("failed to resolve x360ce executable path")?;
    let executable = executable.to_string_lossy();
    let command = format!("\"{executable}\" --gamebar-noop");

    for protocol in PROTOCOLS {
        let root = format!(r"HKCU\Software\Classes\{protocol}");
        set_string(&root, None, "URL:x360ce Game Bar blocker")?;
        set_string(&root, Some("URL Protocol"), "")?;
        set_string(&format!(r"{root}\shell\open\command"), None, &command)?;
    }

    Ok(())
}

fn set_dword(key: &str, value_name: &str, value: u32) -> Result<()> {
    run_reg([
        "ADD",
        key,
        "/v",
        value_name,
        "/t",
        "REG_DWORD",
        "/d",
        &value.to_string(),
        "/f",
    ])
}

fn set_string(key: &str, value_name: Option<&str>, value: &str) -> Result<()> {
    let mut args = vec!["ADD", key];
    match value_name {
        Some(name) => {
            args.push("/v");
            args.push(name);
        }
        None => args.push("/ve"),
    }
    args.extend(["/t", "REG_SZ", "/d", value, "/f"]);
    run_reg(args)
}

fn run_reg<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let status = Command::new("reg.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .context("failed to start Windows registry command")?;

    if !status.success() {
        bail!("Windows registry command failed with exit code {status}");
    }

    Ok(())
}
