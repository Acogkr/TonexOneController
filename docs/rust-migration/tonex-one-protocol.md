# TONEX ONE protocol migration notes

Status: reverse-engineered baseline; hardware confirmation pending

## Sources

The current authoritative implementation is:

- `legacy/source/main/usb_tonex_common.c`
- `legacy/source/main/usb_tonex_one.c`
- `legacy/source/main/usb_tonex_common.h`
- `legacy/source/main/usb_tonex_one.h`

No sanitized physical USB capture is present in this checkout. Constants
derived only from source code remain unconfirmed until compared with an
actual device capture.

## Frame layer

- boundary: `0x7e`
- escape: `0x7d`
- escaped byte: byte XOR `0x20`
- CRC: CRC-16/X-25
- CRC initial value: `0xffff`
- reversed polynomial: `0x8408`
- result complemented
- CRC serialized little-endian before escaping

The Rust implementation rejects missing boundaries, incomplete escape
sequences, short CRC fields, CRC mismatches, and configured streaming-size
overflow.

## Header layer

Decoded messages start with `b9 03`, followed by three variable-width
integers:

1. message type;
2. body length;
3. an unknown field, commonly `0x0b`.

The C reference decodes the integer forms as:

| Form | Decoded value |
| --- | --- |
| `VV` | one-byte `VV` |
| `80 VV` | one-byte `VV` |
| `81 LL HH` | little-endian `HHLL` |
| `82 LL HH` | little-endian `HHLL` |

Known response types:

| Value | Meaning |
| ---: | --- |
| `0x0002` | hello |
| `0x0303` | full preset details |
| `0x0304` | preset summary |
| `0x0306` | state update |
| `0x0309` | parameter changed |

Rust validates that the remaining decoded bytes exactly equal the declared
body length. Unknown message types are preserved as values instead of being
silently reclassified.

## Boot synchronization

The intended sequence reconstructed from the C implementation is:

```text
connect
  -> hello request
  <- hello response
  -> state request
  <- state update
  -> preset summaries 0 through 19
  -> current preset summary/details
  -> global volume request
  <- parameter-changed/global response
  -> ready
```

The legacy C loop requests index 20 after storing preset 19, even though
`MAX_PRESETS_TONEX_ONE` is 20 and normal preset indices are `0..19`. It waits
for that response before requesting the current preset. The Rust state machine
now preserves this boot sentinel request because same-board physical testing
showed that source-level cleanup of the sequence was not strong enough
evidence to diverge from the stable C firmware.

## Static request fixtures

The hello, state, preset-summary, and global-volume request payloads are
copied byte-for-byte from the C reference and framed by the tested Rust
framer. They are source-derived fixtures, not yet capture-derived fixtures.

## Unresolved evidence

- meaning of the `0x80/0x81/0x82` integer tags;
- meaning and valid values of the header's third integer;
- response shape for the global-volume request;
- whether all firmware versions return `ParameterChanged` for global volume;
- retry and timeout requirements for each synchronization stage;
- maximum valid state and preset-summary body sizes on current TONEX ONE
  firmware;
- device behavior when a request is retransmitted after reconnect.
