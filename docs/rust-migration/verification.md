# Migration verification ledger

This ledger distinguishes implemented evidence from planned or unavailable
evidence. A green host test does not prove target compilation or hardware
operation.

| Requirement | Current evidence | Status |
| --- | --- | --- |
| Reproducible Rust workspace | root `Cargo.toml`, Cargo lockfile, ESP-IDF component lockfile, setup/build scripts, and host CI workflow | Implemented on host |
| Dependency security and licensing | 214 locked packages; RustSec audit; explicit seven-license allowlist; crates.io-only source policy; version-constrained workspace paths | Implemented on host |
| Legacy product isolation | Rust product code contains no GP-5, TONEX Pedal, generic modeller, or 150-preset branch; old C CI is manual-only | Implemented; retained evidence is not yet removable |
| TONEX ONE preset range | `tonex-domain` tests enforce `0..20` | Implemented |
| CRC-16/X-25 compatibility | standard check vector and C-derived algorithm | Implemented |
| Framing and escaping | round-trip and invalid-frame tests | Implemented |
| Partial/combined USB chunks | `StreamDecoder` tests | Implemented |
| Parser does not panic on malformed input | boundary tests plus 4,096-case deterministic arbitrary-byte corpus across framing, stream, message, state, name, volume, parameter-change, and preset-block parsers | Implemented on host |
| Single mutable state owner | private `AppController` state behind `TonexRuntime` snapshots | Implemented for headless sync path |
| Connection/sync lifecycle | partial/combined USB chunks, 20 names, disconnect reset, TX failure tests | Headless path implemented |
| All legacy build variants registered | 22 unique descriptors with source-derived display and I²C wiring tested | Metadata implemented |
| Board target compilation | ESP-IDF 5.5.1 release builds for all 22 core/BLE-only/Wi-Fi+Serial variants, plus the 20 supported 8/16MiB maximum-feature variants | Compile matrix implemented; 4MiB Wi-Fi+BLE is intentionally rejected |
| Physical board validation | JC3248W535 previously displayed and synchronized TONEX ONE with the stable Wi-Fi-only composition; current Phase 17 changes have not been flashed | Partial; current-firmware physical regression pending |
| TONEX message body parser | validated header, state globals/slots, preset name, 109-value preset block, parameter-change parsers | Core read path implemented |
| TONEX parameter registry | 116 source-derived definitions, generated-file drift check, typed bounds and fixed-capacity cache | Implemented |
| Parameter and preset writes | exact single-parameter, master-volume, and preserved-state slot/preset packets; transactional ready-state gates | Implemented; hardware validation pending |
| USB Host FFI | daemon ownership, exact-device CDC open, bounded RX, TX, disconnect and endpoint workaround | Implemented; physical validation pending |
| Settings schema and migration | fixed v5 encoding, explicit v0–v4 migration, validated AP/Station credentials, paired BLE peer and foot mappings, ESP NVS canonical rewrite | Implemented; physical power-cycle validation pending |
| Tap Tempo | pure 40–240 BPM reference behavior and clock anomaly tests | Implemented |
| MIDI parsing and high-level CC mapping | channel filter, PC, typed navigation/A-B bank/slot/tap/bypass/volume commands, explicit source-derived parameter map, range/select/toggle and sync-dependent scaling | Pure mapping implemented; hardware validation pending |
| Serial MIDI transport | source-derived UART1 pin matrix, 31,250 baud, partial/running-status decoder, real-time byte tolerance, fixed 128-byte RX buffer, bounded action queue | ESP32-S3 optimized build passes; physical validation pending |
| BLE-MIDI transport | Bluedroid Central scans user-selected devices, connects as a GATT client to the standard MIDI service/characteristic, subscribes to notifications, stores the peer, and uses bounded decoding/queues | Optimized ESP32-S3 build passes; physical M-VAVE validation pending |
| UI classes and display adapter | bounded snapshot/model and RGB565 rendering for all six visual classes; deterministic preset-name/model-gain tone profile; unified profile palette, rounded card hierarchy, larger preset title, generated per-class pixel previews, and a responsive-region non-overlap test; non-blocking SPI, QSPI, I80, and RGB paths | ST7789, GC9107, SH8601, ST7796, AXS15231B, and RGB target builds pass; physical validation pending |
| Touch input | source-derived wiring/transforms, shared I²C, CST816/CST328/GT911/AXS15231B polling, typed taps and contextual horizontal swipes on a separate task | Implemented; hardware validation pending |
| Footswitch inputs | 13 source-derived wiring profiles, direct GPIO and CH422G adapters, 20 ms debounce, typed short/hold action mapping | Implemented; hardware validation pending |
| Addressable LEDs | per-variant GPIO/count/order, official RMT `led_strip` adapter, connection-state indication | Implemented; hardware validation pending |
| Wi-Fi/Web control | optional WPA2 AP/Station, default-AP recovery after Station failure, captive-DNS failure isolation, embedded responsive UI, bounded WebSocket parser/serializer, typed command queue, NVS save/restart | JavaScript/accessibility/self-contained checks and target builds pass; physical Station timeout/AP recovery and ESP browser validation pending |
| Tuner core and UI | allocation-free YIN, same-note smoothing, immediate note transition, bounded dropout hold, explicit unavailable/waiting/weak/stable UI, synthetic guitar/bass tests | Implemented in software; real audio adapter and MCU measurements missing |
| TONEX ONE functional equivalence | core USB/preset/parameter/Web paths have software and prior JC3248W535 evidence; complete current-firmware integration still needs physical regression | Partial |

## Current host commands

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo machete
cargo audit
cargo deny check
.\tools\check-rust-unsafe.ps1
.\tools\check-rust-product-scope.ps1
.\tools\check-source-hygiene.ps1
.\tools\verify-rust-migration.ps1
.\tools\check-all-rust-boards.ps1 -TargetDirectory C:\tx7 -WifiWeb
.\tools\check-all-rust-boards.ps1 -TargetDirectory C:\tx2 -WifiWeb -SerialMidi
.\tools\build-rust-esp.ps1 -Board devkitc-n8r2 -TargetDirectory C:\tx3 -WifiWeb -SerialMidi -BleMidi
```

As of 2026-08-10, all commands pass with 177 workspace unit tests. `cargo audit`
reports no known vulnerabilities in the 214 locked packages.
`cargo deny check` passes advisories, bans, licenses, and sources under the
checked-in `deny.toml`: only Apache-2.0, BSD-3-Clause, ISC, MIT,
Unicode-3.0, Unlicense, and Zlib are accepted; unknown registries, Git
dependencies, and wildcard version requirements are denied. Duplicate
transitive versions are reported as warnings because ESP-IDF's build-time
toolchain currently needs multiple API generations. `cargo machete 0.9.2`
reports no unused workspace dependencies. This count will
change as coverage grows and is not itself a completion criterion.

The managed-source hygiene gate excludes generated and vendored code, rejects
TODO/FIXME/HACK/XXX debt markers and commented-out Rust statements, and passes
with 936 maintained comment lines. Of those, 115 lines begin explicit unsafe
rationales; most remaining lines are public API contracts, continuations of
the safety invariants, protocol provenance, or hardware constraints. This gate
prevents debug/dead code from being disguised as comments without deleting
information required to audit FFI and protocol behavior.

MIDI CC 38 intentionally corrects one inconsistent legacy branch. The C path
linearly scaled `0..127` into fractional values for a parameter declared as a
switch, while every equivalent synchronization switch used Off/Toggle/On.
Rust applies the common switch rule and retains strict integral switch
validation.

All 22 board features also pass an optimized
`xtensa-esp32s3-espidf` compile through
`tools/check-all-rust-boards.ps1`, both for the core firmware and with the
optional Wi-Fi/Web feature. This is compile evidence only; every
descriptor remains Tier 3 until its actual hardware is exercised.
The matrix tool accepts `-SerialMidi` and `-BleMidi`. On 2026-08-10 all 22
registered variants passed core, BLE-only, and Wi-Fi/Web + Serial MIDI builds.
The maximum Wi-Fi/Web + Serial MIDI + BLE MIDI composition passed on all 20
8/16MiB boards. The two 4MiB boards reject Wi-Fi+BLE by design and are covered
by a negative validation test.

`tools/verify-rust-migration.ps1` is the single independent-workstation host
gate. It includes strict UTF-8 and JavaScript validation for the embedded Web
UI plus a zero-loss diagnostic-analyzer fixture. With `-TargetMatrix`, it
additionally applies the requested supported feature matrix to the registered
boards; unsupported 4MiB Wi-Fi+BLE compositions are rejected explicitly.
`tools/new-verification-package.ps1` creates the minimal checksum-protected
handoff archive. `independent-audit-request.md` gives the receiving Codex a
strict review protocol and explicitly forbids treating target compilation as
physical equivalence.

`tools/verify-package-integrity.ps1` validates every manifest hash and rejects
missing, unexpected, duplicate, unsafe, or modified entries. On 2026-08-10 the
current code archive was checked against its adjacent SHA-256 file, extracted
outside the working tree, and its 995-file manifest plus the complete host
verifier passed. The archive is regenerated after the verification ledger is
updated, and the adjacent SHA-256 plus an independently extracted manifest are
checked again. The product-owned
skin sources now live under `rust/crates/tonex-ui-renderer/assets/skins`, so the
archive no longer relies on the retained legacy tree to compile the renderer.

The final 2026-08-10 archive was also extracted outside the working tree and
used as the sole source root for both the complete host verifier and an
optimized `jc3248w535` Wi-Fi Web + BLE MIDI target build. Both passed. The
resulting linked application was 3,730,264 bytes, exactly the same size as the
working-tree build. This proves archive self-containment on the current
workstation; it is not a substitute for a separate-workstation or physical
hardware audit.

An earlier checksum-protected package was also extracted under a separate temporary
source root on 2026-07-30. Its own then-current host verifier passed all tests and every
static gate without reading the working tree. A hidden portability defect was
then found: direct target-script invocation inherited the caller's working
directory. `build-rust-esp.ps1` now anchors Cargo to the script's repository
root and rejects Windows target paths longer than 12 characters before
toolchain work begins. From a newly generated extracted package, the
Waveshare Zero and the then-supported matrix passed with compiler source paths
pointing exclusively at the temporary package. That historical run proves the
archive was self-contained at that revision, not that today's intentionally
rejected 4MiB Wi-Fi+BLE composition is supported or independently approved.

After the unsafe-wrapper reduction, optimized target builds passed for seven
representative variants covering ST7789 SPI, GC9107 SPI, SH8601 SPI, ST7796
SPI, AXS15231B QSPI, raw RGB, and ST7789 I80. This proves compilation and
linking of each display path, not electrical behavior.

The representative `devkitc-n8r2` optimized build also passes with Wi-Fi/Web,
Serial MIDI, and BLE MIDI enabled together. This validates ESP-IDF radio-token
splitting, combined sdkconfig defaults, Bluedroid GATT linkage, and firmware
composition. Scanning, M-VAVE notifications, reconnects, and Wi-Fi/BLE
coexistence still require physical validation.

The Apache-2.0 GC9107 ESP-IDF component is now vendored under `rust/vendor`
rather than imported from the former C firmware tree. Supported optimized
`waveshare-zero` builds passed after the move; its 4MiB Wi-Fi+BLE combination
is intentionally rejected. The only remaining
Rust-tooling reference into `legacy/source/` is the parameter-registry provenance
check; it is not part of firmware compilation or linking.

The embedded page is covered by bounded parser, serializer, BLE/Serial MIDI
settings, self-contained-asset, secure-response-header, and JavaScript syntax
checks. It uses capped exponential WebSocket reconnect, disables unavailable
controls, tolerates malformed status messages, selects `ws`/`wss` from the page
scheme, honors reduced-motion preferences, and never returns the Wi-Fi
password. Its visible-tab snapshot interval is 200 ms for responsive control;
the hidden-tab interval remains 2 seconds. Board and Web renderers share the
accented preset-number tile, preset/title hierarchy, tone/amp panel, A/B/C
cards, slot/preset navigation, and semantic tone-profile colors. On
2026-07-30 the page was rendered in the in-app Chromium browser
at 1280x720 from a local HTTP server: the desktop layout had no horizontal
overflow, its accessibility tree exposed the heading, live connection status,
named controls, and disabled state, and the console contained no errors. The
offline state disables device controls and settings inputs and marks the
controller region busy. A host WebSocket fixture also verified the server-push
initial state, ready-state rendering, active A/B/C slot, Next and Tap command
frames, periodic snapshot requests, and a clean console. The client no longer
duplicates the server's initial snapshot and uses a 2-second poll while its tab
is hidden. This is host-browser evidence only. Mobile device rendering, radio
behavior, and the ESP32-S3 HTTP endpoint still require physical validation.

The firmware binary now runs the headless USB connection/retry loop and
drives the Rust handshake, 20-preset name synchronization, 109-value current
preset import, master-volume synchronization, and guarded parameter writes.
Serial MIDI, BLE MIDI, footswitch, and touch command failures are no longer
discarded. The periodic diagnostic record separates expected not-ready
rejections from unexpected runtime errors, MIDI mapping errors, switch-read
errors, and LED update errors. A failed LED transfer is reported and the failed
device is removed instead of being falsely recorded as updated. The analyzer
fixture covers the expanded schema. Current Waveshare Zero builds use only
feature combinations allowed by the 4MiB memory policy.
This does not yet prove physical TONEX ONE communication, settings
persistence, footswitches, LEDs, or displays. The ST7789 SPI/I80, GC9107 SPI,
SH8601 SPI, ST7796 SPI, AXS15231B QSPI, and raw RGB adapters use the separate
bounded display task and compile in optimized ESP32-S3 builds. This is not
electrical or visual validation.

The requirement-by-requirement release verdict is maintained in
`completion-audit.md`. It is authoritative about which implemented features
remain Partial because physical or independent evidence is unavailable.
