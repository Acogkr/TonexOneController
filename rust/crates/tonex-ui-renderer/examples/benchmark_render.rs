use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use tonex_ui_model::{UiClass, UiSnapshot, UiViewModel};
use tonex_ui_renderer::render;

const DEFAULT_SAMPLES: usize = 100;

fn main() {
    let samples = std::env::args().nth(1).map_or(DEFAULT_SAMPLES, |value| {
        value
            .parse::<usize>()
            .ok()
            .filter(|samples| *samples > 0)
            .expect("sample count must be a positive integer")
    });
    for class in [
        UiClass::Tiny128,
        UiClass::Compact320x170,
        UiClass::Portrait240x280,
        UiClass::Landscape280x240,
        UiClass::Medium480x320,
        UiClass::Large800x480,
    ] {
        measure(class, samples);
    }
}

fn measure(class: UiClass, sample_count: usize) {
    let model = UiViewModel::new(class, UiSnapshot::default());
    let metrics = class.metrics();
    let pixel_count = usize::from(metrics.width) * usize::from(metrics.height);
    let mut pixels = vec![0_u16; pixel_count];

    for _ in 0..10 {
        render(black_box(&model), black_box(&mut pixels)).expect("warm-up render");
    }

    let mut samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let start = Instant::now();
        render(black_box(&model), black_box(&mut pixels)).expect("measured render");
        samples.push(start.elapsed());
    }
    samples.sort_unstable();

    println!(
        "UI_RENDER class={class:?} pixels={pixel_count} samples={sample_count} median_us={} p95_us={} max_us={}",
        micros(samples[sample_count / 2]),
        micros(samples[sample_count * 95 / 100]),
        micros(samples[sample_count - 1])
    );
    black_box(pixels);
}

fn micros(duration: Duration) -> u128 {
    duration.as_micros()
}
