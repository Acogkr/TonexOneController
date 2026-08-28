$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$rustRoot = Join-Path $repoRoot "rust"
$legacyWorkflow = Join-Path $repoRoot ".github\workflows\ci.yaml"
$violations = [System.Collections.Generic.List[string]]::new()

$forbidden = "(?i)valeton|gp-?5|tonex\s+pedal|max_presets\s*(?:=|:)\s*150"
$matches = & rg -n --pcre2 $forbidden $rustRoot 2>$null
if ($LASTEXITCODE -eq 0) {
    foreach ($match in $matches) {
        $violations.Add("forbidden mixed-device product reference: $match")
    }
} elseif ($LASTEXITCODE -ne 1) {
    throw "Unable to scan Rust product sources"
}

$manifestReferences = & rg -n "(?i)source[/\\]" `
    (Join-Path $repoRoot "Cargo.toml") `
    $rustRoot `
    -g "Cargo.toml" 2>$null
if ($LASTEXITCODE -eq 0) {
    foreach ($reference in $manifestReferences) {
        $violations.Add("Rust product manifest reaches into the legacy source tree: $reference")
    }
} elseif ($LASTEXITCODE -ne 1) {
    throw "Unable to scan Rust product manifests"
}

$workflow = Get-Content -Raw -LiteralPath $legacyWorkflow
if ($workflow -match "(?m)^\s*(push|pull_request):") {
    $violations.Add(
        "$legacyWorkflow must remain manual-only and cannot run on push or pull_request"
    )
}
if ($workflow -notmatch "(?m)^\s*workflow_dispatch:") {
    $violations.Add("$legacyWorkflow has no explicit manual trigger")
}

if ($violations.Count -ne 0) {
    $violations | ForEach-Object { Write-Error $_ }
    throw "Rust product-scope audit failed with $($violations.Count) violation(s)"
}

Write-Host "Rust product-scope audit passed: TONEX ONE-only product paths; legacy C workflow is manual-only."
$global:LASTEXITCODE = 0
