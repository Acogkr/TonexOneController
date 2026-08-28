# Failure-mode and recovery audit — 2026-08-10

This audit separates behavior proven by host tests and browser automation from
behavior that still requires the physical controller, phone, TONEX ONE, or BLE
pedal.

## Verified in software

| Scenario | Behavior | Evidence |
| --- | --- | --- |
| Short TONEX disconnect | The last ready stage remains visible only during the bounded reconnect grace; controls remain locked | firmware unit tests |
| Sustained TONEX disconnect | The display returns to the disconnected stage and TONEX-dependent pages close | firmware unit tests |
| USB receive-ring overflow | Damaged bytes are discarded; a ready session resets framing, while boot sync restarts | runtime implementation and protocol tests |
| Malformed/asynchronous protocol input | The message is logged and skipped without treating it as a physical disconnect | runtime tests |
| Web command burst | The queue is bounded, excess input receives a busy response, and equivalent rapid writes are coalesced | parser and firmware batching tests |
| Web reconnect | Requests are bounded, reconnect uses exponential backoff, and page hide/show stops and resumes the socket | embedded Web checks |
| Web snapshot contention | UI, parameter, and foot-controller snapshots each retain an independent retry marker | 2026-08-10 implementation audit |
| Captive DNS failure | Web control remains available at `http://192.168.4.1/` instead of failing with DNS | 2026-08-10 implementation audit |
| Station network unavailable | Startup falls back to the known default WPA2 access point and publishes that effective AP state, so QR/Web settings remain usable | ESP target build and implementation audit; physical timeout/fallback pending |
| Corrupt or old settings | Invalid data falls back to validated defaults; schemas 0–4 migrate explicitly to schema 5 | settings tests |
| Mobile internal Back | Settings and Bluetooth pages unwind to the previous internal page; a directly restored hash route first returns to the main page | automated browser run |
| Unchanged Web state | Slot and Bluetooth DOM trees are rebuilt only when their source data changes | 2026-08-10 Web optimization |

## Security and resource boundaries

- Web commands and text are length- and range-validated before they reach the
  application runtime.
- Preset names are rendered as text, not interpreted as HTML.
- Wi-Fi passwords are never included in Web snapshots.
- HTTP responses disable caching and framing and declare content types.
- The Wi-Fi WPA2 credential is the product's control boundary. There is no
  additional Web login by design, so anyone joined to the controller AP can
  control the connected TONEX ONE.
- Command channels, display updates, USB receive storage, and response buffers
  are bounded. The large Web response buffer is allocated once and reused.
- Host snapshot serialization measured 590 bytes, 0.9 microsecond median and
  1.1 microsecond p95 over 20,000 iterations. This is a host reference, not an
  ESP32-S3 latency measurement.

## Physical validation still required

1. Power-cycle persistence after changing Wi-Fi, Bluetooth peer, foot mappings,
   brightness, and amp skin.
2. AP reconnect and captive-portal behavior on Samsung Internet and Chrome,
   including private-DNS configurations.
3. Repeated TONEX unplug/replug, rapid preset/slot changes, and USB power dip
   behavior on JC3248W535.
4. Touch response, display edge stability, and WebSocket control with the
   stable `wifi-web,board-jc3248w535` firmware.
5. M-VAVE Chocolate discovery, pairing, reconnect, short/hold mapping, and
   long-session BLE stability. Compile success is not physical BLE proof.
6. Station-mode recovery timing and default-AP availability when the configured
   external network is unavailable. The recovery implementation and target build
   pass, but radio behavior still needs a phone and physical board.
7. TONEX ONE UAC2 descriptor capture and MCU tuner CPU/heap/latency. The live
   tuner remains unavailable until a real audio input adapter is proven.
