param(
    [Parameter(Mandatory)]
    [string]$LogPath,
    [switch]$RequireZeroDrops,
    [int]$MaxHeapDriftBytes = -1
)

$ErrorActionPreference = "Stop"
$resolvedLog = (Resolve-Path -LiteralPath $LogPath).Path
$pattern = [regex]::new(
    "DIAG uptime_s=(\d+) heap_free=(\d+) heap_min=(\d+) " +
    "largest_block=(\d+) main_stack_hwm=(\d+) " +
    "serial_midi_drops=(\d+) ble_midi_drops=(\d+) touch_drops=(\d+) " +
    "command_not_ready=(\d+) command_errors=(\d+) midi_mapping_errors=(\d+) " +
    "switch_read_errors=(\d+) led_errors=(\d+)"
)
$samples = [System.Collections.Generic.List[object]]::new()
$taskPattern = [regex]::new(
    "DIAG_TASK name=([a-z0-9-]+) uptime_s=(\d+) stack_hwm=(\d+)" +
    "(?: frames=(\d+) render_last_us=(\d+) render_max_us=(\d+) " +
    "present_last_us=(\d+) present_max_us=(\d+))?"
)
$taskSamples = [System.Collections.Generic.List[object]]::new()

foreach ($line in Get-Content -LiteralPath $resolvedLog) {
    $match = $pattern.Match($line)
    if ($match.Success) {
        $samples.Add([PSCustomObject]@{
            UptimeSeconds = [uint64]$match.Groups[1].Value
            FreeHeapBytes = [uint64]$match.Groups[2].Value
            MinimumFreeHeapBytes = [uint64]$match.Groups[3].Value
            LargestFreeBlockBytes = [uint64]$match.Groups[4].Value
            MainStackHighWaterBytes = [uint64]$match.Groups[5].Value
            SerialMidiDrops = [uint64]$match.Groups[6].Value
            BleMidiDrops = [uint64]$match.Groups[7].Value
            TouchDrops = [uint64]$match.Groups[8].Value
            CommandNotReady = [uint64]$match.Groups[9].Value
            CommandErrors = [uint64]$match.Groups[10].Value
            MidiMappingErrors = [uint64]$match.Groups[11].Value
            SwitchReadErrors = [uint64]$match.Groups[12].Value
            LedErrors = [uint64]$match.Groups[13].Value
        })
    }
    $taskMatch = $taskPattern.Match($line)
    if ($taskMatch.Success) {
        $hasDisplayTiming = $taskMatch.Groups[4].Success
        $taskSamples.Add([PSCustomObject]@{
            Name = $taskMatch.Groups[1].Value
            UptimeSeconds = [uint64]$taskMatch.Groups[2].Value
            StackHighWaterBytes = [uint64]$taskMatch.Groups[3].Value
            Frames = if ($hasDisplayTiming) { [uint64]$taskMatch.Groups[4].Value } else { $null }
            RenderLastUs = if ($hasDisplayTiming) { [uint64]$taskMatch.Groups[5].Value } else { $null }
            RenderMaxUs = if ($hasDisplayTiming) { [uint64]$taskMatch.Groups[6].Value } else { $null }
            PresentLastUs = if ($hasDisplayTiming) { [uint64]$taskMatch.Groups[7].Value } else { $null }
            PresentMaxUs = if ($hasDisplayTiming) { [uint64]$taskMatch.Groups[8].Value } else { $null }
        })
    }
}

if ($samples.Count -eq 0) {
    throw "No valid DIAG samples found in $resolvedLog"
}

for ($index = 1; $index -lt $samples.Count; $index++) {
    $previous = $samples[$index - 1]
    $current = $samples[$index]
    if ($current.UptimeSeconds -le $previous.UptimeSeconds) {
        throw "DIAG uptime is not strictly increasing at sample $($index + 1)"
    }
    if ($current.SerialMidiDrops -lt $previous.SerialMidiDrops -or
        $current.BleMidiDrops -lt $previous.BleMidiDrops -or
        $current.TouchDrops -lt $previous.TouchDrops -or
        $current.CommandNotReady -lt $previous.CommandNotReady -or
        $current.CommandErrors -lt $previous.CommandErrors -or
        $current.MidiMappingErrors -lt $previous.MidiMappingErrors -or
        $current.SwitchReadErrors -lt $previous.SwitchReadErrors -or
        $current.LedErrors -lt $previous.LedErrors) {
        throw "DIAG drop counters decreased at sample $($index + 1)"
    }
}

foreach ($group in $taskSamples | Group-Object Name) {
    $ordered = @($group.Group)
    for ($index = 1; $index -lt $ordered.Count; $index++) {
        if ($ordered[$index].UptimeSeconds -le $ordered[$index - 1].UptimeSeconds) {
            throw "DIAG_TASK uptime is not strictly increasing for $($group.Name)"
        }
        if ($null -ne $ordered[$index].Frames -and
            $ordered[$index].Frames -lt $ordered[$index - 1].Frames) {
            throw "DIAG_TASK frame count decreased for $($group.Name)"
        }
        if ($null -ne $ordered[$index].RenderMaxUs -and
            ($ordered[$index].RenderMaxUs -lt $ordered[$index - 1].RenderMaxUs -or
                $ordered[$index].PresentMaxUs -lt $ordered[$index - 1].PresentMaxUs)) {
            throw "DIAG_TASK maximum timing decreased for $($group.Name)"
        }
    }
}

$first = $samples[0]
$last = $samples[$samples.Count - 1]
$heapDrift = [int64]$last.FreeHeapBytes - [int64]$first.FreeHeapBytes
$displaySamples = @($taskSamples | Where-Object Name -eq "display")
$displayLast = if ($displaySamples.Count -ne 0) { $displaySamples[-1] } else { $null }
$displayStackMinimum = if ($displaySamples.Count -ne 0) {
    ($displaySamples | Measure-Object StackHighWaterBytes -Minimum).Minimum
} else {
    $null
}
$touchStacks = @($taskSamples | Where-Object Name -eq "touch")
$serialMidiStacks = @($taskSamples | Where-Object Name -eq "serial-midi")
$summary = [PSCustomObject]@{
    Samples = $samples.Count
    DurationSeconds = $last.UptimeSeconds - $first.UptimeSeconds
    MinimumObservedFreeHeapBytes = ($samples | Measure-Object FreeHeapBytes -Minimum).Minimum
    MinimumReportedHeapBytes = ($samples | Measure-Object MinimumFreeHeapBytes -Minimum).Minimum
    MinimumLargestFreeBlockBytes = ($samples | Measure-Object LargestFreeBlockBytes -Minimum).Minimum
    MinimumMainStackHighWaterBytes = ($samples | Measure-Object MainStackHighWaterBytes -Minimum).Minimum
    FinalSerialMidiDrops = $last.SerialMidiDrops
    FinalBleMidiDrops = $last.BleMidiDrops
    FinalTouchDrops = $last.TouchDrops
    FinalCommandNotReady = $last.CommandNotReady
    FinalCommandErrors = $last.CommandErrors
    FinalMidiMappingErrors = $last.MidiMappingErrors
    FinalSwitchReadErrors = $last.SwitchReadErrors
    FinalLedErrors = $last.LedErrors
    FreeHeapDriftBytes = $heapDrift
    MinimumDisplayStackHighWaterBytes = $displayStackMinimum
    MinimumTouchStackHighWaterBytes = if ($touchStacks.Count -ne 0) {
        ($touchStacks | Measure-Object StackHighWaterBytes -Minimum).Minimum
    } else { $null }
    MinimumSerialMidiStackHighWaterBytes = if ($serialMidiStacks.Count -ne 0) {
        ($serialMidiStacks | Measure-Object StackHighWaterBytes -Minimum).Minimum
    } else { $null }
    DisplayFrames = if ($null -ne $displayLast) { $displayLast.Frames } else { $null }
    DisplayRenderLastUs = if ($null -ne $displayLast) { $displayLast.RenderLastUs } else { $null }
    DisplayRenderMaxUs = if ($null -ne $displayLast) { $displayLast.RenderMaxUs } else { $null }
    DisplayPresentLastUs = if ($null -ne $displayLast) { $displayLast.PresentLastUs } else { $null }
    DisplayPresentMaxUs = if ($null -ne $displayLast) { $displayLast.PresentMaxUs } else { $null }
}

$summary | Format-List

if ($RequireZeroDrops -and
    ($last.SerialMidiDrops -ne 0 -or
        $last.BleMidiDrops -ne 0 -or
        $last.TouchDrops -ne 0 -or
        $last.CommandErrors -ne 0 -or
        $last.MidiMappingErrors -ne 0 -or
        $last.SwitchReadErrors -ne 0 -or
        $last.LedErrors -ne 0)) {
    throw "Input loss or runtime hardware/command errors were observed"
}
if ($MaxHeapDriftBytes -ge 0 -and
    [Math]::Abs($heapDrift) -gt $MaxHeapDriftBytes) {
    throw "Free heap drift exceeded $MaxHeapDriftBytes bytes"
}
