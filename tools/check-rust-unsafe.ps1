$ErrorActionPreference = "Stop"

$allowedCrates = @(
    "esp-idf-board-io",
    "esp-idf-display",
    "esp-idf-diagnostics",
    "esp-idf-i2c",
    "esp-idf-midi",
    "esp-idf-touch",
    "esp-idf-usb-host"
)

$rustRoot = Join-Path $PSScriptRoot "..\rust"
$violations = [System.Collections.Generic.List[string]]::new()
$unsafeCount = 0

$files = & rg --files $rustRoot |
    Where-Object { $_ -match "\.rs$" }

foreach ($file in $files) {
    $normalized = $file.Replace("\", "/")
    $isAllowed = $false
    foreach ($crate in $allowedCrates) {
        if ($normalized -match "/crates/$crate/") {
            $isAllowed = $true
            break
        }
    }

    $lines = Get-Content -LiteralPath $file
    for ($index = 0; $index -lt $lines.Count; $index++) {
        if ($lines[$index] -notmatch "\bunsafe\s*\{") {
            continue
        }

        $unsafeCount++
        $location = "${file}:$($index + 1)"
        if (-not $isAllowed) {
            $violations.Add("$location is outside an approved hardware-boundary crate")
        }

        $contextStart = [Math]::Max(0, $index - 3)
        $context = $lines[$contextStart..$index] -join "`n"
        if ($context -notmatch "SAFETY:") {
            $violations.Add("$location has no SAFETY explanation in the preceding three lines")
        }
    }
}

if ($violations.Count -ne 0) {
    $violations | ForEach-Object { Write-Error $_ }
    throw "Rust unsafe audit failed with $($violations.Count) violation(s)"
}

Write-Host "Rust unsafe audit passed: $unsafeCount documented block(s) in approved boundary crates."
