# Completion-condition audit

This matrix preserves the original completion conditions and separates source
or automated evidence from evidence that can exist only after using the real
controller, TONEX ONE, and an independent workstation.

- **Proven** means direct source inspection or a reproducible automated gate
  supports the claim.
- **Partial** means the implementation exists but required physical or
  independent evidence is absent.
- No Partial row may be treated as release-complete.

## Original completion conditions

| # | Condition | Evidence | Verdict |
| --- | --- | --- | --- |
| 1 | Reproducible Rust workspace | Checked-in workspace, Cargo and ESP-IDF component lockfiles, setup/build scripts, host and target gates | Proven |
| 2 | TONEX ONE protocol core implemented and tested | Framing, stream decoder, messages, session state, fixtures, and arbitrary-input corpus | Partial — real-device packet exchange remains |
| 3 | Connect, sync, preset, parameter, save, and reconnect paths | Application and USB lifecycle tests plus optimized target builds | Partial — complete physical lifecycle remains |
| 4 | No Valeton or TONEX Pedal runtime branch | Product-scope gate and source inspection of active Rust paths | Proven |
| 5 | Application core independent of boards and ESP-IDF | `no_std` domain/protocol/application crates and dependency boundaries | Proven |
| 6 | Existing ESP32-S3 boards registered modularly | 22 build variants composed from 13 hardware profiles | Proven for repository-derived metadata |
| 7 | Every registered board compiles | Optimized 22-board core/BLE-only/Wi-Fi+Serial matrices and 20 supported 8/16MiB maximum-feature builds; 4MiB Wi-Fi+BLE is rejected by policy | Proven |
| 8 | Unverified boards have an explicit tier | All descriptors remain Tier 3 pending hardware | Proven |
| 9 | LCD/touch initialization is not copied per board | Reusable component drivers plus declarative descriptors | Proven |
| 10 | Core state changes pass through `AppController` | Typed command boundary and private state ownership | Proven |
| 11 | No mutable internal-pointer API | Boundary/API inspection and unsafe gate | Proven |
| 12 | Unsafe is limited and justified | 110 approved, documented low-level/FFI blocks; automated inventory gate | Proven statically |
| 13 | Settings have schema version and migration | Fixed v5 encoding, explicit v0–v4 migration, validation tests | Partial — NVS power-cycle remains |
| 14 | Core tests and static checks pass | 178 tests; format, lint, scope, dependency, unsafe, Web, package, and build gates pass | Proven on the current workstation |
| 15 | Headless and representative display integration | Headless and all display transports compile; renderer previews exist | Partial — representative physical boards remain |
| 16 | Build, flash, board-addition, test, release procedures documented | README and migration documentation plus validation/package scripts | Proven |
| 17 | Remaining hardware work is reported honestly | Verification ledger, hardware checklist, performance ledger, and audit request | Proven |
| 18 | Old/new parity and removed scope appear in final report | Evidence is distributed across migration documents | Partial — final parity verdict waits for hardware |
| 19 | Temporary, dead, duplicate, and debug code cleaned | Active Rust tree passes lint, dependency, scope, and structure gates | Proven for the active product path |
| 20 | Mixed-device assets/dependencies absent from final firmware | Active builds and verification package exclude historical product paths | Proven for product output |

## Additional requested outcomes

| Outcome | Evidence | Verdict |
| --- | --- | --- |
| Board UI and Wi-Fi Web UI use one design language | Both use the same warm-dark palette, accented preset identity, tone/amp panel, A/B/C cards, and slot/preset navigation | Partial — current physical display and ESP-hosted browser regression remain |
| Presets choose visuals automatically from amp/tone attributes | Deterministic name-keyword profile classification with gain fallback; exact amp identity is not present in the verified protocol data | Proven within available metadata |
| Maximum practical performance | Host render/serialization benchmarks, conservative 4MiB partition 94.76%→76.88% app-image optimization (62.10% on the JC large partition), pixel-identical RLE validation, and target diagnostics | Partial — on-device latency and endurance measurements remain |
| Unnecessary legacy is removed | Historical mixed-device material is isolated under `legacy/` and excluded from product/release output | Partial — evidence tree is intentionally retained until parity and independent audit |
| Pointless comments are removed | Active product path was reviewed; the hygiene gate rejects debt markers and commented-out Rust while retaining safety invariants, API contracts, protocol provenance, and hardware constraints | Proven for the active product path |
| Independent Codex verification is possible | Current checksum-protected package, strict audit request, 995-file integrity check, and extracted-package host gate pass | Partial — a separate clean workstation has not audited it |

## Release decision

**NOT COMPLETE — PHYSICAL AND INDEPENDENT EVIDENCE MISSING.**

The remaining release gates are the physical hardware matrix (including the
TONEX ONE lifecycle, representative display buses, controls, radio features,
NVS, measured performance, and endurance) and reproduction of the complete
verification package on an independent clean workstation. Until both finish,
the legacy evidence must remain and every board remains Tier 3.
