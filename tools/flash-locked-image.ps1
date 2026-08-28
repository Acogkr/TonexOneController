param(
    [Parameter(Mandatory = $true)]
    [string]$Image,

    [Parameter(Mandatory = $true)]
    [ValidatePattern("^[0-9a-fA-F]{64}$")]
    [string]$ExpectedSha256,

    [Parameter(Mandatory = $true)]
    [ValidatePattern("^COM[0-9]+$")]
    [string]$Port,

    [ValidateSet(4, 8, 16)]
    [int]$FlashMegabytes = 16,

    [switch]$PlanOnly
)

$ErrorActionPreference = "Stop"
$resolvedImage = (Resolve-Path -LiteralPath $Image).Path
$expectedBytes = $FlashMegabytes * 1024 * 1024
$actualBytes = (Get-Item -LiteralPath $resolvedImage).Length
if ($actualBytes -ne $expectedBytes) {
    throw "Locked image size mismatch: expected $expectedBytes bytes, got $actualBytes."
}

$actualSha256 = (Get-FileHash -LiteralPath $resolvedImage -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha256 -ne $ExpectedSha256.ToLowerInvariant()) {
    throw "Locked image SHA-256 mismatch: expected $ExpectedSha256, got $actualSha256."
}

$command = @(
    "+esp", "espflash", "write-bin",
    "0x0", $resolvedImage,
    "--chip", "esp32s3",
    "--port", $Port,
    "--non-interactive",
    "--skip-update-check"
)

Write-Host "Locked image : $resolvedImage"
Write-Host "Image bytes  : $actualBytes"
Write-Host "SHA-256      : $actualSha256"
Write-Host "Target port  : $Port"
if ($PlanOnly) {
    Write-Host "Plan only: checksum verified; no flash action was performed."
    exit 0
}

$serialPort = Get-CimInstance Win32_SerialPort | Where-Object DeviceID -eq $Port
if (-not $serialPort) {
    throw "$Port is not present. Put the board in boot mode before flashing."
}

$espEnvironment = Join-Path $env:USERPROFILE "export-esp.ps1"
if (-not (Test-Path -LiteralPath $espEnvironment)) {
    throw "ESP Rust environment not found. Run tools/setup-rust-esp.ps1 -InstallFlashTool first."
}
. $espEnvironment

cargo @command
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}
