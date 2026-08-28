# Legacy boundary and removal gate

The current product entry point is the Rust workspace. The former C firmware is
retained as evidence, not as a supported product.

## Active Rust product paths

- `Cargo.toml`, `Cargo.lock`, `rust/`, `sdkconfig*.defaults`, and `tools/`
- `.github/workflows/rust-host.yaml`
- the 22 board features selected by `tools/build-rust-esp.ps1`

The Rust application, protocol, MIDI, Web, settings, and firmware crates contain
no Valeton GP-5, TONEX Pedal, generic modeller selection, or 150-preset product
branch.

## Retained evidence

- `legacy/source/main/usb_tonex_common.*` and `usb_tonex_one.*`: protocol and lifecycle
  provenance
- `legacy/source/main/tonex_params.*`: source of the generated 116-parameter registry
- `legacy/source/esp_idf_project_configuration.json` and platform headers: board wiring
  and memory provenance
- `legacy/source/`, `legacy/Releases/`, `legacy/build_distrib/`,
  `legacy/build_all.bat`, and the legacy
  development/upload documents: former mixed-device implementation and binaries

The old C workflow is manual-only. It is not a push, pull-request, or release
gate.

## Remaining technical references into `legacy/source/`

1. `tools/generate-tonex-parameters.py --check` compares the committed Rust
   registry with `legacy/source/main/tonex_params.*`. This is a verification-only
   dependency.
2. Board documentation and tests retain legacy configuration keys as
   provenance labels. They do not enable another product.

## Deletion gate

Do not delete the retained C tree or historical binaries until all of the
following are true:

- a physical TONEX ONE passes connect, handshake, complete 20-preset sync,
  parameter read/write, save, disconnect, and reconnect tests;
- representative headless, SPI, QSPI, I80, and RGB boards pass physical I/O and
  display tests;
- Wi-Fi AP/Station, Web control, Serial MIDI, BLE-MIDI, NVS power-cycle, and
  Wi-Fi/BLE coexistence pass on hardware;
- protocol and parameter provenance needed by deterministic tests is copied into
  a small immutable evidence package;
- the vendored GC9107 component passes the physical Waveshare Zero display test;
- an independently prepared workstation reproduces the host gate and complete
  22-board target matrix.

Until then, removal would reduce the evidence available to diagnose a parity
failure. The final release package must exclude these retained directories even
while the repository still contains them.

The complete release-condition verdict is tracked in `completion-audit.md`.
