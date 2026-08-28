param(
    [Parameter(Mandatory = $true)]
    [string]$Board,

    [Parameter(Mandatory = $true)]
    [ValidatePattern("^COM[0-9]+$")]
    [string]$Port,

    [string]$TargetDirectory = "C:\t1f",
    [string]$LogPath = "",
    [int]$MonitorBaud = 115200,
    [switch]$WifiWeb,
    [switch]$SerialMidi,
    [switch]$BleMidi,
    [switch]$NoMonitor,
    [switch]$KeepMonitor,
    [switch]$PlanOnly
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$firmwareManifest = Join-Path $repoRoot "rust\firmware\tonex-controller\Cargo.toml"

$knownBoards = Get-Content -LiteralPath $firmwareManifest |
    ForEach-Object {
        if ($_ -match "^board-([a-z0-9-]+)\s*=") {
            $Matches[1]
        }
    }
if ($knownBoards -notcontains $Board) {
    throw "Unknown board '$Board'. Valid IDs: $($knownBoards -join ', ')"
}

$resolvedTarget = [System.IO.Path]::GetFullPath($TargetDirectory)
if ($resolvedTarget.Length -gt 12) {
    throw "TargetDirectory must be a short absolute path (12 characters or fewer), for example C:\t1f."
}

if ($MonitorBaud -lt 1200 -or $MonitorBaud -gt 3000000) {
    throw "MonitorBaud must be between 1200 and 3000000."
}
if ($NoMonitor -and $KeepMonitor) {
    throw "NoMonitor and KeepMonitor cannot be used together."
}

# JC3248W535 exposes its programming serial connection through the same native
# USB pins used by the product USB Host. The COM port therefore disappears
# when the firmware takes host ownership. Treat that transition as expected
# unless an external debug serial connection is explicitly being monitored.
$useMonitor = -not $NoMonitor -and ($Board -ne "jc3248w535" -or $KeepMonitor)

$features = [System.Collections.Generic.List[string]]::new()
$features.Add("board-$Board")
$fourMegabyteBoards = @("waveshare-zero", "pirate-polar-zero")
$sixteenMegabyteBoards = @(
    "waveshare-43b",
    "waveshare-35b",
    "jc3248w535",
    "lilygo-tdisplay-s3",
    "pirate-polar-43b",
    "pirate-polar-plus-v2",
    "pirate-polar-pro",
    "pirate-polar-max-v2"
)
$flashSize = if ($fourMegabyteBoards -contains $Board) {
    "4mb"
} elseif ($sixteenMegabyteBoards -contains $Board) {
    "16mb"
} else {
    "8mb"
}
$partitionTable = if ($fourMegabyteBoards -contains $Board) {
    Join-Path $repoRoot "partitions.csv"
} else {
    Join-Path $repoRoot "partitions.large.csv"
}
if (($fourMegabyteBoards -contains $Board) -and $WifiWeb -and $BleMidi) {
    throw "4 MB boards cannot fit Wi-Fi Web and Bluetooth Central together. Build Bluetooth-only or Wi-Fi-only firmware for '$Board'."
}
$sdkconfigDefaults = [System.Collections.Generic.List[string]]::new()
$sdkconfigDefaults.Add((Join-Path $repoRoot "sdkconfig.defaults"))
if ($Board -eq "jc3248w535") {
    $sdkconfigDefaults.Add(
        (Join-Path $repoRoot "rust\firmware\tonex-controller\sdkconfig.psram-octal.defaults")
    )
}
if ($WifiWeb) {
    $features.Add("wifi-web")
    $sdkconfigDefaults.Add(
        (Join-Path $repoRoot "rust\firmware\tonex-controller\sdkconfig.web.defaults")
    )
}
if ($SerialMidi) {
    $features.Add("serial-midi")
}
if ($BleMidi) {
    $features.Add("ble-midi")
    $sdkconfigDefaults.Add(
        (Join-Path $repoRoot "rust\firmware\tonex-controller\sdkconfig.ble.defaults")
    )
}

$cargoArguments = [System.Collections.Generic.List[string]]::new()
@(
    "+esp",
    "espflash",
    "flash",
    "--package", "tonex-controller",
    "--release",
    "--target", "xtensa-esp32s3-espidf",
    "--target-dir", $resolvedTarget,
    "--no-default-features",
    "--features", ($features -join ","),
    "--port", $Port,
    "--non-interactive",
    "--skip-update-check"
) | ForEach-Object { $cargoArguments.Add($_) }
$cargoArguments.Add("--partition-table")
$cargoArguments.Add($partitionTable)
$cargoArguments.Add("--flash-size")
$cargoArguments.Add($flashSize)

if ($useMonitor) {
    $cargoArguments.Add("--monitor")
    $cargoArguments.Add("--monitor-baud")
    $cargoArguments.Add($MonitorBaud.ToString())
}

if ([string]::IsNullOrWhiteSpace($LogPath)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $LogPath = Join-Path $repoRoot "artifacts\hardware-$Board-$stamp.log"
}
$resolvedLog = [System.IO.Path]::GetFullPath($LogPath)
if ($useMonitor -and (Test-Path -LiteralPath $resolvedLog)) {
    throw "Refusing to overwrite an existing hardware log: $resolvedLog"
}

$displayCommand = "cargo " + (($cargoArguments | ForEach-Object {
    if ($_ -match "\s") { "'$($_.Replace("'", "''"))'" } else { $_ }
}) -join " ")

Write-Host "Board firmware port : $Port"
Write-Host "Board               : $Board"
Write-Host "Features            : $($features -join ', ')"
Write-Host "TONEX connection    : board USB Host port (not the computer)"
Write-Host "Computer connection : board programming/debug USB port"
if ($useMonitor) {
    Write-Host "Diagnostic log      : $resolvedLog"
    Write-Host "Stop monitoring with Ctrl+C after the test."
} elseif ($Board -eq "jc3248w535" -and -not $NoMonitor) {
    Write-Host "Serial monitor      : disabled; native USB changes to USB Host after boot"
    Write-Host "External diagnostics: use -KeepMonitor only with a separate debug serial port"
}
Write-Host "Command             : $displayCommand"

if ($PlanOnly) {
    Write-Host "Plan only: no build, flash, or monitor action was performed."
    Write-Output "PLAN_COMMAND=$displayCommand"
    exit 0
}

$espEnvironment = Join-Path $env:USERPROFILE "export-esp.ps1"
if (-not (Test-Path -LiteralPath $espEnvironment)) {
    throw "ESP Rust environment not found. Run tools\setup-rust-esp.ps1 -InstallFlashTool first."
}
. $espEnvironment

$espflashVersion = & cargo espflash --version
if (
    $LASTEXITCODE -ne 0 -or
    $espflashVersion -notmatch "^cargo-espflash(?:-espflash)? 4\.5\.0$"
) {
    throw "cargo-espflash 4.5.0 is required. Run tools\setup-rust-esp.ps1 -InstallFlashTool."
}

$env:ESP_IDF_SDKCONFIG_DEFAULTS = $sdkconfigDefaults -join ";"

New-Item -ItemType Directory -Path (Split-Path -Parent $resolvedLog) -Force | Out-Null
Push-Location $repoRoot
try {
    if ($useMonitor) {
        & cargo @cargoArguments 2>&1 | Tee-Object -FilePath $resolvedLog
    } else {
        & cargo @cargoArguments
    }
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}
