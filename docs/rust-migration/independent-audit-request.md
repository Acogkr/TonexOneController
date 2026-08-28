# Independent Codex audit request

Use this document on a clean workstation after extracting the verification
archive. Do not assume the migration report is correct merely because its local
checks pass.

## Audit objective

Determine whether this repository is a TONEX ONE-only ESP32-S3 Rust firmware
that preserves the former TONEX ONE behavior, supports all 22 registered build
variants through reusable hardware modules, and contains no active Valeton
GP-5, TONEX Pedal, or generic modeller product path.

## Required method

1. Verify the archive checksum, then run
   `tools/verify-package-integrity.ps1` to reject missing, unexpected,
   duplicated, path-traversing, or hash-mismatched manifest entries.
2. Begin with `docs/rust-migration/completion-audit.md`, challenge every
   **Proven** verdict against direct evidence, then read `README.md` and every
   remaining file under `docs/rust-migration/`.
3. Inspect the actual Cargo workspace, feature graph, firmware entry point,
   protocol state machine, USB boundary, settings migrations, board registry,
   display/touch/I/O adapters, MIDI transports, Web control, and CI workflows.
4. Run `tools/verify-rust-migration.ps1`.
5. Install the pinned Espressif environment and run
   `tools/verify-rust-migration.ps1 -TargetMatrix`.
6. Confirm that exactly one board is required, invalid board selections fail,
   all 22 supported core/BLE-only/Wi-Fi+Serial builds succeed, the 20 supported
   8/16MiB maximum-feature builds succeed, and 4MiB Wi-Fi+BLE is rejected.
7. Compare every broad claim in `verification.md` with direct source or command
   evidence. Treat compilation as compilation evidence only.
8. Search independently for panic-prone parsers, unbounded callback work,
   hidden allocation, leaked secrets, unused dependencies, undocumented unsafe
   code, mixed-device branches, stubs, and stale release paths.
9. Report every discrepancy with file and line references, severity, command
   output, and a proposed acceptance test.
10. Analyze the complete endurance serial capture with
    `tools/analyze-diagnostics.ps1`; retain the input log and analyzer output
    as audit artifacts.

## Physical evidence

Do not approve functional equivalence without hardware evidence for:

- TONEX ONE USB detection, handshake, 20-preset synchronization, parameter
  reads/writes, save, disconnect, reconnect, and error recovery;
- representative headless, SPI, QSPI, I80, and RGB boards;
- touch, switches, LEDs, Serial MIDI, BLE-MIDI, NVS power-cycle behavior;
- Wi-Fi AP and Station modes, WebSocket control, browser layout, reconnect, and
  Wi-Fi/BLE coexistence;
- measured latency, minimum heap, largest free block, task stack high-water
marks, repeated reconnect memory stability, and event-loss counters.

Use `hardware-validation.md` as the minimum execution record. A checked box
without a log, measurement, photograph/video reference, or reproducible
observation is not evidence.

If these artifacts are absent, return `NOT APPROVED — PHYSICAL EVIDENCE
MISSING`, even when all static gates pass.

## Expected response

Return:

- verdict: approved, conditionally approved, or not approved;
- requirement-by-requirement evidence table;
- commands executed and exact results;
- defects and risk ranking;
- unverified claims;
- minimum remaining work before release.

Do not edit the project during the first audit pass. A green verifier is input
to the review, not a substitute for independent inspection.
