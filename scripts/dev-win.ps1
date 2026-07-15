param(
    [string]$SessionName = "x360ce-win-dev",
    [string]$WindowsProjectPath = "D:\github\rozsazoltan\x360ce"
)

$ErrorActionPreference = "Stop"

if (-not (Get-Command mutagen -ErrorAction SilentlyContinue)) {
    throw "Mutagen was not found on PATH. Install mutagen.exe on Windows first, then reopen PowerShell."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Cargo was not found on PATH. Install Rust for Windows first, then reopen PowerShell."
}

$WindowsProjectPath = $WindowsProjectPath.Trim().Trim('"')
if (-not (Test-Path $WindowsProjectPath)) {
    throw "Windows project path was not found: $WindowsProjectPath"
}

mutagen daemon start

$SessionExists = mutagen sync list --long 2>$null | Select-String -SimpleMatch "Name: $SessionName"
if (-not $SessionExists) {
    throw "Mutagen sync session not found: $SessionName. Run scripts\setup-mutagen-wsl-dev.ps1 first."
}

mutagen sync flush $SessionName

Set-Location $WindowsProjectPath

Write-Host "x360ce Windows dev runner"
Write-Host "Project: $WindowsProjectPath"
Write-Host "Mutagen session: $SessionName"
Write-Host ""

cargo dev
