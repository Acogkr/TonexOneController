# Rust ESP32-S3 development environment

The host-only workspace uses stable Rust. ESP32-S3 firmware requires the
Espressif Xtensa Rust toolchain and ESP-IDF build dependencies.

## Windows setup

From the repository root:

```powershell
.\tools\setup-rust-esp.ps1
. "$env:USERPROFILE\export-esp.ps1"
```

Pass `-InstallFlashTool` when local flashing and monitoring are also needed.

The setup script pins:

- espup 0.17.1
- ldproxy 0.3.5
- cargo-espflash 4.5.0 when requested

The versions were selected from crates.io on 2026-07-30. Update them
deliberately and validate every board build rather than silently following
latest releases.

## Host validation

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo machete
cargo audit
cargo deny check
```

The firmware crate defaults to `host-sim` so host workspace commands do not
attempt to link ESP-IDF.

Install Node.js 20 or newer for the embedded JavaScript syntax gate. Install
`cargo-audit 0.22.2`, `cargo-deny 0.20.2`, and
`cargo-machete 0.9.2` when preparing a verification workstation. The
repository's `deny.toml` is the release policy: dependency licenses and
sources must be reviewed rather than accepted implicitly.

For a one-command host audit, run:

```powershell
.\tools\verify-rust-migration.ps1
```

This command also validates the embedded page as strict UTF-8, parses its
JavaScript with Node.js, checks required accessibility/offline markers, and
runs the diagnostic-log analyzer against its checked-in zero-loss fixture.

After installing the ESP32-S3 toolchain, add `-TargetMatrix` to compile every
registered board with all optional transports.

Create a minimal handoff archive for a clean verification workstation with:

```powershell
.\tools\new-verification-package.ps1 -RunHostVerification
```

The archive contains a per-file SHA-256 manifest and a separate archive
checksum. Historical releases, build caches, UI editor projects, and the mixed
device C firmware are excluded.

## ESP32-S3 target build

Disable `host-sim` and select exactly one board:

```powershell
.\tools\build-rust-esp.ps1 -Board devkitc-n8r2
```

The build script rejects zero board features, multiple board features, and a
board combined with `host-sim`.

Optional transports are selected independently:

```powershell
.\tools\build-rust-esp.ps1 -Board devkitc-n8r2 -WifiWeb -SerialMidi -BleMidi
```

BLE MIDI adds the BLE-only Bluedroid sdkconfig. When Wi-Fi and BLE are both
selected, the HAL-supported modem split supplies each stack its own typed
radio token.

Build the complete legacy variant matrix with:

```powershell
.\tools\check-all-rust-boards.ps1
```

On Windows, ESP-IDF rejects long build output paths. The wrapper therefore
uses `C:\tx1` by default while keeping the repository in its original
location. Override `-TargetDirectory` if that path is unavailable, but keep
the replacement path short.

## Flash and capture a physical run

Connect the computer to the board's programming/debug USB port and connect
TONEX ONE to the board's USB Host port. First preview the selected board,
features, port, and flash command:

```powershell
.\tools\start-hardware-validation.ps1 `
    -Board waveshare-zero -Port COM5 -WifiWeb -SerialMidi -BleMidi -PlanOnly
```

Remove `-PlanOnly` only after checking the values. The run requires pinned
`cargo-espflash` 4.5.0, refuses to overwrite an existing log, and records the
serial monitor output in `artifacts`. See `hardware-validation.md` for the
acceptance checklist and diagnostic analysis.

The binary contains the USB Host/CDC lifecycle, exact TONEX ONE filter,
handshake, synchronization, application controller, board peripherals,
optional transports, and reconnect loop. Target builds are compile evidence;
physical TONEX ONE, board, Wi-Fi, Serial MIDI, and BLE MIDI validation remains
separate.
