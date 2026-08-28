$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$rustRoot = Join-Path $repoRoot "rust"
$commonArguments = @(
    $rustRoot,
    "-g", "*.rs",
    "-g", "!vendor/**",
    "-g", "!crates/tonex-parameters/src/generated.rs"
)

function Find-Matches {
    param(
        [Parameter(Mandatory)]
        [string]$Pattern
    )

    $matches = & rg -n --pcre2 $Pattern @commonArguments 2>$null
    if ($LASTEXITCODE -eq 0) {
        return @($matches)
    }
    if ($LASTEXITCODE -eq 1) {
        return @()
    }
    throw "Unable to scan managed Rust sources."
}

$debtMarkers = Find-Matches '\b(?:TODO|FIXME|HACK|XXX)\b'
$commentedCode = Find-Matches (
    '^\s*//\s*(?:let\s|fn\s|pub\s|if\s|else\b|match\s|loop\b|' +
    'while\s|for\s+\w+\s+in\b|use\s|mod\s|impl\s|struct\s|enum\s|' +
    'const\s|static\s|return\b|[A-Za-z_][A-Za-z0-9_]*!\s*\()'
)

$violations = @($debtMarkers) + @($commentedCode)
if ($violations.Count -ne 0) {
    $violations | ForEach-Object { Write-Error $_ }
    throw "Managed Rust source hygiene failed with $($violations.Count) violation(s)."
}

$comments = Find-Matches '^\s*//'
$safetyComments = Find-Matches '^\s*//\s*SAFETY:'
Write-Host (
    "Managed Rust source hygiene passed: " +
    "$($comments.Count) comment line(s), " +
    "$($safetyComments.Count) unsafe rationale header(s), " +
    "no debt markers or commented-out code."
)
$global:LASTEXITCODE = 0
