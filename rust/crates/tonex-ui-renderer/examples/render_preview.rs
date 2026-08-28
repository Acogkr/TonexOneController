use std::{
    env, fs,
    path::{Path, PathBuf},
};

use tonex_domain::{ConnectionState, PresetIndex, Slot, SyncState};
use tonex_ui_model::{
    EditBlock, FxState, PresetLabel, TunerSignalState, TunerState, UiClass, UiPage, UiSnapshot,
    UiViewModel,
};
use tonex_ui_renderer::render;

#[allow(clippy::too_many_lines)]
fn main() {
    let output = env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("ui-previews"), PathBuf::from);
    fs::create_dir_all(&output).expect("create output directory");
    for (name, label, gain) in [
        ("british", b"JCM 800 LEAD".as_slice(), 6.8),
        ("american", b"FENDER TWIN".as_slice(), 2.4),
        ("modern", b"5150 HIGH GAIN".as_slice(), 8.7),
        ("boutique", b"AC30 CHIME".as_slice(), 4.2),
        ("bass", b"SVT BASS".as_slice(), 5.6),
        ("neutral", b"MY FAVORITE TONE".as_slice(), 5.0),
    ] {
        write_preview(
            &output,
            name,
            UiClass::Medium480x320,
            label,
            gain,
            UiPage::Stage,
            ConnectionState::Ready,
        );
    }
    let pages = [
        ("stage", UiPage::Stage),
        ("presets", UiPage::Presets { offset: 5 }),
        ("edit-menu", UiPage::EditMenu { offset: 0 }),
        (
            "edit-amp",
            UiPage::Edit {
                block: EditBlock::Amp,
                offset: 0,
            },
        ),
        ("global", UiPage::Global { offset: 0 }),
        ("settings", UiPage::Settings),
        ("quick-connect", UiPage::QuickConnect),
        ("display", UiPage::Display),
        ("device", UiPage::DeviceInfo),
        ("tuner", UiPage::Tuner),
    ];
    for (class_name, class) in [
        ("tiny", UiClass::Tiny128),
        ("compact", UiClass::Compact320x170),
        ("portrait", UiClass::Portrait240x280),
        ("landscape", UiClass::Landscape280x240),
        ("medium", UiClass::Medium480x320),
        ("large", UiClass::Large800x480),
    ] {
        for (page_name, page) in pages {
            write_preview(
                &output,
                &format!("{page_name}-{class_name}"),
                class,
                b"JCM 800 LEAD",
                6.8,
                page,
                ConnectionState::Ready,
            );
        }
    }
    write_preview(
        &output,
        "offline-medium",
        UiClass::Medium480x320,
        b"JCM 800 LEAD",
        6.8,
        UiPage::Stage,
        ConnectionState::Disconnected,
    );
}

fn write_preview(
    output: &Path,
    name: &str,
    class: UiClass,
    label: &[u8],
    gain: f32,
    page: UiPage,
    connection: ConnectionState,
) {
    let metrics = class.metrics();
    let names: [&[u8]; tonex_domain::PRESET_COUNT] = [
        b"CLEAN TWIN",
        b"AC30 CHIME",
        b"PLEXI RHYTHM",
        b"JCM CRUNCH",
        b"RECTO LEAD",
        b"AMBIENT CLEAN",
        b"EDGE DRIVE",
        b"JCM 800 LEAD",
        b"FUZZ STACK",
        b"BASS SVT",
        b"MODERN 5150",
        b"DELUXE BREAKUP",
        b"ORANGE DRIVE",
        b"D STYLE CLEAN",
        b"MARK V LEAD",
        b"SUPRO GRIT",
        b"JAZZ CHORUS",
        b"TWEED EDGE",
        b"FRIEDMAN BE",
        b"MY FAVORITE",
    ];
    let mut preset_labels = names.map(PresetLabel::from_bytes);
    preset_labels[7] = PresetLabel::from_bytes(label);
    let snapshot = UiSnapshot {
        connection,
        bluetooth: tonex_ui_model::BluetoothState::Connected,
        bluetooth_devices: [None; 8],
        sync: if connection == ConnectionState::Ready {
            SyncState::Complete
        } else {
            SyncState::NotStarted
        },
        preset: PresetIndex::new(7).expect("valid preview preset"),
        slot: Slot::B,
        slot_presets: [PresetIndex::new(7).expect("valid preview preset"); 3],
        preset_label: PresetLabel::from_bytes(label),
        preset_labels,
        bpm: 118.0,
        preset_volume: 5.0,
        master_volume_db: -8.5,
        model_gain: gain,
        preset_color: None,
        bypass: false,
        mute: tonex_ui_model::PerformanceMuteState::Inactive,
        fx: FxState {
            gate: true,
            compressor: true,
            amp: true,
            cabinet: true,
            modulation: false,
            delay: true,
            reverb: true,
        },
        page,
        tuner: TunerState {
            signal: TunerSignalState::Stable,
            note: Some(40),
            cents: -7.0,
            frequency_hz: 81.74,
            target_hz: 82.41,
            reference_hz: 440,
            muted: true,
        },
        board_label: PresetLabel::from_bytes(b"jc3248w535"),
        display_width: metrics.width,
        display_height: metrics.height,
        touch_ready: class != UiClass::Tiny128,
        display_brightness_percent: 100,
        brightness_dimmable: false,
        wifi: UiSnapshot::default().wifi,
        skin: tonex_skins::resolve_skin(tonex_skins::SkinSelection::Auto, None, label, gain),
        skin_selection: tonex_skins::SkinSelection::Auto,
        parameters: *tonex_parameters::ParameterStore::with_defaults().values(),
    };
    let mut pixels = vec![0_u16; usize::from(metrics.width) * usize::from(metrics.height)];
    render(
        &UiViewModel::new(class, snapshot).with_touch(class != UiClass::Tiny128),
        &mut pixels,
    )
    .expect("render preview");
    let bitmap = encode_bmp(metrics.width, metrics.height, &pixels);
    fs::write(output.join(format!("{name}.bmp")), bitmap).expect("write preview");
}

fn encode_bmp(width: u16, height: u16, pixels: &[u16]) -> Vec<u8> {
    let row_bytes = usize::from(width) * 3;
    let row_stride = (row_bytes + 3) & !3;
    let pixel_bytes = row_stride * usize::from(height);
    let mut output = vec![0_u8; 54 + pixel_bytes];
    let file_bytes = u32::try_from(output.len()).unwrap();
    output[0..2].copy_from_slice(b"BM");
    output[2..6].copy_from_slice(&file_bytes.to_le_bytes());
    output[10..14].copy_from_slice(&54_u32.to_le_bytes());
    output[14..18].copy_from_slice(&40_u32.to_le_bytes());
    output[18..22].copy_from_slice(&u32::from(width).to_le_bytes());
    output[22..26].copy_from_slice(&u32::from(height).to_le_bytes());
    output[26..28].copy_from_slice(&1_u16.to_le_bytes());
    output[28..30].copy_from_slice(&24_u16.to_le_bytes());
    output[34..38].copy_from_slice(&u32::try_from(pixel_bytes).unwrap().to_le_bytes());
    for source_y in 0..usize::from(height) {
        let destination_y = usize::from(height) - 1 - source_y;
        let destination = 54 + destination_y * row_stride;
        for x in 0..usize::from(width) {
            let colour = pixels[source_y * usize::from(width) + x];
            let red = u8::try_from(((colour >> 11) & 0x1f) * 255 / 31).unwrap();
            let green = u8::try_from(((colour >> 5) & 0x3f) * 255 / 63).unwrap();
            let blue = u8::try_from((colour & 0x1f) * 255 / 31).unwrap();
            let offset = destination + x * 3;
            output[offset..offset + 3].copy_from_slice(&[blue, green, red]);
        }
    }
    output
}
