$ErrorActionPreference = "Stop"

function Test-CMakeExecutable {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path) -and -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }

    try {
        $StartInfo = [System.Diagnostics.ProcessStartInfo]::new()
        $StartInfo.FileName = $Path
        $StartInfo.Arguments = "--version"
        $StartInfo.UseShellExecute = $false
        $StartInfo.RedirectStandardOutput = $true
        $StartInfo.RedirectStandardError = $true
        $StartInfo.CreateNoWindow = $true

        $Process = [System.Diagnostics.Process]::new()
        $Process.StartInfo = $StartInfo
        if (-not $Process.Start()) {
            return $false
        }
        $Output = $Process.StandardOutput.ReadToEnd()
        $Process.StandardError.ReadToEnd() | Out-Null
        $Process.WaitForExit()
        return $Process.ExitCode -eq 0 -and $Output -match 'cmake version'
    } catch {
        return $false
    }
}

function Add-CMakeCandidate {
    param(
        [System.Collections.Generic.List[string]]$Candidates,
        [string]$Path
    )

    if (-not [string]::IsNullOrWhiteSpace($Path) -and -not $Candidates.Contains($Path)) {
        $Candidates.Add($Path)
    }
}

$Candidates = [System.Collections.Generic.List[string]]::new()
Add-CMakeCandidate $Candidates $env:CMAKE

Get-Command cmake.exe -All -ErrorAction SilentlyContinue | ForEach-Object {
    Add-CMakeCandidate $Candidates $_.Source
}

foreach ($Base in @($env:ProgramFiles, ${env:ProgramFiles(x86)})) {
    if ([string]::IsNullOrWhiteSpace($Base)) {
        continue
    }

    Add-CMakeCandidate $Candidates (Join-Path $Base 'CMake\bin\cmake.exe')

    $VisualStudioRoot = Join-Path $Base 'Microsoft Visual Studio\2022'
    if (Test-Path -LiteralPath $VisualStudioRoot -PathType Container) {
        Get-ChildItem -LiteralPath $VisualStudioRoot -Directory -ErrorAction SilentlyContinue | ForEach-Object {
            Add-CMakeCandidate $Candidates (Join-Path $_.FullName 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe')
        }
    }
}

$VsWhere = $null
if (-not [string]::IsNullOrWhiteSpace(${env:ProgramFiles(x86)})) {
    $VsWhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
}
if ($VsWhere -and (Test-Path -LiteralPath $VsWhere -PathType Leaf)) {
    $Installations = & $VsWhere -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    foreach ($Installation in @($Installations)) {
        if (-not [string]::IsNullOrWhiteSpace($Installation)) {
            Add-CMakeCandidate $Candidates (Join-Path $Installation.Trim() 'Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe')
        }
    }
}

$ResolvedCMake = $null
foreach ($Candidate in $Candidates) {
    if (Test-CMakeExecutable $Candidate) {
        $ResolvedCMake = (Resolve-Path -LiteralPath $Candidate -ErrorAction SilentlyContinue).Path
        if ([string]::IsNullOrWhiteSpace($ResolvedCMake)) {
            $ResolvedCMake = $Candidate
        }
        break
    }
}

if ([string]::IsNullOrWhiteSpace($ResolvedCMake)) {
    throw @"
Working CMake executable not found.
Install Visual Studio 2022 Desktop development with C++ including C++ CMake tools for Windows, or install standalone CMake.
If mise owns cmake shim, install managed CMake with: mise use -g cmake@latest
"@
}

$env:CMAKE = $ResolvedCMake
$CMakeDirectory = Split-Path -Parent $ResolvedCMake
if (-not [string]::IsNullOrWhiteSpace($CMakeDirectory)) {
    $PathEntries = @($env:PATH -split ';' | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($PathEntries -notcontains $CMakeDirectory) {
        $env:PATH = "$CMakeDirectory;$env:PATH"
    }
}

Write-Host "CMake: $ResolvedCMake"
