$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path "$PSScriptRoot\..")

Write-Host "x360ce dev launcher"
Write-Host "Using project-local dev runner; cargo-watch is not required."

cargo run --bin x360ce-dev --
