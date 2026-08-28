use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use tonex_settings::MidiSettings;
use tonex_ui_model::UiSnapshot;
use tonex_web::{MAX_SNAPSHOT_BYTES, encode_snapshot_with_midi};

const SAMPLES: usize = 20_000;

fn main() {
    let snapshot = UiSnapshot::default();
    let midi = MidiSettings {
        channel: 16,
        serial_enabled: true,
        ble_enabled: true,
        paired_peer: None,
    };
    let mut output = [0_u8; MAX_SNAPSHOT_BYTES];

    for _ in 0..1_000 {
        encode_snapshot_with_midi(black_box(&snapshot), Some(midi), black_box(&mut output))
            .expect("warm-up encode");
    }

    let mut samples = Vec::with_capacity(SAMPLES);
    let mut encoded_bytes = 0;
    for _ in 0..SAMPLES {
        let start = Instant::now();
        encoded_bytes =
            encode_snapshot_with_midi(black_box(&snapshot), Some(midi), black_box(&mut output))
                .expect("measured encode")
                .len();
        samples.push(start.elapsed());
    }
    samples.sort_unstable();

    println!(
        "WEB_SNAPSHOT bytes={encoded_bytes} capacity={MAX_SNAPSHOT_BYTES} samples={SAMPLES} median_ns={} p95_ns={} max_ns={}",
        nanos(samples[SAMPLES / 2]),
        nanos(samples[SAMPLES * 95 / 100]),
        nanos(samples[SAMPLES - 1])
    );
    black_box(output);
}

fn nanos(duration: Duration) -> u128 {
    duration.as_nanos()
}
