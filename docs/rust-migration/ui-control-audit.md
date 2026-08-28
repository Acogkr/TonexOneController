# Board and Web UI control audit

This file records the user-facing contract so future refactors do not silently
restore the legacy multi-device UI or claim hardware behavior that has not been
observed.

## Product boundary

- The runtime controls TONEX ONE only.
- Board-specific display, touch, switches, buses, and memory settings remain in
  the shared board registry; product behavior is not copied into board drivers.
- The JC3248W535 is the physical validation board, not a special UI fork.

## Board stage screen

- Preset number and preset name are the primary visual hierarchy.
- Transport and protocol diagnostics stay in the serial log and never replace
  the preset/title area with raw USB enumeration or parser errors.
- A, B, and C are visible and the selected slot is unambiguous.
- Gate, compressor, amp, cabinet, modulation, delay, and reverb show live state.
  EQ has no independent TONEX ONE enable parameter and must not display a fake
  on/off state.
- Master volume and BPM remain visible without overlapping at every registered
  display geometry.
- Settings is a compact bottom action, not a dominant performance control.

## Web control

- The main page mirrors the board palette and information hierarchy.
- A/B/C buttons show their assigned preset names. The assignment control below
  them edits only the currently selected slot; a second three-slot assignment
  grid must not be shown in the preset browser.
- The signal chain has one row only; selecting a block opens its editor.
- Gate, compressor, EQ, amp, cabinet/VIR, modulation, delay, and reverb expose
  every validated source-derived parameter. The cabinet editor includes the
  global Cab Sim bypass as a user-facing Cabinet On/Off switch.
- Successful parameter synchronization is silent. Transient counts such as
  "5 live parameters synchronized" are not presented as changing UI status.
- Reverb, modulation, delay, and VIR show only parameters applicable to the
  selected model.
- Model names, musical note divisions, units, and fine adjustment controls are
  user-facing; raw protocol labels are not the primary UI.
- Parameter controls remain visible but disabled until TONEX ONE synchronization
  reaches `Ready`. Wi-Fi and MIDI settings remain available without the pedal.
- The current non-secret Wi-Fi mode and SSID may be returned to the browser.
  The stored Wi-Fi password must never be returned.

## Wi-Fi entry

- The board QR encodes WPA credentials using the standard Wi-Fi QR format.
- Captive DNS maps the AP-local friendly address `http://tonexone.test/` to the
  controller, and operating-system probe redirects use the same address.
- Automatic portal opening depends on the phone operating system. The fixed IP
  `http://192.168.4.1/` remains the deterministic recovery fallback.

## Tuner boundary

- A responsive tuner view model and renderer exist for every display class.
- The source-derived protocol currently proves tuning reference and tuner
  mute/thru configuration only.
- Neither the legacy controller nor the captured TONEX ONE messages provide a
  proven note/cents stream. The production runtime therefore must not fabricate
  pitch or present the tuner screen as live.
- Activating the tuner requires either a verified TONEX ONE pitch message or a
  separate, explicitly designed audio pitch-detection input path.

## Physical verification still required

- Flash only when the validation board is intentionally placed in boot mode.
- Verify cold boot, stable display, touch settings entry/return, QR connection,
  captive portal opening, TONEX ONE synchronization, every parameter family,
  AMP/CAB status, and preset/slot changes on real hardware.
- Do not reset or flash merely to run host tests, render previews, or compile
  the board matrix.
