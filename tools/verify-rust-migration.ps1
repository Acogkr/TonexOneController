param(
    [switch]$TargetMatrix,
    [string]$TargetDirectory = "C:\txverify"
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Invoke-Checked {
    param(
        [Parameter(Mandatory)]
        [string]$Label,
        [Parameter(Mandatory)]
        [scriptblock]$Command
    )

    Write-Host "==> $Label"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

Push-Location $repoRoot
try {
    Invoke-Checked "Rust formatting" { cargo fmt --all -- --check }
    Invoke-Checked "Generated TONEX parameter registry" {
        python tools/generate-tonex-parameters.py --check
    }
    Invoke-Checked "Embedded Web UI" {
        & "$PSScriptRoot\check-web-assets.ps1"
    }
    Invoke-Checked "Diagnostic log analyzer fixture" {
        & "$PSScriptRoot\test-diagnostics-analyzer.ps1"
    }
    Invoke-Checked "Hardware validation tool" {
        & "$PSScriptRoot\test-hardware-validation-tool.ps1"
    }
    Invoke-Checked "Rust lints" {
        cargo clippy --workspace --all-targets -- -D warnings
    }
    Invoke-Checked "Rust tests" { cargo test --workspace }
    Invoke-Checked "Unused dependency audit" { cargo machete }
    Invoke-Checked "Unsafe boundary audit" {
        & "$PSScriptRoot\check-rust-unsafe.ps1"
    }
    Invoke-Checked "TONEX ONE-only product scope" {
        & "$PSScriptRoot\check-rust-product-scope.ps1"
    }
    Invoke-Checked "Managed source hygiene" {
        & "$PSScriptRoot\check-source-hygiene.ps1"
    }
    Invoke-Checked "Dependency advisories" { cargo audit }
    Invoke-Checked "Dependency policy" { cargo deny check }

    if ($TargetMatrix) {
        Invoke-Checked "22-board core ESP32-S3 matrix" {
            & "$PSScriptRoot\check-all-rust-boards.ps1" `
                -TargetDirectory $TargetDirectory
        }
        Invoke-Checked "22-board BLE-only ESP32-S3 matrix" {
            & "$PSScriptRoot\check-all-rust-boards.ps1" `
                -TargetDirectory $TargetDirectory `
                -BleMidi
        }
        Invoke-Checked "22-board Wi-Fi Web plus Serial MIDI ESP32-S3 matrix" {
            & "$PSScriptRoot\check-all-rust-boards.ps1" `
                -TargetDirectory $TargetDirectory `
                -WifiWeb `
                -SerialMidi
        }
        Invoke-Checked "20-board supported maximum-feature ESP32-S3 matrix" {
            & "$PSScriptRoot\check-all-rust-boards.ps1" `
                -TargetDirectory $TargetDirectory `
                -WifiWeb `
                -SerialMidi `
                -BleMidi
        }
    }
} finally {
    Pop-Location
}

Write-Host "Rust migration verification passed."
