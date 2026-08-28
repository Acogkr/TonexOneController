param(
    [string]$Board = "devkitc-n8r2",
    [string]$TargetDirectory = "C:\tx1",
    [switch]$WifiWeb,
    [switch]$SerialMidi,
    [switch]$BleMidi,
    [switch]$UsbDiagnostics,
    [switch]$TouchDisabledDiagnostics,
    [string]$OutputImage = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$resolvedTarget = [System.IO.Path]::GetFullPath($TargetDirectory)
if ($resolvedTarget.Length -gt 12) {
    throw "TargetDirectory must be a short absolute path (12 characters or fewer), for example C:\tx1."
}
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
if (($fourMegabyteBoards -contains $Board) -and $WifiWeb -and $BleMidi) {
    throw "4 MB boards cannot fit Wi-Fi Web and Bluetooth Central together. Build Bluetooth-only or Wi-Fi-only firmware for '$Board'."
}

$espEnvironment = Join-Path $env:USERPROFILE "export-esp.ps1"
if (-not (Test-Path -LiteralPath $espEnvironment)) {
    throw "ESP Rust environment not found. Run tools/setup-rust-esp.ps1 first."
}

. $espEnvironment
$env:CARGO_TARGET_DIR = $resolvedTarget
$features = "board-$Board"
$sdkconfigDefaults = [System.Collections.Generic.List[string]]::new()
$sdkconfigDefaults.Add((Join-Path $repoRoot "sdkconfig.defaults"))
if ($Board -eq "jc3248w535") {
    $sdkconfigDefaults.Add(
        (Join-Path $repoRoot "rust\firmware\tonex-controller\sdkconfig.psram-octal.defaults")
    )
}
if ($WifiWeb) {
    $features += ",wifi-web"
    $sdkconfigDefaults.Add(
        (Join-Path $PSScriptRoot "..\rust\firmware\tonex-controller\sdkconfig.web.defaults")
    )
}
if ($SerialMidi) {
    $features += ",serial-midi"
}
if ($BleMidi) {
    $features += ",ble-midi"
    $sdkconfigDefaults.Add(
        (Join-Path $PSScriptRoot "..\rust\firmware\tonex-controller\sdkconfig.ble.defaults")
    )
}
if ($UsbDiagnostics) {
    $features += ",usb-connection-diagnostics"
}
if ($TouchDisabledDiagnostics) {
    $features += ",touch-disabled-diagnostics"
}
$env:ESP_IDF_SDKCONFIG_DEFAULTS = $sdkconfigDefaults -join ";"

Push-Location $repoRoot
try {
    cargo +esp build `
        -p tonex-controller `
        --release `
        --target xtensa-esp32s3-espidf `
        --no-default-features `
        --features $features
    $buildExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}

if ($buildExitCode -ne 0) {
    exit $buildExitCode
}

if (-not [string]::IsNullOrWhiteSpace($OutputImage)) {
    $resolvedOutput = [System.IO.Path]::GetFullPath($OutputImage)
    if (Test-Path -LiteralPath $resolvedOutput) {
        throw "Refusing to overwrite an existing firmware image: $resolvedOutput"
    }
    $outputParent = Split-Path -Parent $resolvedOutput
    if (-not (Test-Path -LiteralPath $outputParent -PathType Container)) {
        throw "Firmware image parent directory does not exist: $outputParent"
    }
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

    Push-Location $repoRoot
    try {
        cargo +esp espflash save-image `
            --chip esp32s3 `
            --merge `
            --flash-size $flashSize `
            --partition-table $partitionTable `
            --package tonex-controller `
            --release `
            --target xtensa-esp32s3-espidf `
            --target-dir $resolvedTarget `
            --no-default-features `
            --features $features `
            $resolvedOutput
        $imageExitCode = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    if ($imageExitCode -ne 0) {
        exit $imageExitCode
    }
}
