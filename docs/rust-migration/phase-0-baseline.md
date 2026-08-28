# Phase 0 baseline

Status: in progress  
Date: 2026-07-30

## Scope and evidence

The current product is an ESP-IDF 5.5.1 ESP32-S3 firmware containing TONEX
ONE, full-size TONEX Pedal, and Valeton GP-5 support. The new firmware is
TONEX ONE-only. The existing C firmware remains unchanged and is the
behavioral reference.

This checkout does not contain `.git` metadata, so history, branches, and
local modifications cannot be audited. `idf.py` is not available in the
current shell. Consequently, the legacy hardware matrix has not yet been
rebuilt locally. Its GitHub Actions definition builds every key from
`legacy/source/esp_idf_project_configuration.json` with ESP-IDF 5.5.1.

The installed host Rust toolchain is stable Rust 1.91.1 for
`x86_64-pc-windows-msvc`. Host-only Rust crates can therefore be tested
before the Xtensa/ESP-IDF toolchain is introduced.

## Legacy startup order

`legacy/source/main/main.c::app_main` performs:

1. NVS/configuration load and legacy migration.
2. I2C mutex and bus initialization.
3. TONEX and Valeton parameter table initialization.
4. Control queue/task initialization.
5. compile-time-selected board initialization.
6. display/LVGL initialization.
7. footswitch initialization.
8. optional BLE MIDI and serial MIDI initialization.
9. USB host initialization.
10. optional Wi-Fi/web initialization.

This initialization is order-dependent and relies on global/static handles.
The Rust replacement must make dependencies explicit during system
composition.

## Legacy task model

| Task | Core | Relative priority | Responsibility |
| --- | ---: | ---: | --- |
| USB host daemon | configured by implementation | idle + 4 | USB library events |
| USB class driver | configured by implementation | idle + 4 | enumeration and model dispatch |
| Control | 1 | idle + 3 | central mutable application state |
| Display | 1 | idle + 2 | LVGL and queued UI updates |
| Serial MIDI | 1 | idle + 2 | UART MIDI receive |
| Footswitch | 1 | idle + 1 | polling, debounce, combinations |
| Wi-Fi | 0 | idle + 1 | Wi-Fi, HTTP, WebSocket |

The control queue is a useful single-owner pattern, but the legacy code
breaks that ownership through direct getters, returned internal pointers,
and direct state changes. The Rust `AppController` will exclusively own
mutable application state and expose copyable snapshots.

## TONEX ONE transport baseline

Evidence: `legacy/source/main/usb_tonex_common.c` and
`legacy/source/main/usb_tonex_one.c`.

- USB vendor ID: `0x1963`.
- TONEX ONE product ID: `0x00d1`.
- CDC interface index: `0`.
- preset count: 20.
- preset name payload capacity: 32 bytes.
- framing boundary: `0x7e`.
- escape byte: `0x7d`.
- escaped value: original byte XOR `0x20`.
- CRC: CRC-16/X-25, initial `0xffff`, reversed polynomial `0x8408`,
  complemented output.
- CRC wire order: little-endian.
- input buffers: three.
- maximum cached state data: 512 bytes.
- legacy communication states: idle, hello, ready, get-state.
- slots: A, B, C.

The initial Rust `tonex-protocol` crate implements and tests this framing
without interpreting undocumented message bodies.

## Legacy board build variants

The authoritative legacy list is
`legacy/source/esp_idf_project_configuration.json`. Every entry is initially Tier 3
in the Rust registry because neither target compilation nor physical
hardware testing has occurred.

| Legacy build | Rust board ID | UI class |
| --- | --- | --- |
| WS169 | waveshare-169 | Portrait 240×280 |
| WS169Land | waveshare-169-landscape | Landscape 280×240 |
| WS169TOUCH | waveshare-169-touch | Portrait 240×280 |
| WS169TOUCHLand | waveshare-169-touch-landscape | Landscape 280×240 |
| WS43B | waveshare-43b | Large 800×480 |
| WS35B | waveshare-35b | Medium 480×320 |
| JC3248W | jc3248w535 | Medium 480×320 |
| WSZERO | waveshare-zero | Headless |
| DEVKITC_N8R2 | devkitc-n8r2 | Headless |
| DEVKITC_N16R8 | devkitc-n16r8 | Headless |
| M5AtomS3R | m5-atoms3r | Tiny 128×128 |
| LGTDisplayS3 | lilygo-tdisplay-s3 | Compact 320×170 |
| WS19Touch | waveshare-19-touch | Compact 320×170 |
| WS7_43 | waveshare-7-43 | Large 800×480 |
| PirateMini | pirate-polar-mini | Portrait 240×280 |
| PiratePlus | pirate-polar-plus | Landscape 280×240 |
| PirateZERO | pirate-polar-zero | Headless |
| Pirate43B | pirate-polar-43b | Large 800×480 |
| PirateMiniV2 | pirate-polar-mini-v2 | Portrait 240×280 |
| PiratePlusV2 | pirate-polar-plus-v2 | Landscape 280×240 |
| PiratePro | pirate-polar-pro | Portrait 240×280 |
| PirateMaxV2 | pirate-polar-max-v2 | Medium 480×320 |

Resolution, touch, LED, flash, PSRAM, and pin claims must be checked against
the corresponding `sdkconfig.*`, platform C file, schematic, and hardware
document before a descriptor can graduate from Tier 3.

## Baseline gaps

- The legacy matrix has not been rebuilt locally because ESP-IDF is absent.
- No physical board is connected or identified in this environment.
- No sanitized USB capture fixtures are present.
- Runtime heap, PSRAM, stack watermark, latency, and reconnect measurements
  have not been captured.
- Several TONEX message-body fields are reverse engineered and need explicit
  provenance before translation.

These are open migration gates, not assumed successes.
