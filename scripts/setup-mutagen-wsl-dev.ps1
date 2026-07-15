param(
    [string]$SessionName = "x360ce-win-dev",
    [string]$WindowsProjectPath = "D:\github\rozsazoltan\x360ce",
    [string]$SourceProjectPath = "",
    [ValidateSet("one-way-replica", "two-way-safe")]
    [string]$SyncMode = "one-way-replica",
    [bool]$InstallMutagenIfMissing = $true,
    [string]$MutagenVersion = "latest",
    [string]$MutagenInstallDirectory = (Join-Path $env:LOCALAPPDATA "Programs\Mutagen")
)

$ErrorActionPreference = "Stop"

function Normalize-NativePath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return ""
    }

    $NormalizedPath = $Path.Trim().Trim('"')
    $ProviderPrefix = "Microsoft.PowerShell.Core\FileSystem::"

    if ($NormalizedPath.StartsWith($ProviderPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        $NormalizedPath = $NormalizedPath.Substring($ProviderPrefix.Length)
    }

    if (Test-Path -LiteralPath $NormalizedPath) {
        $ResolvedPath = Resolve-Path -LiteralPath $NormalizedPath | Select-Object -First 1
        if ($ResolvedPath.ProviderPath) {
            $NormalizedPath = $ResolvedPath.ProviderPath
        } else {
            $NormalizedPath = $ResolvedPath.Path
        }

        if ($NormalizedPath.StartsWith($ProviderPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
            $NormalizedPath = $NormalizedPath.Substring($ProviderPrefix.Length)
        }
    }

    return $NormalizedPath
}

function Get-WorkspaceRoot {
    if (-not $PSScriptRoot) {
        throw "This script must be run from a saved .ps1 file inside the repository scripts directory."
    }

    return Normalize-NativePath (Join-Path -Path $PSScriptRoot -ChildPath "..")
}

function Test-IsWslUncPath {
    param([string]$Path)

    $NormalizedPath = Normalize-NativePath $Path
    return $NormalizedPath -match '^\\\\(wsl\$|wsl\.localhost)\\[^\\]+\\'
}

function Resolve-MutagenExecutable {
    $Command = Get-Command mutagen -ErrorAction SilentlyContinue
    if ($Command) {
        return $Command.Source
    }

    $ExpectedExe = Join-Path $MutagenInstallDirectory "mutagen.exe"
    if (Test-Path -LiteralPath $ExpectedExe) {
        return $ExpectedExe
    }

    if (-not $InstallMutagenIfMissing) {
        throw "Mutagen was not found on PATH. Run scripts\install-mutagen-windows.ps1 or enable automatic installation."
    }

    $Installer = Join-Path $PSScriptRoot "install-mutagen-windows.ps1"
    if (-not (Test-Path -LiteralPath $Installer)) {
        throw "Mutagen installer script was not found: $Installer"
    }

    & $Installer -Version $MutagenVersion -InstallDirectory $MutagenInstallDirectory | Out-Host
    if (-not (Test-Path -LiteralPath $ExpectedExe)) {
        throw "Mutagen installer completed, but executable was not found: $ExpectedExe"
    }

    return $ExpectedExe
}

if ([string]::IsNullOrWhiteSpace($SourceProjectPath)) {
    $SourceProjectPath = Get-WorkspaceRoot
}

$SourceProjectPath = Normalize-NativePath $SourceProjectPath
$WindowsProjectPath = Normalize-NativePath $WindowsProjectPath
$MutagenExe = Resolve-MutagenExecutable

if ([string]::IsNullOrWhiteSpace($SourceProjectPath)) {
    throw "Source project path is required."
}

if ([string]::IsNullOrWhiteSpace($WindowsProjectPath)) {
    throw "Windows mirror path is required."
}

if (-not (Test-IsWslUncPath $SourceProjectPath)) {
    throw "Source project path must be a WSL UNC path, for example \\wsl$\Ubuntu\github\rozsazoltan\x360ce. Run this script from the WSL path or pass -SourceProjectPath. Received: $SourceProjectPath"
}

if (-not (Test-Path -LiteralPath $SourceProjectPath)) {
    throw "Source project path was not found: $SourceProjectPath"
}

$SourceFullPath = Normalize-NativePath $SourceProjectPath
$TargetFullPath = [System.IO.Path]::GetFullPath($WindowsProjectPath)

if ($SourceFullPath.TrimEnd('\') -ieq $TargetFullPath.TrimEnd('\')) {
    throw "Source project path and Windows mirror path must be different."
}

Write-Host "x360ce Mutagen WSL -> Windows development setup"
Write-Host ""
Write-Host "Mutagen:         $MutagenExe"
Write-Host "Source workspace: $SourceFullPath"
Write-Host "Windows mirror:   $TargetFullPath"
Write-Host "Mutagen session:  $SessionName"
Write-Host "Sync mode:        $SyncMode"
Write-Host ""

New-Item -ItemType Directory -Force -Path $TargetFullPath | Out-Null

& $MutagenExe daemon start

$SessionList = & $MutagenExe sync list --long 2>$null
$ExistingSession = $SessionList | Select-String -SimpleMatch "Name: $SessionName"
if ($ExistingSession) {
    Write-Host "Mutagen session '$SessionName' already exists."
    Write-Host "Use 'mutagen sync monitor $SessionName' to watch it, or terminate it first if you want to recreate it."
} else {
    & $MutagenExe sync create `
        --name $SessionName `
        --sync-mode $SyncMode `
        --ignore-vcs `
        --ignore ".cache" `
        --ignore "target" `
        --ignore "dist" `
        --ignore ".x360ce-data" `
        --ignore "*.zip" `
        --ignore "*.exe" `
        --ignore "*.pdb" `
        $SourceFullPath `
        $TargetFullPath
}

& $MutagenExe sync flush $SessionName

Write-Host ""
Write-Host "x360ce Windows dev mirror ready."
Write-Host ""
Write-Host "Run Windows dev app from PowerShell:"
Write-Host "  cd $TargetFullPath"
Write-Host "  scripts\dev-win.ps1"
Write-Host ""
Write-Host "Keep Git operations on WSL source workspace side."
Write-Host ""
Write-Host "Useful Mutagen commands:"
Write-Host "  mutagen sync list"
Write-Host "  mutagen sync monitor $SessionName"
Write-Host "  mutagen sync flush $SessionName"
Write-Host "  mutagen sync terminate $SessionName"
