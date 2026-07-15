use anyhow::{Context, Result};
use reqwest::{blocking::Client, StatusCode};
use semver::Version;
use serde::{de::DeserializeOwned, Deserialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const RELEASES_API: &str = "https://api.github.com/repos/rozsazoltan/x360ce/releases";
const LATEST_RELEASE_API: &str = "https://api.github.com/repos/rozsazoltan/x360ce/releases/latest";
const USER_AGENT: &str = "x360ce-Updater";

#[derive(Clone, Debug)]
pub struct UpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub asset_name: Option<String>,
    pub asset_download_url: Option<String>,
    pub is_update_available: bool,
    pub prerelease: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

fn ensure_updates_enabled() -> Result<()> {
    if cfg!(debug_assertions) {
        anyhow::bail!("updates are disabled in development builds");
    }
    Ok(())
}

pub fn check_for_update(include_prereleases: bool) -> Result<UpdateCheck> {
    ensure_updates_enabled()?;

    if include_prereleases {
        check_latest_prerelease()
    } else {
        check_latest_stable()
    }
}

pub fn check_latest_stable() -> Result<UpdateCheck> {
    ensure_updates_enabled()?;
    let client = http_client()?;
    let release: GitHubRelease =
        get_github_json(&client, LATEST_RELEASE_API, "GitHub latest stable release")?;
    update_check_from_release(release)
}

pub fn check_latest_prerelease() -> Result<UpdateCheck> {
    ensure_updates_enabled()?;
    let client = http_client()?;
    let releases: Vec<GitHubRelease> = get_github_json(&client, RELEASES_API, "GitHub releases")?;
    let mut candidates = releases
        .into_iter()
        .filter(|release| !release.draft && release.prerelease)
        .filter_map(|release| {
            let parsed = parse_release_version(&release.tag_name).ok()?;
            Some((parsed, release))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    let release = candidates
        .into_iter()
        .next()
        .map(|(_, release)| release)
        .context("no prerelease builds were found in GitHub releases")?;
    update_check_from_release(release)
}

fn update_check_from_release(release: GitHubRelease) -> Result<UpdateCheck> {
    let current_version = env!("CARGO_PKG_VERSION").to_owned();
    let current_semver =
        Version::parse(&current_version).context("invalid current application version")?;
    let latest_semver = parse_release_version(&release.tag_name)
        .context("invalid latest GitHub release version")?;
    let asset = release.assets.iter().find(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name == "x360ce.exe"
            || name == "x360ce-windows-x64.exe"
            || (name.starts_with("x360ce-") && name.ends_with("-windows-x64.exe"))
    });

    Ok(UpdateCheck {
        current_version,
        latest_version: latest_semver.to_string(),
        asset_name: asset.map(|asset| asset.name.clone()),
        asset_download_url: asset.map(|asset| asset.browser_download_url.clone()),
        is_update_available: latest_semver > current_semver,
        prerelease: release.prerelease,
    })
}

fn parse_release_version(value: &str) -> Result<Version> {
    Version::parse(value.trim_start_matches('v'))
        .with_context(|| format!("invalid release version: {value}"))
}

fn http_client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .context("failed to build HTTP client")
}

fn get_github_json<T>(client: &Client, url: &str, label: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("failed to contact {label}"))?;
    let status = response.status();
    if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
        anyhow::bail!(
            "GitHub temporarily refused the release check ({status}). Wait before checking again; repeated requests may trigger API rate limiting."
        );
    }
    response
        .error_for_status()
        .with_context(|| format!("{label} request failed"))?
        .json()
        .with_context(|| format!("failed to parse {label} response"))
}

pub fn install_update(check: &UpdateCheck) -> Result<()> {
    ensure_updates_enabled()?;

    let Some(download_url) = check.asset_download_url.as_ref() else {
        anyhow::bail!("the selected release does not contain a Windows executable asset");
    };

    let current_exe = env::current_exe().context("failed to resolve current executable path")?;
    ensure_current_exe_can_be_replaced(&current_exe)?;

    let update_dir = current_exe
        .parent()
        .map(|parent| parent.join(".x360ce-data").join("update"))
        .unwrap_or_else(|| env::temp_dir().join("x360ce-update"));
    fs::create_dir_all(&update_dir)
        .with_context(|| format!("failed to create update folder: {}", update_dir.display()))?;

    let new_exe = update_dir.join("x360ce-update.exe");

    let bytes = http_client()?
        .get(download_url)
        .send()
        .context("failed to download update asset")?
        .error_for_status()
        .context("GitHub update asset download failed")?
        .bytes()
        .context("failed to read downloaded update asset")?;
    if bytes.len() < 2 || &bytes[..2] != b"MZ" {
        anyhow::bail!("downloaded update asset is not a Windows PE executable");
    }
    fs::write(&new_exe, &bytes)
        .with_context(|| format!("failed to write update asset: {}", new_exe.display()))?;

    launch_windows_replacer(&current_exe, &new_exe)?;
    std::process::exit(0);
}

#[cfg(windows)]
fn launch_windows_replacer(current_exe: &Path, new_exe: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let update_dir = new_exe
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let script = update_dir.join("x360ce-update.ps1");
    let script_contents = format!(
        "$ErrorActionPreference = 'SilentlyContinue'\r\n\
$newExe = '{new_exe}'\r\n\
$currentExe = '{current_exe}'\r\n\
for ($i = 0; $i -lt 40; $i++) {{\r\n\
  try {{\r\n\
    Copy-Item -LiteralPath $newExe -Destination $currentExe -Force -ErrorAction Stop\r\n\
    Start-Process -FilePath $currentExe\r\n\
    Remove-Item -LiteralPath $newExe -Force -ErrorAction SilentlyContinue\r\n\
    Remove-Item -LiteralPath $PSCommandPath -Force -ErrorAction SilentlyContinue\r\n\
    exit 0\r\n\
  }} catch {{\r\n\
    Start-Sleep -Milliseconds 500\r\n\
  }}\r\n\
}}\r\n\
exit 1\r\n",
        new_exe = powershell_single_quoted_path(new_exe),
        current_exe = powershell_single_quoted_path(current_exe),
    );
    fs::write(&script, script_contents)
        .with_context(|| format!("failed to write updater script: {}", script.display()))?;

    let script_path = script.to_string_lossy().to_string();
    Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
            script_path.as_str(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .context("failed to launch hidden updater helper")?;
    Ok(())
}

#[cfg(windows)]
fn powershell_single_quoted_path(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

#[cfg(not(windows))]
fn launch_windows_replacer(_current_exe: &Path, _new_exe: &Path) -> Result<()> {
    anyhow::bail!("self-update replacement is currently implemented for Windows builds only")
}

fn ensure_current_exe_can_be_replaced(current_exe: &Path) -> Result<()> {
    let Some(exe_dir) = current_exe.parent() else {
        anyhow::bail!("could not resolve the executable folder for update replacement");
    };
    let probe = exe_dir.join(".x360ce-update-write-test.tmp");
    fs::write(&probe, b"write-test").with_context(|| {
        format!(
            "x360ce cannot update itself because the executable folder is not writable: {}. Move the app to a user-writable fixed folder or run the update with sufficient permissions.",
            exe_dir.display()
        )
    })?;
    let _ = fs::remove_file(probe);
    Ok(())
}

