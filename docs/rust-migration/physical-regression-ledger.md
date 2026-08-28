# Physical regression ledger

This ledger is the authority for hardware claims. A successful build or flash
does not make a firmware image physically verified. Do not name or report an
image as `final` until every required JC3248W535 row below passes on the same
binary.

## Immutable result history

| Date | Image / composition | Direct evidence | Verdict |
| --- | --- | --- | --- |
| 2026-08-09 | Saved JC3248W535 Wi-Fi Web image before the long audit | User observed display, touch, TONEX connection, presets, and preset-volume synchronization | Historical baseline only; later source cannot inherit this verdict |
| 2026-08-10 | `board-jc3248w535,wifi-web,ble-midi`, 3,610,016-byte app | Flash completed, but user observed that TONEX ONE did not connect | Failed; never use as a release baseline |
| 2026-08-10 | `jc3248w535-wifi-ble-dma-reserve-candidate.bin`, SHA-256 `3d7a9eb0151469a404e60ed7e5224d0fa169468b895239c691b72c555cc0ec74` | Initial DMA reserve existed, but timed opens could temporarily release it before a TONEX descriptor appeared | Superseded; do not flash |
| 2026-08-10 | `jc3248w535-wifi-ble-dma-reserve-candidate-v2.bin`, SHA-256 `094107d5ce7dac4dbf6a27a1e0c8dfaabaad8e56a4ed01c24c85c8544e36355f` | Reserve remains held until exact VID/PID, but a later audit found the C controller's 100ms/250ms CDC settle intervals were still absent | Superseded; do not flash |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-stable-candidate-v3.bin`, SHA-256 `56e7c9c8f871c91b8255869cb99c013daf725ddbfbb59bd8c801801c6b392e3e` | DMA reserve, exact-device gate, and reference CDC settle timing passed automation, but a later reconnect audit found that an ignored reserve-restore failure could permit an unreserved retry | Superseded; do not flash |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-stable-candidate-v4.bin`, SHA-256 `489e2b3300306857d7e770d8537f5790cc7ecf7009e61a3fa4efe7fd41b80838` | Exact 16MiB image flashed successfully at 18:20 KST, but the user observed that TONEX ONE did not connect | Failed; never use as a release baseline |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-stable-candidate-v5.bin`, SHA-256 `03852c36fa4ada6e222f2a80bac3aaca252b9161d0dfbe93000d28217edcf8ac` | Restored the legacy five-second cold-start settle and CDC GET/SET/line-state sequence, but its source formatting gate had not passed when the image was named | Superseded; do not flash by this filename |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-stable-candidate-v6.bin`, SHA-256 `03852c36fa4ada6e222f2a80bac3aaca252b9161d0dfbe93000d28217edcf8ac` | Exact 16MiB image flashed successfully at 18:30 KST. After more than the required ten-second cold-start interval, the user observed no TONEX connection and no preset. | Failed; never use as a release baseline |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v1.bin`, SHA-256 `4beb9db1d1f2b675cc6a780b6b89cf2974a55908fb98c8e296a1c2aa70991af6` | Full Wi-Fi+BLE composition with temporary on-display USB phase/error reporting; exact flash passed at 18:46 KST. User observed `USB OPEN ERR 257`, proving enumeration and identity succeeded but CDC open returned `ESP_ERR_NO_MEM`. | Diagnostic complete; superseded |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v2.bin`, SHA-256 `da2f0ab3296e5c4e3768d4835e683d71d8983e1500201423fb391d7c191e147a` | Exact image flashed successfully at 19:00 KST, but the user again observed `USB OPEN ERR 257`; increasing the arena alone did not remove the allocation race | Diagnostic complete; superseded |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v3.bin`, SHA-256 `353ae72bd41a5dba90223fdbeee3272ff1e6b4b7e198265de372e9ea756e9993` | Exact image flashed successfully at 19:09 KST. The user obtained one real TONEX connection after interacting with the pedal, proving the 4 KiB transfer can open and synchronize, but a later attempt remained unresponsive. | Partial improvement; failed reconnect reliability |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v4.bin`, SHA-256 `e64d9f1a921d1721f05d09b7e120d1adaa835d77e2551c6511686b06761bc94a` | Added automatic root-port recovery, but the target compiler found a diagnostic assignment that could never be presented | Superseded before flash |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v5.bin`, SHA-256 `33493cc1487d639d981ec4ad8dd25c599fc387c77db2292065194bc9ca2200a4` | Exact image flashed successfully at 19:20 KST. `USB HOST STARTING` repeatedly reappeared and the right edge corrupted while TONEX power cycled, proving periodic root-port recovery reset/browned out the board. | Failed; automatic power cycling forbidden |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v6.bin`, SHA-256 `e166e9903ef6baddb21d954b90ca963e9fada3895bf9d6f62faae2d355b85ac8` | Exact image flashed successfully at 19:35 KST. With no pedal-button intervention, the user observed a successful TONEX connection and preset synchronization. A subsequent unplug/replug did not reconnect. | Cold attach passed, reconnect failed; superseded |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-stable-candidate-v7.bin`, SHA-256 `a77370e171692095c0c5dcce09b21933648a9d7f725c8728a4701438d7b1dfc6` | Product build matched the diagnostic behavior but emitted one feature-off unused-variable warning | Superseded before flash |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-stable-candidate-v8.bin`, SHA-256 `a77370e171692095c0c5dcce09b21933648a9d7f725c8728a4701438d7b1dfc6` | Same deterministic product binary after warning cleanup; diagnostic title messages disabled. Warning-free 3,611,216/5,111,808-byte target build, 179 workspace tests, full verifier, unsafe audit, and locked checksum pass. It still contains the reconnect path that failed on diagnostic v6. | Superseded before flash |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v7.bin`, SHA-256 `7e86f8397327b81affd521ee5638a401a3a2f68da30bb4dd7efb194bc747c225` | Reconnect-only correction: the boot-time 12 KiB guard remains strict, post-radio guard restoration is best effort, and a prior successful session enables a bounded 250ms direct reopen probe if a rapid attach callback is missed. Warning-free target build, 3,611,280/5,111,808 bytes. Exact 16 MiB image flashed successfully to the ESP32-S3 at 19:51 KST. Initial connection succeeded but was perceived as slow; same-session unplug/replug did not reconnect. | Failed reconnect; superseded |
| 2026-08-10 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v8.bin`, SHA-256 `67a1d14cf205fa7a70deaade84a601d295d453bce3575b549a487520722c5c09` | Added CDC client uninstall/reinstall after disconnect. Exact 16 MiB image flashed successfully at 20:06 KST, but the user then observed no TONEX connection at all. | Failed; never use as a release baseline |
| 2026-08-11 | Rollback to `jc3248w535-wifi-ble-usb-connection-diagnostic-v7.bin`, SHA-256 `7e86f8397327b81affd521ee5638a401a3a2f68da30bb4dd7efb194bc747c225` | Exact previously tested 16 MiB image was restored to COM4 at 13:33 KST. With TONEX disconnected the board/display remained stable, but attaching TONEX caused changing USB status and right-edge corruption. | USB attach unstable; superseded |
| 2026-08-11 | `jc3248w535-wifi-ble-usb-connection-diagnostic-v9.bin`, SHA-256 `9df288fbf3fa5d29454c7e3b3f81fc5af0a68c87b2248d775d9df8dffa9e7b6e` | Reduced the CDC IN allocation and diagnostic redraw frequency. Exact 16 MiB image flashed successfully at 15:26 KST, but the user observed that TONEX still did not connect. | Failed; temporary tuning reverted |
| 2026-08-11 | `jc3248w535-wifi-ble-usb-ownership-diagnostic-v10.bin`, SHA-256 `83c84ddefbd43dfabdd53b330ec33ba5db92626568f2b6a541e5e1298778ce43` | First dedicated-enumerator build compiled, but review found a remaining history-based blind reconnect path that could bypass raw-device ownership. | Superseded before flash |
| 2026-08-11 | `jc3248w535-wifi-ble-usb-ownership-diagnostic-v11.bin`, SHA-256 `8a6e03c57d48ab8c46095d2b1f257eae9f42227799b65e7e8d1763f20a5211bb` | Dedicated USB Host client owns the raw device, validates exact VID/PID, patches descriptors through the retained handle, installs CDC only afterward, and releases CDC before the raw device on disconnect. Warning-free target build, 3,613,072/5,111,808 bytes, full verifier passed, and exact 16 MiB image flashed at 15:49 KST. The user still observed no connection, changing status titles, and right-edge corruption. | Failed; Rust flashing paused pending legacy A/B control |
| 2026-08-11 | Official legacy C `TonexController_V2.0.4.2_beta5_JC3248W` | Full flash erased, then checksum-locked bootloader, partition table, OTA state, app, and skins were written at manifest addresses through COM4 at 16:11 KST. The user observed immediate stable TONEX connection on the same board, cable, and pedal. | PASS control; hardware, cable, power, and pedal cleared |
| 2026-08-11 | `jc3248w535-wifi-ble-legacy-config-diagnostic-v12.bin`, SHA-256 `b6d7c879ae3a96fdb8d0efd6ddbeb64eb8932600352149de0feac2135c0b138f` | Keeps the v11 USB ownership code but aligns proven C build settings: 1000 Hz timing and 500 ms startup guard, Wi-Fi/LwIP PSRAM preference, PSRAM instruction/rodata fetch, Wi-Fi IRAM placement disabled, CPU0 LwIP affinity, and bounded BA windows. App 3,589,600/5,111,808 bytes; full verifier passed and exact 16 MiB image flashed at 16:28 KST. The user then observed repeated connect/disconnect transitions with right-edge corruption. | Failed; transport layer still unclassified |
| 2026-08-11 | `jc3248w535-wifi-ble-usb-layer-diagnostic-v13.bin`, SHA-256 `d66fc4a89c13bfd3912b8a5fefa079252e6c7e804137a3799d3cd9b2fabb4513` | Adds latched, one-second transport counters without changing the v12 USB lifecycle: raw enumeration (`E`), CDC open (`O`), physical device-gone (`G`), handshake failure (`H`), and last open error (`X`). This separates electrical/root-port churn from protocol synchronization without rapid title redraws. App 3,591,264/5,111,808 bytes; full verifier passed; locked 16 MiB image flashed to COM4 at 17:06 KST. | Physical counter observation pending |
| 2026-08-11 | `jc3248w535-wifi-ble-legacy-root-port-diagnostic-v14.bin`, SHA-256 `ae0484371deb01783abd81869e5f98fb640e7925bb0cb168a03d83daf3a45222` | v13 remained at `E0 O0 G0 H0 X0` while the display edge corrupted, proving the failure preceded enumeration. Removed Rust-only delayed/manual root-port power and restored the legacy C behavior where `usb_host_install` powers the port normally. App 3,590,784/5,111,808 bytes; full verifier passed; locked 16 MiB image flashed to COM4 at 17:13 KST. | Physical reset and attach observation pending |
| 2026-08-11 | `jc3248w535-static-ui-usb-display-diagnostic-v15.bin`, SHA-256 `f36cbcd87e3e6caea554945af5efac65611d22e919adf8080586e4563a12dd5e` | Isolates display corruption by rendering the same stage UI exactly once, then starting USB Host after five seconds with no Web, BLE, touch task, or application-state redraws. App 1,805,552/5,111,808 bytes; locked 16 MiB image flashed to COM4 at 18:48 KST. The user observed no edge corruption. USB connection was intentionally unavailable because this image does not open CDC or run the application protocol. | PASS display driver/static frame; dynamic subsystem isolation continues |
| 2026-08-11 | `jc3248w535-full-no-touch-diagnostic-v16.bin`, SHA-256 `491a31d6f2969b99d5e279e7bae02e2259a8f7276c0901f9c5e6c19c19951663` | Restores the complete Wi-Fi, BLE, USB, Web, application, and dynamic display composition while disabling only touch initialization and polling. App 3,579,648/5,111,808 bytes; locked 16 MiB image flashed to COM4 at 18:58 KST. The user observed brief apparent connection followed by offline UI and renewed edge corruption. Counters remained `E1 O1 G0 H0 X0`, proving one stable enumeration/open with no physical gone event, handshake transmit failure, or USB open error. | Touch excluded; dynamic full-screen redraw identified |
| 2026-08-11 | `jc3248w535-coalesced-display-diagnostic-v17.bin`, SHA-256 `1ae1480d9dc73c533c86cdcf18a8a55b04c82cb117dee849f43177e25fef6cea` | Restores touch and adds a 30 ms latest-state coalescing window plus RGB565 framebuffer hashing. Identical rendered pixels no longer trigger QSPI transfers, and synchronization bursts cannot queue trains of obsolete full frames. App build and full migration verifier passed; locked 16 MiB image flashed to COM4 at 19:15 KST. The user still observed edge corruption and no completed connection. | Failed; coalescing alone is insufficient |
| 2026-08-11 | `jc3248w535-stable-startup-display-diagnostic-v18.bin`, SHA-256 `4c2dd8074673b51fa10b63e8b7d9c1c1ab2e8744163dd936475eba1e7c9fd29b` | Keeps one fixed product-facing offline frame throughout initial enumeration and protocol synchronization; the LCD changes only after the first complete Ready snapshot. Retains framebuffer de-duplication and state coalescing. Fourteen controller tests and the full migration verifier passed; locked 16 MiB image flashed to COM4 at 19:40 KST. The user later observed that TONEX still did not connect correctly and the right side still showed white noise. | Failed; startup display gating alone is insufficient |
| 2026-08-11 | `jc3248w535-legacy-boot-sentinel-display-settle-diagnostic-v19.bin`, SHA-256 `baced5090a68fb6dc4bbbb4b2a99d6d74c6d226476a42a5ffd3b413df1305f11` | Restored the retained C firmware's boot-time preset index 20 sentinel request and delayed the first Ready frame. App 3,593,296/5,111,808 bytes; full verifier and locked flash passed. The user then observed continued right-edge white noise, failed connection, and an unreadable or missing `E/O/G/H/X` summary. | Failed; full runtime and diagnostic visibility required correction |
| 2026-08-11 | `jc3248w535-legacy-runtime-baseline-diagnostic-v20.bin`, SHA-256 `2091c827e3352ff928bfcd7dce7e1ad4bfb280ac4b9f31c27e87bc55f53229bb` | Radio-free, touch-disabled physical baseline. Restores the legacy 8,192-byte CDC RX allocation and 8,960-byte DMA reserve, 240 MHz/performance/WDT configuration, USB tasks at priority 4/core 0, display at priority 2/core 1 with its 64 KiB stack in PSRAM, the legacy AXS15231B 48-column transfer cadence and one-tick wait, and a 64 KiB protocol decoder for approximately 30 KiB full-detail responses. Diagnostic builds now show live `E/O/G/H/X` state before Ready and include protocol ingest and CDC line-state errors. App 1,983,776/5,111,808 bytes; workspace tests, unsafe audit, full migration verifier, locked checksum, and exact 16 MiB flash to ESP32-S3 revision v0.2 on COM4 passed at 20:44 KST. | Flash PASS; physical display and TONEX result pending |

## Current candidate focus after v19

v19 proved that protocol-sentinel parity and startup-frame gating were not
enough. The full Rust composition still differed from the retained C runtime
in internal-memory pressure, task priority/affinity, CDC transfer size, CPU and
watchdog configuration, AXS15231B transfer cadence, and maximum protocol-frame
size. v20 is intentionally radio-free and touch-disabled so one physical run
can validate display plus USB/protocol behavior before optional services are
added back individually.

## USB ownership discrepancy addressed after v9

The retained C controller does not delegate initial enumeration to the CDC
driver. It registers a dedicated asynchronous USB Host client, opens and owns
the raw USB device, validates the exact VID/PID, patches the cached endpoint
descriptor through that owned handle, and only then installs and opens CDC.
On disconnect it closes CDC, uninstalls CDC, releases the raw device owner, and
returns to enumeration.

The earlier Rust port simplified that lifecycle. v11 restored the dedicated
enumerator and explicit raw-device ownership; the current implementation keeps
that ownership model. Later failures therefore cannot be attributed to the
old callback-only enumerator.

## USB allocation root cause addressed earlier

The retained C firmware allocated 8,960 contiguous DMA-capable bytes before
other subsystems could fragment internal memory. It released that reservation
immediately before the CDC driver allocated the TONEX ONE 8,192-byte RX and
512-byte TX buffers, then restored the reservation after disconnect. The Rust
port omitted this lifecycle. Enabling Bluetooth reduced available internal
DRAM by about 17 KiB and exposed the omission as a TONEX enumeration failure.

`esp-idf-usb-host::TonexUsbStack` now owns the same 8,960-byte boot-time
reservation with RAII and restores the reference 8,192-byte CDC RX transfer.
Installation fails early if that cold-start guarantee cannot be made, and open
releases it immediately before CDC allocates its exact buffers. Restoration
after a failed open or disconnect remains best effort because optional radios
may have fragmented internal memory. Runtime root-port power cycling remains
forbidden.

## Required same-binary JC3248W535 gate

Record PASS/FAIL and observations for every row. Any FAIL returns the candidate
to development; do not ask the user to continue testing unrelated features.

| Order | Test | Pass condition | Status |
| --- | --- | --- | --- |
| 1 | Cold boot, TONEX disconnected | Main UI appears once without black cycling or edge noise | Pending |
| 2 | Touch | Settings opens once, stays open, Back returns once | Pending |
| 3 | TONEX hot plug | Pedal powers, enumerates, synchronizes, and shows preset | Pending |
| 4 | TONEX cold-connected boot | Reboot with pedal attached reconnects without replugging | PASS on diagnostic v6: no-touch connection and preset synchronization |
| 5 | Rapid preset/slot changes | 30 rapid changes without disconnect, stale FX, or protocol title text | Pending |
| 6 | Physical preset volume | Pedal change updates board and Chrome without changing amp identity | Pending |
| 7 | Board controls | A/B/C, FX toggles, swipe, mute, and bank navigation stay synchronized | Pending |
| 8 | Disconnect/reconnect | Short loss preserves the stage briefly; sustained loss locks controls; reconnect recovers | FAIL on diagnostic v6; corrective diagnostic v7 pending |
| 9 | Chrome Web | Connect, nested Back, slot assignment, FX editor, CAB switch, and parameter writes work | Pending |
| 10 | Wi-Fi recovery | Bad Station settings fall back to the documented AP | Pending |
| 11 | Bluetooth | Scan, pair, reconnect, buttons 1-4, short press, and hold work while TONEX stays ready | Pending |
| 12 | Persistence | Power cycle restores Wi-Fi, paired peer, mappings, brightness, and manual skin | Pending |
| 13 | Endurance | 30 minutes of TONEX + Chrome + BLE use without reset, display corruption, or disconnect | Pending |

Flash this exact candidate without rebuilding:

```powershell
.\tools\flash-locked-image.ps1 `
  -Image .\artifacts\jc3248w535-legacy-runtime-baseline-diagnostic-v20.bin `
  -ExpectedSha256 2091c827e3352ff928bfcd7dce7e1ad4bfb280ac4b9f31c27e87bc55f53229bb `
  -Port COM4 `
  -FlashMegabytes 16
```

The tool rejects a size or checksum mismatch before accessing the board.

## Promotion rule

- **Candidate:** automated gates and target build pass.
- **JC verified:** every row above passes on the exact same binary and its hash
  is recorded.
- **Release candidate:** JC verified plus an independent package check.
- Other registered boards remain unverified until their own display, touch,
  power, and control rows pass; compilation never promotes a board tier.
