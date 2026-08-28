param(
    [string]$OutputDirectory = "",
    [switch]$RunHostVerification
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory = Join-Path $repoRoot "artifacts"
}
$outputRoot = [System.IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null

if ($RunHostVerification) {
    & "$PSScriptRoot\verify-rust-migration.ps1"
    if ($LASTEXITCODE -ne 0) {
        throw "Host verification failed; no package was created"
    }
}

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$packageName = "tonex-one-rust-verification-$stamp"
$stageRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    "tonex-one-verification-" + [guid]::NewGuid().ToString("N")
)
$packageRoot = Join-Path $stageRoot $packageName
$archivePath = Join-Path $outputRoot "$packageName.zip"

$items = @(
    ".cargo",
    ".github\workflows\rust-host.yaml",
    ".github\workflows\ci.yaml",
    "docs\rust-migration",
    "rust",
    "tools",
    "Cargo.toml",
    "Cargo.lock",
    "components_esp32s3.lock",
    "deny.toml",
    "espflash.toml",
    "LICENSE",
    "README.md",
    "WORK_PROGRESS.md",
    "partitions.csv",
    "partitions.large.csv",
    "sdkconfig.defaults",
    "rust\firmware\tonex-controller\sdkconfig.web.defaults",
    "rust\firmware\tonex-controller\sdkconfig.ble.defaults",
    "legacy\README.md",
    "legacy\source\main\tonex_params.c",
    "legacy\source\main\tonex_params.h",
    "legacy\source\esp_idf_project_configuration.json"
)

try {
    New-Item -ItemType Directory -Path $packageRoot | Out-Null
    foreach ($relative in $items) {
        $sourcePath = Join-Path $repoRoot $relative
        if (-not (Test-Path -LiteralPath $sourcePath)) {
            throw "Required package input is missing: $relative"
        }

        $destinationPath = Join-Path $packageRoot $relative
        $destinationParent = Split-Path -Parent $destinationPath
        New-Item -ItemType Directory -Path $destinationParent -Force | Out-Null
        Copy-Item -LiteralPath $sourcePath -Destination $destinationPath -Recurse
    }

    $instructions = @"
# Independent verification package

This package contains the TONEX ONE-only Rust product, its vendored ESP-IDF
component, verification tools, migration documentation, and only the small
legacy evidence inputs needed to reproduce parameter and board provenance.
It intentionally excludes historical releases, generated build output, UI
editor projects, and the former mixed-device C firmware.

From the extracted package root:

1. Before extraction, compare the archive with its adjacent `.zip.sha256`
   using `Get-FileHash -Algorithm SHA256`.
2. After extraction, run: `.\tools\verify-package-integrity.ps1`
3. Install stable Rust, Python, and Node.js.
4. Install cargo-audit 0.22.2, cargo-deny 0.20.2, and cargo-machete 0.9.2.
5. Run: `.\tools\verify-rust-migration.ps1`
6. Install the pinned ESP32-S3 environment described in
   docs\rust-migration\development-environment.md.
7. Run: `.\tools\verify-rust-migration.ps1 -TargetMatrix`
8. Record the workstation, tool versions, command output, and any divergence.

Physical hardware claims are not proven by this package. Follow the outstanding
checklist in docs\rust-migration\verification.md on real hardware.
"@
    Set-Content -LiteralPath (Join-Path $packageRoot "INDEPENDENT-VERIFICATION.md") `
        -Value $instructions `
        -Encoding UTF8

    $manifestPath = Join-Path $packageRoot "MANIFEST.sha256"
    $manifestLines = Get-ChildItem -LiteralPath $packageRoot -Recurse -File |
        Where-Object { $_.FullName -ne $manifestPath } |
        Sort-Object FullName |
        ForEach-Object {
            $relativePath = $_.FullName.Substring(
                $packageRoot.Length + 1
            ).Replace("\", "/")
            $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
            "$hash  $relativePath"
        }
    Set-Content -LiteralPath $manifestPath -Value $manifestLines -Encoding ASCII

    if (Test-Path -LiteralPath $archivePath) {
        throw "Refusing to overwrite existing archive: $archivePath"
    }
    Compress-Archive -LiteralPath $packageRoot -DestinationPath $archivePath
    $archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    Set-Content -LiteralPath "$archivePath.sha256" `
        -Value "$archiveHash  $([System.IO.Path]::GetFileName($archivePath))" `
        -Encoding ASCII
} finally {
    $resolvedStage = [System.IO.Path]::GetFullPath($stageRoot)
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedStage.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -and
        (Test-Path -LiteralPath $resolvedStage)) {
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force
    }
}

Write-Host "Verification package: $archivePath"
Write-Host "Archive checksum: $archivePath.sha256"
