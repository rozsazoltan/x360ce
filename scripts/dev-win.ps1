param(
    [string]$SessionName = "x360ce-win-dev",
    [string]$WindowsProjectPath = "D:\github\rozsazoltan\x360ce"
)

$ErrorActionPreference = "Stop"

$MutagenCommand = Get-Command mutagen -ErrorAction SilentlyContinue
if (-not $MutagenCommand) {
    throw "Mutagen was not found on PATH. Install mutagen.exe on Windows first, then reopen PowerShell."
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Cargo was not found on PATH. Install Rust for Windows first, then reopen PowerShell."
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

$WindowsProjectPath = $WindowsProjectPath.Trim().Trim('"')
if (-not (Test-Path -LiteralPath $WindowsProjectPath)) {
    throw "Windows project path was not found: $WindowsProjectPath"
}

Invoke-Mutagen -Arguments @("daemon", "start")

$SessionList = Invoke-Mutagen -Arguments @("sync", "list", "--long") -CaptureOutput
$SessionExists = $SessionList | Select-String -SimpleMatch "Name: $SessionName"
if (-not $SessionExists) {
    throw "Mutagen sync session not found: $SessionName. Run scripts\setup-mutagen-wsl-dev.ps1 first."
}

Invoke-Mutagen -Arguments @("sync", "flush", $SessionName)

Set-Location $WindowsProjectPath
& "$PSScriptRoot\resolve-cmake.ps1"

Write-Host "x360ce Windows dev runner"
Write-Host "Project: $WindowsProjectPath"
Write-Host "Mutagen session: $SessionName"
Write-Host ""

cargo dev
