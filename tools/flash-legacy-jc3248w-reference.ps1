param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern("^COM[0-9]+$")]
    [string]$Port,

    [switch]$PlanOnly
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$package = Join-Path $repoRoot "legacy\build_distrib\TonexController_V2.0.4.2_beta5_JC3248W"
$parts = @(
    @{ Name = "bootloader.bin"; Offset = "0x0"; Sha256 = "d9e7a8d806250495a664d33bb313748c0e8d4c4a58fedec5e5cc1ba9e9156668" },
    @{ Name = "partition-table.bin"; Offset = "0x8000"; Sha256 = "dda453342c9ca946f3368d6aa0bcae527546ec36c0c3fffe49da5b7eb230cc0e" },
    @{ Name = "ota_data_initial.bin"; Offset = "0xd000"; Sha256 = "7d2c7ac4888bfd75cd5f56e8d61f69595121183afc81556c876732fd3782c62f" },
    @{ Name = "TonexController.bin"; Offset = "0x10000"; Sha256 = "c1c3fa9c9ed170320a845211d88a8a30ba67ff134189ad651bb13d75ee3cc0ff" },
    @{ Name = "skins.bin"; Offset = "0x4f2000"; Sha256 = "35261d0993123cfd3ccca2011d9555e54ab919a21d44e058478291aadfc71d8e" }
)

foreach ($part in $parts) {
    $path = Join-Path $package $part.Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing legacy reference part: $path"
    }
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    if ($actual -ne $part.Sha256) {
        throw "Legacy reference checksum mismatch for $($part.Name): $actual"
    }
    Write-Host "$($part.Offset) $($part.Name) $actual"
}

if ($PlanOnly) {
    Write-Host "Plan only: all five legacy reference parts verified; flash was not modified."
    exit 0
}

$espEnvironment = Join-Path $env:USERPROFILE "export-esp.ps1"
if (-not (Test-Path -LiteralPath $espEnvironment)) {
    throw "ESP Rust environment not found: $espEnvironment"
}
. $espEnvironment

cargo +esp espflash erase-flash --chip esp32s3 --port $Port --non-interactive --skip-update-check
if ($LASTEXITCODE -ne 0) {
    throw "Legacy reference erase failed."
}

for ($index = 0; $index -lt $parts.Count; $index++) {
    $part = $parts[$index]
    $path = Join-Path $package $part.Name
    $after = if ($index -eq $parts.Count - 1) { "hard-reset" } else { "no-reset" }
    cargo +esp espflash write-bin $part.Offset $path `
        --chip esp32s3 `
        --port $Port `
        --non-interactive `
        --after $after `
        --skip-update-check
    if ($LASTEXITCODE -ne 0) {
        throw "Legacy reference write failed at $($part.Offset): $($part.Name)"
    }
}

Write-Host "Legacy JC3248W reference firmware flashed successfully."
