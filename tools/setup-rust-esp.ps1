[CmdletBinding()]
param(
    [switch]$InstallFlashTool
)

$ErrorActionPreference = "Stop"

$espupVersion = "0.17.1"
$espflashVersion = "4.5.0"
$ldproxyVersion = "0.3.5"

Write-Host "Installing pinned ESP Rust tools"
cargo install espup --version $espupVersion --locked
cargo install ldproxy --version $ldproxyVersion --locked

if ($InstallFlashTool) {
    cargo install cargo-espflash --version $espflashVersion --locked
}

Write-Host "Installing the ESP32-S3 Rust toolchain"
espup install --targets esp32s3

$exportFile = Join-Path $env:USERPROFILE "export-esp.ps1"
if (-not (Test-Path -LiteralPath $exportFile)) {
    throw "espup completed but did not create $exportFile"
}

Write-Host "Toolchain installed."
Write-Host "Import it in each PowerShell session with:"
Write-Host ". '$exportFile'"
Write-Host "Then build one board, for example:"
Write-Host "cargo +esp build -p tonex-controller --release --target xtensa-esp32s3-espidf --no-default-features --features board-devkitc-n8r2"

