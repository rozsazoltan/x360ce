param(
    [string]$SessionName = "x360ce-win-dev",
    [string]$WindowsProjectPath = "D:\github\rozsazoltan\x360ce",
    [string]$SourceProjectPath = "",
    [ValidateSet("one-way-replica", "two-way-safe")]
    [string]$SyncMode = "one-way-replica"
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

    return [System.IO.Path]::GetFullPath($NormalizedPath)
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

$MutagenCommand = Get-Command mutagen -ErrorAction SilentlyContinue
if (-not $MutagenCommand) {
    throw "Mutagen was not found on PATH. Install mutagen.exe on Windows first, then reopen PowerShell."
}

$MutagenExe = $MutagenCommand.Source

function Invoke-Mutagen {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,
        [switch]$CaptureOutput
    )

    $PreviousErrorActionPreference = $ErrorActionPreference
    try {
        # Windows PowerShell can wrap native stderr as NativeCommandError even when
        # Mutagen exits successfully. Capture both streams and trust exit code.
        $ErrorActionPreference = "Continue"
        $CommandOutput = & $MutagenExe @Arguments 2>&1
        $ExitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $PreviousErrorActionPreference
    }

    $OutputLines = @($CommandOutput | ForEach-Object { $_.ToString() })

    if ($ExitCode -ne 0) {
        $CommandText = "mutagen " + ($Arguments -join " ")
        $Details = ($OutputLines -join [Environment]::NewLine).Trim()

        if ($Details) {
            throw "$CommandText failed with exit code $ExitCode.`n$Details"
        }

        throw "$CommandText failed with exit code $ExitCode."
    }

    if ($CaptureOutput) {
        return $OutputLines
    }

    $OutputLines | ForEach-Object { Write-Host $_ }
}

if ([string]::IsNullOrWhiteSpace($SourceProjectPath)) {
    $SourceProjectPath = Get-WorkspaceRoot
}

$SourceProjectPath = Normalize-NativePath $SourceProjectPath
$WindowsProjectPath = Normalize-NativePath $WindowsProjectPath

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
Write-Host "Mutagen:          $MutagenExe"
Write-Host "Source workspace: $SourceFullPath"
Write-Host "Windows mirror:   $TargetFullPath"
Write-Host "Mutagen session:  $SessionName"
Write-Host "Sync mode:        $SyncMode"
Write-Host ""

New-Item -ItemType Directory -Force -Path $TargetFullPath | Out-Null

Invoke-Mutagen -Arguments @("daemon", "start")

$SessionList = Invoke-Mutagen -Arguments @("sync", "list", "--long") -CaptureOutput
$ExistingSession = $SessionList | Select-String -SimpleMatch "Name: $SessionName"

if ($ExistingSession) {
    Write-Host "Mutagen session '$SessionName' already exists."
    Write-Host "Use 'mutagen sync monitor $SessionName' to watch it, or terminate it first if you want to recreate it."
} else {
    Invoke-Mutagen -Arguments @(
        "sync", "create",
        "--name", $SessionName,
        "--sync-mode", $SyncMode,
        "--ignore-vcs",
        "--ignore", ".cache",
        "--ignore", "target",
        "--ignore", "dist",
        "--ignore", ".x360ce-data",
        "--ignore", "*.zip",
        "--ignore", "*.pdb",
        $SourceFullPath,
        $TargetFullPath
    )
}

Invoke-Mutagen -Arguments @("sync", "flush", $SessionName)

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
