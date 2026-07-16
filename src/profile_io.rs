use crate::model::ControllerProfile;
use anyhow::{Context, Result};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
};
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const FILTER: &str = "x360ce mapping (*.json)|*.json|JSON files (*.json)|*.json|All files (*.*)|*.*";

pub fn export_profile(profile: &ControllerProfile) -> Result<Option<PathBuf>> {
    let default_name = format!("{}.x360ce.json", sanitize_filename(&profile.name));
    let Some(path) = choose_path(DialogKind::Save, &default_name)? else {
        return Ok(None);
    };

    let mut serialized = serde_json::to_vec_pretty(profile)
        .context("failed to serialize controller mapping")?;
    serialized.push(b'\n');
    fs::write(&path, serialized)
        .with_context(|| format!("failed to export mapping: {}", path.display()))?;
    Ok(Some(path))
}

pub fn import_profile() -> Result<Option<(PathBuf, ControllerProfile)>> {
    let Some(path) = choose_path(DialogKind::Open, "mapping.x360ce.json")? else {
        return Ok(None);
    };

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read mapping: {}", path.display()))?;
    let profile: ControllerProfile = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid x360ce mapping: {}", path.display()))?;
    Ok(Some((path, profile)))
}

#[derive(Clone, Copy)]
enum DialogKind {
    Open,
    Save,
}

fn choose_path(kind: DialogKind, default_name: &str) -> Result<Option<PathBuf>> {
    let dialog_type = match kind {
        DialogKind::Open => "OpenFileDialog",
        DialogKind::Save => "SaveFileDialog",
    };
    let check_file_exists = match kind {
        DialogKind::Open => "$dialog.CheckFileExists = $true;",
        DialogKind::Save => "$dialog.OverwritePrompt = $true;",
    };
    let safe_name = powershell_single_quoted(default_name);
    let safe_filter = powershell_single_quoted(FILTER);
    let script = format!(
        r#"[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false);
Add-Type -AssemblyName System.Windows.Forms;
$dialog = New-Object System.Windows.Forms.{dialog_type};
$dialog.Filter = '{safe_filter}';
$dialog.DefaultExt = 'json';
$dialog.AddExtension = $true;
$dialog.FileName = '{safe_name}';
{check_file_exists}
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {{
    [Console]::Write($dialog.FileName)
}}"#
    );

    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-Sta",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("failed to open Windows mapping file dialog")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        anyhow::bail!(
            "mapping file dialog failed{}",
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }

    let selected = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if selected.is_empty() {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(selected)))
    }
}

fn sanitize_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "mapping".to_owned()
    } else {
        sanitized.to_owned()
    }
}

fn powershell_single_quoted(value: &str) -> String {
    value.replace('`', "``").replace('\'', "''")
}
