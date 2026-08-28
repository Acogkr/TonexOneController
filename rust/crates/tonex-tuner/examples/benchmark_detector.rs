use std::time::Instant;

use tonex_tuner::{TunerConfig, YinDetector};

const SAMPLE_RATE: u16 = 16_000;
const FRAME_SAMPLES: u16 = 2_048;
const ITERATIONS: u16 = 200;

fn main() {
    let samples: Vec<f32> = (0..FRAME_SAMPLES)
        .map(|index| {
            0.7 * (2.0 * core::f32::consts::PI * 110.0 * f32::from(index) / f32::from(SAMPLE_RATE))
                .sin()
        })
        .collect();
    let mut detector = YinDetector::default();
    let started = Instant::now();
    for _ in 0..ITERATIONS {
        detector
            .detect(&samples, TunerConfig::default())
            .expect("benchmark signal is periodic");
    }
    let elapsed = started.elapsed();
    let average_us = elapsed.as_micros() / u128::from(ITERATIONS);
    println!("frames={ITERATIONS} average_us={average_us}");
}
