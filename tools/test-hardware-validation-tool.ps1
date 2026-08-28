$ErrorActionPreference = "Stop"
$tool = Join-Path $PSScriptRoot "start-hardware-validation.ps1"
$buildTool = Join-Path $PSScriptRoot "build-rust-esp.ps1"
$manifest = Join-Path $PSScriptRoot "..\rust\firmware\tonex-controller\Cargo.toml"
$boards = Get-Content -LiteralPath $manifest |
    ForEach-Object {
        if ($_ -match "^board-([a-z0-9-]+)\s*=") {
            $Matches[1]
        }
    }

if ($boards.Count -ne 22 -or ($boards | Select-Object -Unique).Count -ne 22) {
    throw "Expected exactly 22 unique board features, found $($boards.Count)."
}

$output = & $tool `
    -Board waveshare-zero `
    -Port COM5 `
    -TargetDirectory C:\t1f `
    -WifiWeb `
    -SerialMidi `
    -PlanOnly 6>&1 |
    Out-String
if ($LASTEXITCODE -ne 0) {
    throw "Hardware validation plan unexpectedly failed."
}

$required = @(
    "board-waveshare-zero,wifi-web,serial-midi",
    "--port COM5",
    "--target-dir C:\t1f",
    "--monitor-baud 115200",
    "PLAN_COMMAND=cargo +esp espflash flash"
)
foreach ($marker in $required) {
    if (-not $output.Contains($marker)) {
        throw "Hardware validation plan is missing: $marker"
    }
}

$oversizedFourMegabyteBuildRejected = $false
try {
    & $tool `
        -Board waveshare-zero `
        -Port COM5 `
        -TargetDirectory C:\t1f `
        -WifiWeb `
        -BleMidi `
        -PlanOnly *> $null
} catch {
    $oversizedFourMegabyteBuildRejected = $_.Exception.Message.Contains(
        "4 MB boards cannot fit Wi-Fi Web and Bluetooth Central together"
    )
}
if (-not $oversizedFourMegabyteBuildRejected) {
    throw "The unsupported 4 MB Wi-Fi plus Bluetooth composition was not rejected."
}

$standaloneOversizedBuildRejected = $false
try {
    & $buildTool `
        -Board waveshare-zero `
        -TargetDirectory C:\t1f `
        -WifiWeb `
        -BleMidi *> $null
} catch {
    $standaloneOversizedBuildRejected = $_.Exception.Message.Contains(
        "4 MB boards cannot fit Wi-Fi Web and Bluetooth Central together"
    )
}
if (-not $standaloneOversizedBuildRejected) {
    throw "The standalone build tool accepted unsupported 4 MB Wi-Fi plus Bluetooth."
}

$jcOutput = & $tool `
    -Board jc3248w535 `
    -Port COM4 `
    -TargetDirectory C:\t1f `
    -WifiWeb `
    -PlanOnly 6>&1 |
    Out-String
if (
    $LASTEXITCODE -ne 0 -or
    $jcOutput.Contains("--monitor") -or
    -not $jcOutput.Contains("native USB changes to USB Host")
) {
    throw "JC3248W535 plan did not account for native USB Host takeover."
}

$invalidBoardRejected = $false
try {
    & $tool -Board invalid-board -Port COM5 -PlanOnly *> $null
} catch {
    $invalidBoardRejected = $_.Exception.Message.Contains("Unknown board")
}
if (-not $invalidBoardRejected) {
    throw "Unknown board was not rejected."
}

$longPathRejected = $false
try {
    & $tool `
        -Board waveshare-zero `
        -Port COM5 `
        -TargetDirectory C:\target-directory-too-long `
        -PlanOnly *> $null
} catch {
    $longPathRejected = $_.Exception.Message.Contains("short absolute path")
}
if (-not $longPathRejected) {
    throw "Long ESP-IDF target path was not rejected."
}

$buildLongPathRejected = $false
try {
    & $buildTool `
        -Board waveshare-zero `
        -TargetDirectory C:\target-directory-too-long *> $null
} catch {
    $buildLongPathRejected = $_.Exception.Message.Contains("short absolute path")
}
if (-not $buildLongPathRejected) {
    throw "The standalone build tool did not reject a long ESP-IDF target path."
}

$buildSource = Get-Content -Raw -LiteralPath $buildTool
if (-not $buildSource.Contains('Push-Location $repoRoot')) {
    throw "The standalone build tool is not anchored to its own repository root."
}

Write-Host "Hardware validation tool checks passed for the 22-board registry."
