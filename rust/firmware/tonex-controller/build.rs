use std::env;

const BOARD_FEATURES: &[(&str, &str)] = &[
    ("CARGO_FEATURE_BOARD_WAVESHARE_169", "waveshare-169"),
    (
        "CARGO_FEATURE_BOARD_WAVESHARE_169_LANDSCAPE",
        "waveshare-169-landscape",
    ),
    (
        "CARGO_FEATURE_BOARD_WAVESHARE_169_TOUCH",
        "waveshare-169-touch",
    ),
    (
        "CARGO_FEATURE_BOARD_WAVESHARE_169_TOUCH_LANDSCAPE",
        "waveshare-169-touch-landscape",
    ),
    ("CARGO_FEATURE_BOARD_WAVESHARE_43B", "waveshare-43b"),
    ("CARGO_FEATURE_BOARD_WAVESHARE_35B", "waveshare-35b"),
    ("CARGO_FEATURE_BOARD_JC3248W535", "jc3248w535"),
    ("CARGO_FEATURE_BOARD_WAVESHARE_ZERO", "waveshare-zero"),
    ("CARGO_FEATURE_BOARD_DEVKITC_N8R2", "devkitc-n8r2"),
    ("CARGO_FEATURE_BOARD_DEVKITC_N16R8", "devkitc-n16r8"),
    ("CARGO_FEATURE_BOARD_M5_ATOMS3R", "m5-atoms3r"),
    (
        "CARGO_FEATURE_BOARD_LILYGO_TDISPLAY_S3",
        "lilygo-tdisplay-s3",
    ),
    (
        "CARGO_FEATURE_BOARD_WAVESHARE_19_TOUCH",
        "waveshare-19-touch",
    ),
    ("CARGO_FEATURE_BOARD_WAVESHARE_7_43", "waveshare-7-43"),
    ("CARGO_FEATURE_BOARD_PIRATE_POLAR_MINI", "pirate-polar-mini"),
    ("CARGO_FEATURE_BOARD_PIRATE_POLAR_PLUS", "pirate-polar-plus"),
    ("CARGO_FEATURE_BOARD_PIRATE_POLAR_ZERO", "pirate-polar-zero"),
    ("CARGO_FEATURE_BOARD_PIRATE_POLAR_43B", "pirate-polar-43b"),
    (
        "CARGO_FEATURE_BOARD_PIRATE_POLAR_MINI_V2",
        "pirate-polar-mini-v2",
    ),
    (
        "CARGO_FEATURE_BOARD_PIRATE_POLAR_PLUS_V2",
        "pirate-polar-plus-v2",
    ),
    ("CARGO_FEATURE_BOARD_PIRATE_POLAR_PRO", "pirate-polar-pro"),
    (
        "CARGO_FEATURE_BOARD_PIRATE_POLAR_MAX_V2",
        "pirate-polar-max-v2",
    ),
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        embuild::espidf::sysenv::output();
    }

    let host_sim = env::var_os("CARGO_FEATURE_HOST_SIM").is_some();
    let selected: Vec<_> = BOARD_FEATURES
        .iter()
        .filter(|(feature, _)| env::var_os(feature).is_some())
        .collect();

    if host_sim {
        assert!(
            selected.is_empty(),
            "host-sim cannot be combined with a board feature"
        );
        println!("cargo:rustc-env=TONEX_BOARD_ID=host-sim");
        return;
    }

    assert!(
        selected.len() == 1,
        "firmware requires exactly one board feature; selected {}",
        selected.len()
    );
    println!("cargo:rustc-env=TONEX_BOARD_ID={}", selected[0].1);
}
