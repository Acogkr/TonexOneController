param()

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$pagePath = Join-Path $repoRoot "rust\crates\tonex-web\assets\index.html"
$node = Get-Command node -ErrorAction SilentlyContinue
if ($null -eq $node) {
    throw "Node.js is required to validate the embedded JavaScript"
}

$strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
$bytes = [System.IO.File]::ReadAllBytes($pagePath)
$page = $strictUtf8.GetString($bytes)
$script = [regex]::Match(
    $page,
    "<script>([\s\S]*?)</script>",
    [System.Text.RegularExpressions.RegexOptions]::CultureInvariant
)
if (-not $script.Success) {
    throw "Embedded JavaScript block was not found"
}

$script.Groups[1].Value | & $node.Source --check
if ($LASTEXITCODE -ne 0) {
    throw "Embedded JavaScript syntax check failed"
}

$required = @(
    'aria-live="polite"',
    'aria-busy="true"',
    'prefers-reduced-motion',
    'document.hidden?2000:200',
    'location.protocol==="https:"?"wss":"ws"',
    'Math.min(reconnectDelay*2,10000)',
    'document.documentElement.dataset.profile=state.profile||"neutral"',
    'id="presetOpen" class="preset-title"',
    'id="presetKey" class="preset-key"',
    'aria-label="Signal chain"',
    'id="presetList"',
    'id="editOpen"',
    'id="settingsOpen"',
    'data-editor="amp"',
    'data-editor="cabinet"',
    'visibleForModel',
    '++poll%3',
    'function requestSnapshot()',
    'function requestParameters(group=activeEditor)',
    'presetKey.textContent=`${String(state.number).padStart(2,"0")}-${state.slot}`'
)
foreach ($marker in $required) {
    if ($page.IndexOf($marker, [System.StringComparison]::Ordinal) -lt 0) {
        throw "Required Web UI behavior is missing: $marker"
    }
}
if ($page.IndexOf("https://", [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
    throw "Embedded page must not depend on external HTTPS assets"
}
if ($page.IndexOf('socket.send("parameters")', [System.StringComparison]::Ordinal) -ge 0) {
    throw "Embedded page must not request the full parameter registry from an FX editor"
}

Write-Host "Embedded Web UI UTF-8, JavaScript, accessibility, and offline-policy checks passed."
