# Unsafe and FFI audit

All project-authored `unsafe` code is confined to the
`esp-idf-usb-host`, `esp-idf-i2c`, `esp-idf-board-io`, `esp-idf-display`,
`esp-idf-touch`, and `esp-idf-midi`
hardware-boundary crates. Domain, protocol, application, board metadata,
controls, rendering, and firmware composition inherit the workspace
`unsafe_code = "deny"` lint.

## Board GPIO

- Board descriptors are the sole source of direct footswitch GPIO numbers.
- Construction resets each available pin, selects input mode, and enables its
  pull-up before reads begin.
- The driver owns its GPIO plan for its full lifetime and reports active-low
  inputs only as a bit mask.
- IO-expander inputs are rejected explicitly; they cannot be silently treated
  as MCU GPIO.

## CH422G switch expander

- The board descriptor supplies every I2C port and pin pair plus the CH422G
  frequency and logical switch-channel mapping.
- A reference-counted bus owner permits the expander, touch, and auxiliary
  devices to share one ESP-IDF bus without duplicating raw handles. Device
  owners keep the bus alive and are removed before its final owner deletes it.
- ESP-IDF's new master driver serializes bus access; Rust keeps individual
  device transactions behind mutable borrows.
- Every one-byte transfer borrows valid storage for the full blocking call.
- Input mode is restored to output mode after each read, matching the legacy
  CH422G sequence.

## Addressable LEDs

- Board variants declare GPIO, pixel count, and RGB/GRB/GBR component order
  from their legacy sdkconfig.
- The official Espressif `led_strip` component owns RMT timing and buffering;
  project code does not implement timing-critical waveforms.
- Pixel indices are checked before FFI and every driver handle is cleared and
  deleted exactly once by its Rust owner.

## Display controllers

- Construction accepts only source-derived ST7789, GC9107, SH8601, ST7796,
  AXS15231B, or raw RGB descriptors with their matching bus type.
- The SPI or I80 bus, panel IO, and panel are represented by one non-copyable owner.
  Initialization state and nullable child handles make every partial-failure
  cleanup path delete only resources that were successfully created.
- The asynchronous transfer callback holds a stable pointer to a boxed atomic
  flag for the full panel-IO lifetime. It only clears that flag.
- Each transfer borrows an internal, DMA-capable 32-row buffer. The owner waits
  for completion before reusing or freeing it; a 500 ms timeout reports a
  stalled peripheral explicitly.
- The framebuffer may use normal or PSRAM-backed heap memory; pixels are copied
  into the internal DMA buffer before each transfer.
- RGB panels expose the single ESP-IDF-owned PSRAM framebuffer as a mutable
  slice only while the unique display owner is mutably borrowed. Rust never
  frees that pointer; deleting the panel releases it exactly once.
- QSPI uses four descriptor-owned data pins, 32-bit command phases, and the
  official AXS15231B component. RGB timing and porch values are descriptor
  data rather than controller policy.
- Panel, IO, and SPI/I80 bus deletion occurs in child-to-parent order.

## Touch GPIO and I2C

- CST816, CST328, GT911, and AXS15231B transactions use the shared owned I2C device
  abstraction; register buffers remain borrowed for each synchronous call.
- Touch reset GPIO is accepted only when the descriptor marks it as direct.
  CH422G reset channels are driven through shared I²C without borrowing an
  unrelated MCU GPIO.
- Coordinate bounds, rotation, and mirroring are applied in safe code before
  an event reaches the application.

## USB Host lifecycle

- `usb_host_install`, event handling, unblock, and uninstall are represented
  by unique Rust owners.
- One daemon task owns `UsbHost` and is the only caller of
  `usb_host_lib_handle_events`.
- Shutdown sets an atomic stop flag, unblocks the ESP-IDF event call, joins the
  task, and only then uninstalls the host library.
- The CDC driver and open device are destroyed before the host daemon because
  of the `TonexUsbStack` field order and explicit device close in `Drop`.

## CDC device handle

- Only VID `0x1963`, PID `0x00d1`, interface `0` can be opened.
- A successful handle is stored once, never copied outside the adapter, and
  replaced with null before close.
- Line coding is fixed to 115200 baud, 8 data bits, no parity, one stop bit;
  DTR and RTS are enabled to match the C reference.
- Blocking TX borrows an immutable Rust slice for the complete duration of the
  FFI call.

## Receive callback and ring

- ESP-IDF callback memory is converted to a slice only for the documented
  callback lifetime and copied immediately.
- The callback performs no parsing, allocation, logging, UI work, NVS access,
  or application-state mutation.
- The 8192-byte SPSC ring has one callback producer and one application-task
  consumer. Acquire/release publication protects slot reuse.
- Overflow drops new bytes and increments an atomic counter. The firmware
  closes and resynchronizes instead of parsing an incomplete stream.

## Serial MIDI UART

- Every UART port, RX/TX GPIO, and baud rate comes from immutable board
  metadata extracted from the legacy `main.h` matrix.
- UART1 is installed once before the receive thread starts. ESP-IDF copies the
  temporary configuration and does not retain its pointer.
- `uart_read_bytes` exclusively borrows a fixed 128-byte stack buffer for the
  complete blocking call; the returned length bounds every safe slice.
- Parsing is allocation-free. Completed actions enter a bounded 16-item
  non-blocking channel, and only the main firmware loop can apply them to
  application state.

## BLE MIDI GATT

- Project code uses the safe `esp-idf-svc` Bluedroid GAP/GATTS owners; raw
  callback and FFI unsafety remains inside that third-party adapter.
- The peripheral publishes only the standard BLE-MIDI service and
  characteristic and never enables legacy client/scanner behavior.
- GAP/GATT callbacks parse bounded packets and use a 16-action non-blocking
  channel. They do not mutate application state, access NVS, or block.
- Callback subscriptions hold weak server references, avoiding an ownership
  cycle with the ESP-IDF callback singleton.
- The Wi-Fi and Bluetooth halves come from the HAL modem's supported split,
  allowing coexistence without duplicating or stealing the radio token.

## TONEX ONE endpoint compatibility

The physical TONEX ONE descriptor advertises a 512-byte OUT endpoint packet
size even when used with the ESP32-S3 full-speed host. The legacy firmware
clamps endpoint descriptors to 64 bytes before CDC parsing.

The Rust adapter preserves this workaround only after confirming the exact
TONEX ONE VID/PID. It walks the complete cached descriptor with ESP-IDF's
bounds-aware parser, changes only `wMaxPacketSize` values above 64, and never
changes descriptor pointers or lengths. Physical-device verification remains
required.

## Audit gate

Run:

```text
.\tools\check-rust-unsafe.ps1
```

Every project-authored result must remain inside `esp-idf-usb-host`,
`esp-idf-i2c`, `esp-idf-board-io`, `esp-idf-display`,
`esp-idf-diagnostics`, `esp-idf-touch`, or `esp-idf-midi` and have a nearby
`SAFETY` comment.
Generated
`esp-idf-sys` bindings are third-party build artifacts and are not committed.

As of 2026-07-30, the automated crate-confinement and per-block documentation
gate passes for 107 authored `unsafe` blocks. The four new calls are read-only
ESP-IDF heap and FreeRTOS resource statistics confined to
`esp-idf-diagnostics`. Repeated display-panel operations
were reduced to seven documented safe wrappers instead of duplicating 25 raw
calls. The scan is a regression gate; it does not replace review of pointer
ownership or physical testing of the affected peripherals.
