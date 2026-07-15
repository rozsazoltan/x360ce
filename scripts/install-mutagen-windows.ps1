param(
    [string]$Version = "latest",
    [string]$InstallDirectory = (Join-Path $env:LOCALAPPDATA "Programs\Mutagen"),
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Get-MutagenArchitecture {
    $Architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    switch ($Architecture) {
        "X64" { return "amd64" }
        "Arm64" { return "arm64" }
        default { throw "Unsupported Windows architecture: $Architecture. Mutagen installer supports x64 and ARM64." }
    }
}

function Invoke-GitHubApi {
    param([string]$Uri)

    $Headers = @{
        Accept = "application/vnd.github+json"
        "User-Agent" = "x360ce-mutagen-installer"
        "X-GitHub-Api-Version" = "2022-11-28"
    }
    return Invoke-RestMethod -Uri $Uri -Headers $Headers
}

function Add-ToUserPath {
    param([string]$Directory)

    $NormalizedDirectory = [System.IO.Path]::GetFullPath($Directory).TrimEnd('\')
    $CurrentUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $PathEntries = @()
    if (-not [string]::IsNullOrWhiteSpace($CurrentUserPath)) {
        $PathEntries = $CurrentUserPath.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)
    }

    $AlreadyPresent = $PathEntries | Where-Object {
        try {
            [System.IO.Path]::GetFullPath($_).TrimEnd('\') -ieq $NormalizedDirectory
        } catch {
            $_.TrimEnd('\') -ieq $NormalizedDirectory
        }
    }

    if (-not $AlreadyPresent) {
        $UpdatedPath = (@($PathEntries) + $NormalizedDirectory) -join ';'
        [Environment]::SetEnvironmentVariable("Path", $UpdatedPath, "User")
    }

    $ProcessEntries = $env:Path.Split(';', [System.StringSplitOptions]::RemoveEmptyEntries)
    if (-not ($ProcessEntries | Where-Object { $_.TrimEnd('\') -ieq $NormalizedDirectory })) {
        $env:Path = "$NormalizedDirectory;$env:Path"
    }
}

if ([string]::IsNullOrWhiteSpace($InstallDirectory)) {
    throw "Install directory is required."
}

$InstallDirectory = [System.IO.Path]::GetFullPath($InstallDirectory)
$InstalledExe = Join-Path $InstallDirectory "mutagen.exe"

if ((Test-Path -LiteralPath $InstalledExe) -and -not $Force -and $Version -eq "latest") {
    Add-ToUserPath $InstallDirectory
    & $InstalledExe version
    Write-Host "Mutagen already installed: $InstalledExe"
    return
}

$Architecture = Get-MutagenArchitecture
$ReleaseApi = if ($Version -eq "latest") {
    "https://api.github.com/repos/mutagen-io/mutagen/releases/latest"
} else {
    $NormalizedVersion = $Version.Trim()
    if ($NormalizedVersion -notmatch '^v') {
        $NormalizedVersion = "v$NormalizedVersion"
    }
    "https://api.github.com/repos/mutagen-io/mutagen/releases/tags/$NormalizedVersion"
}

Write-Host "Resolving Mutagen release..."
$Release = Invoke-GitHubApi $ReleaseApi
$AssetPattern = "^mutagen_windows_${Architecture}_v[^/]+\.zip$"
$Asset = $Release.assets | Where-Object { $_.name -match $AssetPattern } | Select-Object -First 1
if (-not $Asset) {
    $Available = ($Release.assets | ForEach-Object { $_.name }) -join ", "
    throw "Could not find Windows $Architecture Mutagen archive in release $($Release.tag_name). Available assets: $Available"
}

$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("x360ce-mutagen-" + [guid]::NewGuid().ToString("N"))
$ArchivePath = Join-Path $TempRoot $Asset.name
$ExtractPath = Join-Path $TempRoot "extract"

try {
    New-Item -ItemType Directory -Force -Path $ExtractPath | Out-Null
    Write-Host "Downloading $($Asset.name)..."
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile $ArchivePath -Headers @{ "User-Agent" = "x360ce-mutagen-installer" }

    if ($Asset.PSObject.Properties.Name -contains "digest" -and -not [string]::IsNullOrWhiteSpace($Asset.digest)) {
        if ($Asset.digest -match '^sha256:(?<hash>[0-9A-Fa-f]{64})$') {
            $ActualHash = (Get-FileHash -LiteralPath $ArchivePath -Algorithm SHA256).Hash
            if ($ActualHash -ine $Matches.hash) {
                throw "Mutagen archive SHA-256 mismatch. Expected $($Matches.hash), got $ActualHash."
            }
        }
    }

    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractPath -Force
    $ExtractedExe = Get-ChildItem -LiteralPath $ExtractPath -Recurse -File -Filter "mutagen.exe" | Select-Object -First 1
    if (-not $ExtractedExe) {
        throw "Downloaded Mutagen archive does not contain mutagen.exe."
    }

    if (Test-Path -LiteralPath $InstalledExe) {
        try {
            & $InstalledExe daemon stop | Out-Null
        } catch {
            Write-Warning "Existing Mutagen daemon could not be stopped cleanly: $($_.Exception.Message)"
        }
    }

    New-Item -ItemType Directory -Force -Path $InstallDirectory | Out-Null
    $PayloadRoot = $ExtractedExe.Directory.FullName
    Get-ChildItem -LiteralPath $PayloadRoot -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $InstallDirectory $_.Name) -Force
    }

    if (-not (Test-Path -LiteralPath $InstalledExe)) {
        throw "Mutagen installation failed: $InstalledExe was not created."
    }

    Add-ToUserPath $InstallDirectory
    Write-Host "Mutagen installed: $InstalledExe"
    & $InstalledExe version
} finally {
    if (Test-Path -LiteralPath $TempRoot) {
        Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
