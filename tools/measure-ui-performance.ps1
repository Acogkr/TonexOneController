param(
    [string]$OutputPath = "",
    [ValidateRange(1, 1000000)]
    [int]$RenderSamples = 100
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $OutputPath = Join-Path $repoRoot "artifacts\ui-performance-$stamp.log"
}
$resolvedOutput = [System.IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $resolvedOutput) {
    throw "Refusing to overwrite an existing measurement: $resolvedOutput"
}

$processor = (Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name).Trim()
$header = @(
    "MEASUREMENT utc=$([DateTime]::UtcNow.ToString('o'))"
    "HOST cpu=$processor"
    "RUST $(& rustc --version)"
)

Push-Location $repoRoot
try {
    $render = & cargo run --release -q -p tonex-ui-renderer --example benchmark_render -- $RenderSamples
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    $web = & cargo run --release -q -p tonex-web --example benchmark_snapshot
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

New-Item -ItemType Directory -Path (Split-Path -Parent $resolvedOutput) -Force | Out-Null
@($header; $render; $web) | Set-Content -LiteralPath $resolvedOutput -Encoding ASCII
Get-Content -LiteralPath $resolvedOutput
Write-Host "Measurement saved: $resolvedOutput"
