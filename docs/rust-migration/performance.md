# Performance and resource ledger

Measurements in this file are evidence records, not estimates. Host timings and
ESP32-S3 link sections are kept separate from measurements that require a
physical board.

## Current product image

Measured on 2026-08-10 with `wifi-web,board-jc3248w535`, the conservative
4,128,768-byte 4MiB-compatible factory partition, release optimization, and
`cargo-espflash 4.5.0 save-image`:

| Representation | App image | Partition use | Free app space |
| --- | ---: | ---: | ---: |
| Raw 50 × 240×80 RGB565 board skins | 3,912,624 B | 94.76% | 216,144 B |
| Lossless row-RLE board skins plus Station recovery | 3,174,192 B | 76.88% | 954,576 B |
| Net improvement | -738,432 B | -17.88 percentage points | +738,432 B |

The RLE blob is 1,164,750 bytes and its compiled row-offset table is 16,004
bytes, replacing 1,920,000 raw bytes and saving 739,246 asset bytes. The final
app-image reduction is 738,432 bytes because the later Station recovery path
adds a small amount of code. Decoding uses one caller-owned 240-pixel
RGB565 row (480 bytes) and no heap allocation. All 4,000 source rows decode to
their exact width, and SHA-256 comparison of 67 complete UI BMP previews found
zero pixel differences before and after compression. The Wi-Fi Web release
matrix subsequently linked on all 22 registered boards.

The real JC3248W535 hardware-validation command selects 16MiB flash and
`partitions.large.csv`, whose factory app partition is 5,111,808 bytes. A
second `save-image --flash-size 16mb --partition-table partitions.large.csv`
measurement reports the same 3,174,192-byte app image at **62.10%**, leaving
1,937,616 bytes. The 76.88% figure is retained as the stricter cross-board
comparison, not as the JC board's actual flash layout.

## ESP32-S3 release link sections

Measured on 2026-07-30 with the `devkitc-n8r2` feature, ESP-IDF 5.5.1, size
optimization, fat LTO, one codegen unit, and stripped symbols:

| Build | text | data | bss | ELF file |
| --- | ---: | ---: | ---: | ---: |
| Core firmware | 423,259 B | 112,944 B | 975,737 B | 596,648 B |
| Core + Wi-Fi/Web | 937,597 B | 201,188 B | 1,999,589 B | 1,219,416 B |
| Wi-Fi/Web increase | 514,338 B | 88,244 B | 1,023,852 B | 622,768 B |

Commands:

```text
.\tools\build-rust-esp.ps1 -Board devkitc-n8r2 -TargetDirectory C:\tx8
.\tools\build-rust-esp.ps1 -Board devkitc-n8r2 -TargetDirectory C:\tx7 -WifiWeb
xtensa-esp32s3-elf-size <ELF>
```

These are linker section totals, not measured free heap or flash image sizes.
The Wi-Fi feature remains optional so headless builds do not link the radio,
HTTP server, WebSocket, or embedded page.

### Serial MIDI incremental link cost

Measured on 2026-07-30 from the same source state and board configuration.
Each variant used its own clean target directory so stale artifacts could not
affect the comparison:

| Build | text | data | bss | ELF file |
| --- | ---: | ---: | ---: | ---: |
| Wi-Fi/Web, no Serial MIDI | 1,026,709 B | 227,740 B | 2,204,525 B | 1,342,296 B |
| Wi-Fi/Web + Serial MIDI | 1,048,221 B | 231,684 B | 2,338,773 B | 1,370,968 B |
| Serial MIDI increase | 21,512 B | 3,944 B | 134,248 B | 28,672 B |

Commands:

```text
.\tools\build-rust-esp.ps1 -Board devkitc-n8r2 -TargetDirectory C:\tx-web-size -WifiWeb
.\tools\build-rust-esp.ps1 -Board devkitc-n8r2 -TargetDirectory C:\tx-midi-size -WifiWeb -SerialMidi
xtensa-esp32s3-elf-size <ELF>
```

The 134,248-byte `bss` difference is the complete linker-section delta after
enabling the ESP-IDF UART/FreeRTOS path. It is not a claim that the MIDI task
allocates that amount from the runtime heap: the adapter itself uses a
128-byte receive buffer and a bounded 16-action channel, while linked ESP-IDF
driver and task support also contributes static storage.

### BLE MIDI incremental link cost

Measured on 2026-07-30 using `devkitc-n8r2`, the same source state, and
separate target directories:

| Build | text | data | bss | ELF file |
| --- | ---: | ---: | ---: | ---: |
| Wi-Fi/Web + Serial MIDI | 1,048,545 B | 231,972 B | 2,338,773 B | 1,370,968 B |
| Wi-Fi/Web + Serial MIDI + BLE MIDI | 1,376,741 B | 311,180 B | 3,075,493 B | 1,792,856 B |
| BLE MIDI increase | 328,196 B | 79,208 B | 736,720 B | 421,888 B |

BLE MIDI is optional because the ESP-IDF Bluedroid controller and host have a
material link cost. Its configuration is restricted to BLE 4.2, one ACL
connection, and one GATT client. Dynamic BLE
environment allocation and PSRAM-first allocation are enabled, matching the
legacy firmware's memory strategy. The `bss` figure is a linker-section delta,
not a measured runtime heap requirement.

## Implemented latency and allocation controls

- USB receive callbacks copy into an 8,192-byte bounded SPSC ring and return
  without parsing, logging, NVS access, or UI work.
- WebSocket commands are limited to 128 bytes and enter an eight-command
  non-blocking queue.
- The server supplies the initial WebSocket snapshot, so the client does not
  duplicate that request at connection time. Snapshot polling is 200 ms while
  visible, bounding normal physical-input-to-browser observation delay without
  a server-side broadcast registry, and backs off to 2 seconds while hidden.
  At the current 590-byte default snapshot this is about 2.95 KB/s of payload per open,
  visible browser before WebSocket framing.
- Web state and parameter JSON share one caller-owned, reusable 20 KiB response
  buffer; snapshot encoding itself has a separately tested 4 KiB upper bound.
- Serial and BLE MIDI queues drain up to eight actions per 1 ms controller
  tick. This bounds per-tick work while avoiding one-message-per-tick
  backpressure during serial line-rate or multi-event BLE packets.
- The application shares Web snapshots only when their value changes; the
  1 ms controller loop does not lock the Web snapshot on unchanged state.
- Multi-pixel LED updates are staged and submitted with one RMT refresh.
- RGB 800×480 boards render directly into the ESP-IDF-owned PSRAM scanout
  framebuffer. This removes the second 768,000-byte RGB565 buffer and the
  768,000-byte full-frame copy previously performed by every `present()`.
- SPI, QSPI, and I80 panels retain a 32-row internal DMA staging buffer rather
  than requiring the full framebuffer to reside in internal DMA-capable RAM.
- A narrow `esp-idf-diagnostics` boundary reads allocator and FreeRTOS
  statistics without exposing raw pointers to the firmware. Every 60 seconds,
  one parseable `DIAG` line reports uptime, current/minimum free heap, largest
  8-bit-capable free block, main-task stack high-water mark, cumulative
  Serial/BLE MIDI and touch queue drops, commands rejected while disconnected,
  unexpected command and MIDI-mapping errors, switch-read errors, and LED
  errors. The display, touch, and Serial MIDI workers emit separate `DIAG_TASK`
  stack high-water records once per minute.
- Display `DIAG_TASK` records also separate CPU rendering from panel
  presentation, reporting the latest and lifetime maximum microseconds plus
  the completed frame count. The log analyzer checks monotonic task uptime,
  frame count, and maximum timings and summarizes display, touch, and Serial
  MIDI stack watermarks.
- Serial and BLE MIDI queues count saturation instead of silently discarding
  evidence. Their producer paths remain non-blocking and allocation-free.
- Touch input uses a one-action bounded queue, counts saturation, exits when
  its consumer disappears, and polls at 5 ms intervals instead of spinning on
  the I2C bus.
- The display worker blocks on its bounded snapshot receiver and begins the
  next render immediately after transfer completion. A redundant 5 ms
  post-frame sleep was removed; the receiver itself provides idle blocking.
- Board skin rows are decoded from lossless RLE only when their source row
  changes during scaling. This trades 480 bytes of display-task stack for
  739,246 bytes of asset storage without heap allocation.

## Host UI microbenchmarks

Measured again on 2026-08-10 after row-RLE and Tuner UI changes, with release
optimization, Rust 1.91.1, and an AMD Ryzen
5 5600X. These numbers measure CPU rendering and allocation-free JSON encoding
on the workstation; they do not predict ESP32-S3 LCD bus transfer time.

| Operation | Work | Median | p95 | Maximum |
| --- | ---: | ---: | ---: | ---: |
| Tiny 128x128 render | 16,384 pixels | 38 us | 38 us | 45 us |
| Compact 320x170 render | 54,400 pixels | 112 us | 115 us | 165 us |
| Portrait 240x280 render | 67,200 pixels | 146 us | 185 us | 386 us |
| Landscape 280x240 render | 67,200 pixels | 135 us | 171 us | 197 us |
| Medium 480x320 render | 153,600 pixels | 228 us | 234 us | 363 us |
| Large 800x480 render | 384,000 pixels | 733 us | 848 us | 964 us |
| Web snapshot encoding | 590 bytes | 900 ns | 1,300 ns | 19,400 ns |

The render rows use 100 individually timed samples after warm-up. Web encoding
uses 20,000 samples after warm-up and writes into a caller-owned 4 KiB
buffer. The maximum Web value is a scheduler outlier; median and p95 represent
steady work. Reproduce and preserve a timestamped raw record with:

```powershell
.\tools\measure-ui-performance.ps1
```

## Physical measurements still required

- boot to USB-host ready;
- TONEX ONE discovery and full synchronization;
- footswitch edge to USB command;
- USB response to application snapshot;
- browser command round trip;
- LCD render/update latency;
- minimum free heap and largest free block;
- internal RAM and PSRAM use;
- task stack high-water marks;
- repeated disconnect/reconnect memory stability.
- event loss under sustained USB, UI, Serial MIDI, BLE MIDI, and Web activity.

Capture the `DIAG` lines throughout endurance testing. An acceptable run needs
stable heap/fragmentation after warm-up; zero queue drops, unexpected command
errors, MIDI-mapping errors, switch-read errors, and LED errors at the stated
input rate. `command_not_ready` is reported separately because input attempted
while TONEX ONE is disconnected can be intentionally rejected. Embedded records
cover the main controller, display, touch, and Serial MIDI tasks. HTTP,
Bluetooth, ESP-IDF UART-driver internals, and other ESP-IDF internal task
watermarks still need task-specific collection on hardware.

Analyze a captured serial log with:

```powershell
.\tools\analyze-diagnostics.ps1 -LogPath .\device.log -RequireZeroDrops -MaxHeapDriftBytes 4096
```

The drift threshold is an explicit test input, not a universal pass value. Set
it from an agreed workload and warm-up period.

No latency, heap, or stack claim is complete until these values are captured on
at least one headless board and each implemented display/bus family.
