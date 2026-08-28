# TONEX ONE legacy feature gap audit — 2026-08-09

This audit compares the retained C controller with the Rust product boundary.
It distinguishes useful TONEX ONE behavior from multi-modeller legacy code and
from ideas that never shipped in the C firmware.

## Equivalent or improved in Rust

- TONEX ONE USB discovery, reconnect, handshake, 20 preset names, A/B/C state,
  preset colours, global state, and the complete 109-value preset block.
- Typed reads and writes for the source-derived TONEX ONE parameter registry,
  including preset volume, AMP, CAB/VIR, all FX groups, BPM, bypass, input trim,
  tuning reference, and master volume.
- Bidirectional board/Web snapshots and grouped Web parameter editors. The CAB
  editor now includes the global Cab Sim bypass rather than exposing only the
  cabinet model parameters.
- Program Change and every CC defined by the retained TONEX ONE MIDI mapping,
  with wired MIDI and BLE-MIDI transports kept as optional firmware features.
- BLE-MIDI central/client mode scans for the legacy Chocolate names
  (`FootCtrl` and `FootCtrlPlus`), subscribes to the standard BLE-MIDI
  characteristic, reports searching/connected state, and reconnects after a
  link loss.
- Four logical foot buttons each have separate short-press and hold actions.
  Web settings provide global defaults plus per-preset overrides, with typed
  persisted actions rather than exposing raw CC numbers to the player.
- Board metadata, display/touch adapters, debounce, LED indication, Wi-Fi
  AP/Station mode, bounded WebSocket control, settings persistence, and schema
  migration.
- TONEX ONE parameter edits do not need a separate Save command: the retained C
  implementation explicitly records that TONEX ONE auto-saves them.

## Useful legacy behavior not yet migrated

1. Preset-twice bypass behavior, custom preset ordering/loop policy, and
   arbitrary Program Change remapping. Preset-twice bypass is intentionally
   excluded from the requested product behavior.
2. Optional display rotation, higher touch-sensitivity selection, and the
   legacy BPM-flash suppression setting.
3. Wi-Fi timed-AP shutdown, selectable transmit power, and configurable mDNS
   hostname. Rust currently provides permanent AP/Station modes, captive DNS at
   `tonexone.test`, and `192.168.4.1` as the recovery address.

## Bluetooth input contract

- Program Change 0..3 is accepted as a one-shot short press for buttons 1..4.
- CC 1..4 with values below/above 64 is accepted as release/press edges. This
  momentary CC form is required to distinguish a short press from a hold.
- The actual Chocolate transmission mode still needs one physical packet
  capture. If it sends different controllers or device names, only this input
  adapter should change; stored actions and the Web UI remain stable.
- The JC3248W535 and other 8/16 MB boards can carry Wi-Fi Web and Bluetooth in
  one image. Current 4 MB boards cannot fit both stacks simultaneously and must
  use either Wi-Fi Web or Bluetooth in a given firmware image.

These are feature gaps, not evidence that the current core path is broken.
They should be added only through shared typed settings and board capabilities,
not by copying the legacy global-state configuration structure.

## Not a migration gap

- Valeton GP-5, TONEX Pedal, generic modeller branches, 150-preset behavior,
  and their device-specific UI are intentionally outside the Only TONEX ONE
  product.
- Expression-pedal input, MIDI output/state mirroring, and multiple simultaneous
  Bluetooth clients appear in the legacy task list as future ideas, not as
  completed C behavior.
- A real tuner note/cents stream is absent from both the retained C controller
  and the currently proven TONEX ONE protocol. The Rust tuner renderer must stay
  unavailable until hardware capture proves telemetry or a separate audio
  pitch-detection input is designed.

## Remaining physical checks

- Physical TONEX ONE preset-volume changes update board and Web immediately and
  remain correct after the authoritative preset-details refresh.
- CAB On/Off from Web changes the pedal, board FX state, and refreshed Web value
  in both directions.
- Selecting A/B/C shows exactly one current-slot assignment selector and never
  resurrects the removed three-slot duplicate grid.
- Disconnect, failed enumeration, and recoverable protocol errors never place
  raw diagnostic text in the stage title.
- Rapid preset/slot/FX changes remain atomic and do not reintroduce stale FX or
  visible synchronization-count flicker.
- Chocolate discovery, reconnect, buttons 1..4, and hold/release packets match
  the adapter contract above on physical hardware.
