param(
    [string]$TargetDirectory = "C:\tx1",
    [switch]$WifiWeb,
    [switch]$SerialMidi,
    [switch]$BleMidi
)

$ErrorActionPreference = "Stop"

$manifest = Join-Path $PSScriptRoot "..\rust\firmware\tonex-controller\Cargo.toml"
$boards = Get-Content -LiteralPath $manifest |
    ForEach-Object {
        if ($_ -match "^board-([a-z0-9-]+)\s*=") {
            $Matches[1]
        }
    }
if ($boards.Count -eq 0) {
    throw "No board features were found in $manifest"
}

$fourMegabyteBoards = @("waveshare-zero", "pirate-polar-zero")
$built = 0
$skipped = 0
foreach ($board in $boards) {
    if (($fourMegabyteBoards -contains $board) -and $WifiWeb -and $BleMidi) {
        Write-Host "Skipping unsupported 4 MB Wi-Fi+Bluetooth composition: $board"
        $skipped++
        continue
    }
    & "$PSScriptRoot\build-rust-esp.ps1" `
        -Board $board `
        -TargetDirectory $TargetDirectory `
        -WifiWeb:$WifiWeb `
        -SerialMidi:$SerialMidi `
        -BleMidi:$BleMidi
    if ($LASTEXITCODE -ne 0) {
        throw "ESP32-S3 build failed for $board"
    }
    $built++
}

Write-Host "Board matrix passed: built=$built skipped_unsupported=$skipped total=$($boards.Count)"
