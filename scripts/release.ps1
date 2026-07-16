param(
    [string]$Version = "",
    [switch]$Prerelease,
    [switch]$Publish,
    [string]$Ref = ""
)

$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path "$PSScriptRoot\..")
function Assert-SemanticVersion {
    param([string]$Value)

    if ($Value -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$') {
        throw "Invalid version '$Value'. Use semantic version format like 0.10.0 or 0.10.0-beta.1 without v prefix."
    }
}

$Target = "x86_64-pc-windows-msvc"
$CargoToml = Get-Content Cargo.toml -Raw
$PackageVersionMatch = [regex]::Match(
    $CargoToml,
    '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"'
)
if (-not $PackageVersionMatch.Success) {
    throw "Could not read package version from Cargo.toml."
}
$PackageVersion = $PackageVersionMatch.Groups[1].Value

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = $PackageVersion
}
$Version = $Version.Trim()
Assert-SemanticVersion $Version

if ($Publish) {
    if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
        throw "GitHub CLI was not found on PATH. Install gh, authenticate with 'gh auth login', then retry."
    }

    $PrereleaseValue = $Prerelease.IsPresent.ToString().ToLowerInvariant()
    $Arguments = @(
        "workflow", "run", "release.yml",
        "--field", "version=$Version",
        "--field", "prerelease=$PrereleaseValue"
    )
    if (-not [string]::IsNullOrWhiteSpace($Ref)) {
        $Arguments += @("--ref", $Ref)
    }

    & gh @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "GitHub release workflow dispatch failed with exit code $LASTEXITCODE."
    }

    $Channel = if ($Prerelease) { "prerelease" } else { "stable" }
    Write-Host "Dispatched $Channel release workflow for v$Version."
    exit 0
}

if ($Version -ne $PackageVersion) {
    throw "Requested local build version $Version does not match Cargo.toml version $PackageVersion. Use -Publish to let release workflow update version metadata."
}

& "$PSScriptRoot\resolve-cmake.ps1"
rustup target add $Target
cargo fmt --all -- --check
cargo clippy --target $Target --all-targets --all-features -- -D warnings
cargo test --target $Target --all
cargo build --target $Target --release --bin x360ce

New-Item -ItemType Directory -Force dist | Out-Null
$Asset = "dist\x360ce-v$Version-windows-x64.exe"
Copy-Item "target\$Target\release\x360ce.exe" $Asset -Force
$Channel = if ($Prerelease) { "prerelease" } else { "stable" }
Write-Host "Local $Channel release asset: $Asset"
