# TONEX ONE Rust firmware

This directory contains the replacement firmware under construction. The
existing ESP-IDF C firmware remains the behavioral reference until the Rust
implementation passes the migration gates documented in
`docs/rust-migration/`.

The first workspace crates deliberately build on the host without ESP-IDF:

- `tonex-domain`: validated TONEX ONE value types.
- `tonex-protocol`: framing, CRC, and streaming frame extraction.
- `hal-contracts`: hardware-independent device contracts.
- `tonex-application`: the single-owner application state machine.
- `tonex-boards`: compile-time board metadata and capability descriptions.

Run the host validation with:

```text
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

ESP-IDF integration will live behind target-specific adapters and must not
leak ESP-IDF types into the crates above.

