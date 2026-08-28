param(
    [string]$PackageRoot = "."
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path -LiteralPath $PackageRoot).Path
$manifestPath = Join-Path $root "MANIFEST.sha256"
if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "MANIFEST.sha256 is missing from $root"
}

$expected = @{}
foreach ($line in Get-Content -LiteralPath $manifestPath -Encoding ASCII) {
    if ($line -notmatch '^([0-9a-f]{64})  (.+)$') {
        throw "Malformed manifest entry: $line"
    }
    $relative = $Matches[2]
    if ([System.IO.Path]::IsPathRooted($relative) -or
        $relative.Split('/') -contains '..') {
        throw "Unsafe manifest path: $relative"
    }
    if ($expected.ContainsKey($relative)) {
        throw "Duplicate manifest path: $relative"
    }
    $expected[$relative] = $Matches[1]
}

$actual = @{}
foreach ($file in Get-ChildItem -LiteralPath $root -Recurse -File) {
    if ($file.FullName -eq $manifestPath) {
        continue
    }
    $relative = $file.FullName.Substring($root.Length + 1).Replace("\", "/")
    $actual[$relative] = $file.FullName
}

$missing = @($expected.Keys | Where-Object { -not $actual.ContainsKey($_) })
$unexpected = @($actual.Keys | Where-Object { -not $expected.ContainsKey($_) })
if ($missing.Count -ne 0) {
    throw "Files listed in the manifest are missing: $($missing -join ', ')"
}
if ($unexpected.Count -ne 0) {
    throw "Files absent from the manifest are present: $($unexpected -join ', ')"
}

foreach ($relative in $expected.Keys) {
    $hash = (Get-FileHash -LiteralPath $actual[$relative] -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($hash -ne $expected[$relative]) {
        throw "SHA-256 mismatch: $relative"
    }
}

Write-Host "Package integrity passed: $($expected.Count) file(s), no missing or unexpected entries."
