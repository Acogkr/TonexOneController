# Legacy reference boundary

This directory contains the former mixed-device C firmware, historical release
archives, generated distribution bundles, UI editor projects, skins, datasheets,
and small development utilities.

None of these paths are part of the current TONEX ONE Rust firmware build or
release. They remain temporarily available as protocol, board-wiring, visual,
and binary parity evidence until the physical validation and independent
workstation gates in `docs/rust-migration/legacy-boundary.md` pass.

The only automated read from this directory is the deterministic TONEX
parameter-registry provenance check. The legacy C workflow remains
manual-dispatch only.

`.embuild-cache/` is a protected, reproducible ESP-IDF build cache that the
current execution policy would not delete. It is ignored and is not referenced
by either product or verification tooling; it can be removed locally without
losing source or validation evidence.
