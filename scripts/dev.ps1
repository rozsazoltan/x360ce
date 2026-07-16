$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path "$PSScriptRoot\..")
& "$PSScriptRoot\resolve-cmake.ps1"

Write-Host "x360ce dev launcher"
Write-Host "Using project-local dev runner; cargo-watch is not required."

cargo dev
