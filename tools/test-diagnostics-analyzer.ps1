$ErrorActionPreference = "Stop"

& "$PSScriptRoot\analyze-diagnostics.ps1" `
    -LogPath "$PSScriptRoot\testdata\diagnostics-sample.log" `
    -RequireZeroDrops `
    -MaxHeapDriftBytes 4096

$invalidRejected = $false
try {
    & "$PSScriptRoot\analyze-diagnostics.ps1" `
        -LogPath "$PSScriptRoot\testdata\diagnostics-bad-task.log" *> $null
} catch {
    $invalidRejected = $_.Exception.Message.Contains("frame count decreased")
}
if (-not $invalidRejected) {
    throw "The diagnostics analyzer accepted decreasing display evidence."
}

Write-Host "Diagnostic analyzer positive and negative fixtures passed."
