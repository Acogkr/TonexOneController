# Hardware validation record

This is the release evidence template. Target compilation does not satisfy any
item below. Record firmware archive hash, board ID, board revision, TONEX ONE
firmware version, tester, date, test equipment, and raw artifact paths for
every run.

## Connection and first run

Two USB connections are required for complete validation:

1. Connect the computer to the ESP32-S3 board's programming/debug USB port.
2. Connect TONEX ONE to the ESP32-S3 board's USB Host port.

Do not connect TONEX ONE to the computer for controller validation. The
computer flashes the board and captures its diagnostics; the board is the USB
host that controls TONEX ONE. Ensure the board's host port can supply suitable
power, or use a correctly wired powered host adapter.

Preview the exact command without touching hardware:

```powershell
.\tools\start-hardware-validation.ps1 `
    -Board waveshare-zero -Port COM5 -WifiWeb -SerialMidi -BleMidi -PlanOnly
```

After confirming the board ID and Windows COM port, remove `-PlanOnly`. The
tool builds, flashes, and opens the serial monitor while preserving its output
under `artifacts`. Stop the monitor with Ctrl+C after the run.

```powershell
.\tools\start-hardware-validation.ps1 `
    -Board waveshare-zero -Port COM5 -WifiWeb -SerialMidi -BleMidi
```

Use the board's actual ID from `rust/firmware/tonex-controller/Cargo.toml`.
The tool rejects unknown IDs, non-COM ports, long ESP-IDF target paths,
unintended log overwrites, and an unpinned flash tool.

JC3248W535 uses the native USB pins for both programming and product USB Host.
Its COM port therefore disappears after a successful boot when the firmware
takes host ownership. The validation tool disables its serial monitor by
default so this expected transition is not reported as a flash failure. Use
`-KeepMonitor` only when `-Port` identifies a separate external debug UART.
For the normal native-USB workflow, a successful flash followed by COM-port
removal is expected; continue validation from the display, touch UI, Wi-Fi Web
and the TONEX ONE host connection.

## Required representatives

Exercise at least one headless board and one board from every implemented
display bus family: SPI, QSPI, I80, and RGB. Every one of the 22 registered
build variants must ultimately receive its own power, pin, input, output, and
USB-host smoke test before its Tier can be raised.

| Run | Board ID | Serial/asset reference | Result | Notes |
| --- | --- | --- | --- | --- |
| Headless | | | Not run | |
| SPI | | | Not run | |
| QSPI | | | Not run | |
| I80 | | | Not run | |
| RGB | | | Not run | |

## TONEX ONE protocol and lifecycle

- Detect only the documented TONEX ONE VID/PID.
- Complete handshake and synchronize all 20 preset names, current preset,
  A/B/C slot, global state, 109 preset values, and supported parameters.
- Select previous/next and each A/B/C slot from every enabled input transport.
- Read, change, and save representative continuous, choice, switch, and volume
  parameters; power-cycle and verify persistence.
- Unplug during idle, receive, transmit, and synchronization; reconnect without
  reboot and verify a complete fresh synchronization.
- Inject or capture malformed, partial, combined, delayed, and failed transfers;
  verify recovery without panic, stale-ready state, or uncontrolled retries.

## Board peripherals and optional transports

- Confirm display geometry, colors, rotation, full redraw, incremental update,
  and sustained refresh without corruption.
- Confirm each touch transform, physical switch, debounce behavior, LED index,
  color order, and connection-state indication.
- Confirm Serial MIDI at 31,250 baud, running status, real-time interleaving,
  channel filtering, PC, and every supported CC mapping.
- Confirm BLE-MIDI advertise, pair/connect, command reception, disconnect,
  advertising restart, and repeated reconnect.
- Confirm NVS defaults, valid migration, invalid-record recovery, settings save,
  and cold power-cycle persistence.
- Confirm Wi-Fi AP and Station modes, WPA2 behavior, wrong credentials,
  reconnect, WebSocket commands, responsive browser layout, malformed JSON,
  offline controls, and Wi-Fi/BLE coexistence.

## Timing, memory, and endurance

Measure boot-to-USB-ready, discovery, complete synchronization, switch-to-USB
command, USB-response-to-snapshot, browser round trip, and display update
latencies. State the measurement method, sample count, median, p95, and maximum.

Capture serial output before warm-up and throughout a representative endurance
run containing repeated TONEX disconnect/reconnect plus concurrent USB, display,
touch/switch, Web, Serial MIDI, and BLE-MIDI activity. Analyze it with:

```powershell
.\tools\analyze-diagnostics.ps1 -LogPath .\device.log -RequireZeroDrops -MaxHeapDriftBytes 4096
```

The heap-drift threshold must be chosen and justified before the run. Preserve
all `DIAG` and `DIAG_TASK` records. Report display frame count, latest and
maximum render time, latest and maximum panel-present time, minimum free heap, smallest largest
free block, warm-state heap drift, main/display/touch/Serial MIDI task stack
high-water marks, reconnect cycles, all queue drops, not-ready command
rejections, unexpected command/MIDI-mapping errors, switch-read errors, LED
errors, command loss, and unexpected resets. Obtain remaining HTTP, Bluetooth,
UART-driver, and ESP-IDF internal task stack watermarks with an ESP-IDF task
snapshot or debugger trace.

## Acceptance

A run passes only when expected and observed behavior agree, measurements and
raw evidence are attached, all queue-drop counters remain zero at the declared
load, all unexpected error counters remain zero, no command is lost, no
unexpected reset occurs, and heap/fragmentation stabilize after warm-up. Explain
any nonzero `command_not_ready` count using the input/disconnection timeline.
Record defects separately; do not convert an untested
item into a pass.
