#![no_std]

use tonex_domain::{ConnectionState, PresetIndex, Slot, SyncState};
use tonex_settings::WifiSettings;

pub const PRESET_LABEL_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiClass {
    Headless,
    Tiny128,
    Compact320x170,
    Portrait240x280,
    Landscape280x240,
    Medium480x320,
    Large800x480,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BluetoothState {
    Off,
    Searching,
    Connected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiMetrics {
    pub width: u16,
    pub height: u16,
    pub margin: u16,
    pub title_height: u16,
    pub touch_target: u16,
}

impl UiClass {
    #[must_use]
    pub const fn metrics(self) -> UiMetrics {
        match self {
            Self::Headless => metrics(0, 0, 0, 0, 0),
            Self::Tiny128 => metrics(128, 128, 6, 22, 0),
            Self::Compact320x170 => metrics(320, 170, 10, 28, 44),
            Self::Portrait240x280 => metrics(240, 280, 12, 34, 48),
            Self::Landscape280x240 => metrics(280, 240, 12, 32, 48),
            Self::Medium480x320 => metrics(480, 320, 16, 40, 52),
            Self::Large800x480 => metrics(800, 480, 24, 56, 64),
        }
    }
}

const fn metrics(
    width: u16,
    height: u16,
    margin: u16,
    title_height: u16,
    touch_target: u16,
) -> UiMetrics {
    UiMetrics {
        width,
        height,
        margin,
        title_height,
        touch_target,
    }
}

pub const SIGNAL_BLOCK_COUNT: usize = 7;
pub const SETTINGS_ITEM_COUNT: usize = 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UiRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl UiRect {
    #[must_use]
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[must_use]
    pub const fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    #[must_use]
    pub const fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }

    #[must_use]
    pub const fn contains(self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageLayout {
    pub preset_number: UiRect,
    pub preset_name: UiRect,
    pub slot: UiRect,
    pub signal_chain: [UiRect; SIGNAL_BLOCK_COUNT],
    pub skin: Option<UiRect>,
    pub master: UiRect,
    pub bpm: UiRect,
    pub settings: Option<UiRect>,
    pub back: Option<UiRect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsLayout {
    pub items: [UiRect; SETTINGS_ITEM_COUNT],
    pub count: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsItem {
    QuickConnect,
    Display,
    Device,
    Tuner,
}

/// Returns the shared geometry used to draw and hit-test the settings menu.
///
/// Touch-capable displays use a two-column dashboard so the settings page
/// keeps the same modular rhythm as the stage. The tiny non-touch preview
/// remains a compact list because two columns would make its labels unreadable.
#[must_use]
pub const fn settings_layout(class: UiClass, show_display: bool) -> SettingsLayout {
    let metrics = class.metrics();
    let top: u16 = match class {
        UiClass::Tiny128 => 26,
        UiClass::Compact320x170 => 54,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => 68,
        UiClass::Medium480x320 => 80,
        UiClass::Large800x480 => 120,
        UiClass::Headless => 0,
    };
    let bottom = metrics.height.saturating_sub(metrics.margin);
    let width = metrics
        .width
        .saturating_sub(metrics.margin.saturating_mul(2));

    let count = if show_display { 4 } else { 3 };
    if matches!(class, UiClass::Tiny128 | UiClass::Headless) {
        let gap: u16 = if matches!(class, UiClass::Tiny128) {
            2
        } else {
            0
        };
        let height = bottom
            .saturating_sub(top)
            .saturating_sub(gap.saturating_mul(count - 1))
            / count;
        return SettingsLayout {
            items: [
                UiRect::new(metrics.margin, top, width, height),
                UiRect::new(metrics.margin, top + height + gap, width, height),
                UiRect::new(metrics.margin, top + (height + gap) * 2, width, height),
                if show_display {
                    UiRect::new(metrics.margin, top + (height + gap) * 3, width, height)
                } else {
                    UiRect::new(0, 0, 0, 0)
                },
            ],
            count: if show_display { 4 } else { 3 },
        };
    }

    let gap = match class {
        UiClass::Compact320x170 => 6,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => 7,
        UiClass::Medium480x320 => 8,
        UiClass::Large800x480 => 12,
        UiClass::Tiny128 | UiClass::Headless => 0,
    };
    let card_width = width.saturating_sub(gap) / 2;
    let card_height = bottom.saturating_sub(top).saturating_sub(gap) / 2;
    let right = metrics.margin + card_width + gap;
    let lower = top + card_height + gap;
    if show_display {
        SettingsLayout {
            items: [
                UiRect::new(metrics.margin, top, card_width, card_height),
                UiRect::new(right, top, card_width, card_height),
                UiRect::new(metrics.margin, lower, card_width, card_height),
                UiRect::new(right, lower, card_width, card_height),
            ],
            count: 4,
        }
    } else {
        SettingsLayout {
            items: [
                UiRect::new(metrics.margin, top, width, card_height),
                UiRect::new(metrics.margin, lower, card_width, card_height),
                UiRect::new(right, lower, card_width, card_height),
                UiRect::new(0, 0, 0, 0),
            ],
            count: 3,
        }
    }
}

#[must_use]
pub fn settings_item_at(
    class: UiClass,
    show_display: bool,
    x: u16,
    y: u16,
) -> Option<SettingsItem> {
    let layout = settings_layout(class, show_display);
    let index = layout
        .items
        .iter()
        .take(usize::from(layout.count))
        .position(|rect| rect.contains(x, y))?;
    Some(match (show_display, index) {
        (_, 0) => SettingsItem::QuickConnect,
        (true, 1) => SettingsItem::Display,
        (true, 2) | (false, 1) => SettingsItem::Device,
        (true, 3) | (false, 2) => SettingsItem::Tuner,
        _ => return None,
    })
}

#[must_use]
// Keeping every board's immutable geometry in one exhaustive table makes
// cross-board comparison and overlap review safer than scattering coordinates.
#[allow(clippy::too_many_lines)]
pub const fn stage_layout(class: UiClass, has_touch: bool) -> StageLayout {
    let mut layout = match class {
        UiClass::Tiny128 => StageLayout {
            preset_number: UiRect::new(92, 6, 14, 22),
            preset_name: UiRect::new(6, 6, 82, 22),
            slot: UiRect::new(110, 6, 12, 22),
            signal_chain: [
                UiRect::new(6, 34, 26, 20),
                UiRect::new(36, 34, 26, 20),
                UiRect::new(66, 34, 26, 20),
                UiRect::new(96, 34, 26, 20),
                UiRect::new(21, 58, 26, 20),
                UiRect::new(51, 58, 26, 20),
                UiRect::new(81, 58, 26, 20),
            ],
            skin: None,
            master: UiRect::new(6, 88, 56, 34),
            bpm: UiRect::new(66, 88, 56, 34),
            settings: None,
            back: None,
        },
        UiClass::Compact320x170 => StageLayout {
            preset_number: UiRect::new(250, 10, 24, 32),
            preset_name: UiRect::new(60, 10, 182, 32),
            slot: UiRect::new(282, 10, 28, 32),
            signal_chain: [
                UiRect::new(10, 50, 36, 44),
                UiRect::new(54, 50, 36, 44),
                UiRect::new(98, 50, 36, 44),
                UiRect::new(142, 50, 36, 44),
                UiRect::new(186, 50, 36, 44),
                UiRect::new(230, 50, 36, 44),
                UiRect::new(274, 50, 36, 44),
            ],
            skin: Some(UiRect::new(200, 103, 104, 35)),
            master: UiRect::new(10, 142, 90, 18),
            bpm: UiRect::new(110, 142, 68, 18),
            settings: Some(UiRect::new(8, 4, 44, 44)),
            back: Some(UiRect::new(262, 107, 48, 48)),
        },
        UiClass::Portrait240x280 => StageLayout {
            preset_number: UiRect::new(180, 12, 24, 48),
            preset_name: UiRect::new(64, 12, 108, 48),
            slot: UiRect::new(212, 12, 16, 48),
            signal_chain: [
                UiRect::new(12, 76, 48, 50),
                UiRect::new(68, 76, 48, 50),
                UiRect::new(124, 76, 48, 50),
                UiRect::new(180, 76, 48, 50),
                UiRect::new(40, 134, 48, 50),
                UiRect::new(96, 134, 48, 50),
                UiRect::new(152, 134, 48, 50),
            ],
            skin: Some(UiRect::new(60, 190, 168, 56)),
            master: UiRect::new(12, 252, 80, 20),
            bpm: UiRect::new(100, 252, 68, 20),
            settings: Some(UiRect::new(8, 12, 48, 48)),
            back: Some(UiRect::new(176, 210, 52, 52)),
        },
        UiClass::Landscape280x240 => StageLayout {
            preset_number: UiRect::new(210, 12, 24, 40),
            preset_name: UiRect::new(60, 12, 148, 40),
            slot: UiRect::new(242, 12, 26, 40),
            signal_chain: [
                UiRect::new(12, 68, 58, 42),
                UiRect::new(78, 68, 58, 42),
                UiRect::new(144, 68, 58, 42),
                UiRect::new(210, 68, 58, 42),
                UiRect::new(45, 118, 58, 42),
                UiRect::new(111, 118, 58, 42),
                UiRect::new(177, 118, 58, 42),
            ],
            skin: Some(UiRect::new(150, 166, 118, 39)),
            master: UiRect::new(12, 210, 88, 20),
            bpm: UiRect::new(112, 210, 72, 20),
            settings: Some(UiRect::new(8, 10, 44, 44)),
            back: Some(UiRect::new(216, 176, 52, 52)),
        },
        UiClass::Medium480x320 => StageLayout {
            preset_number: UiRect::new(378, 14, 40, 56),
            preset_name: UiRect::new(102, 14, 268, 56),
            slot: UiRect::new(426, 14, 38, 56),
            signal_chain: [
                UiRect::new(16, 86, 58, 94),
                UiRect::new(81, 86, 58, 94),
                UiRect::new(146, 86, 58, 94),
                UiRect::new(211, 86, 58, 94),
                UiRect::new(276, 86, 58, 94),
                UiRect::new(341, 86, 58, 94),
                UiRect::new(406, 86, 58, 94),
            ],
            skin: Some(UiRect::new(90, 188, 300, 100)),
            master: UiRect::new(16, 294, 112, 18),
            bpm: UiRect::new(144, 294, 72, 18),
            settings: Some(UiRect::new(10, 16, 52, 52)),
            back: Some(UiRect::new(408, 223, 56, 56)),
        },
        UiClass::Large800x480 => StageLayout {
            preset_number: UiRect::new(640, 24, 56, 80),
            preset_name: UiRect::new(160, 24, 478, 80),
            slot: UiRect::new(704, 24, 72, 80),
            signal_chain: [
                UiRect::new(24, 126, 98, 148),
                UiRect::new(133, 126, 98, 148),
                UiRect::new(242, 126, 98, 148),
                UiRect::new(351, 126, 98, 148),
                UiRect::new(460, 126, 98, 148),
                UiRect::new(569, 126, 98, 148),
                UiRect::new(678, 126, 98, 148),
            ],
            skin: Some(UiRect::new(160, 286, 480, 160)),
            master: UiRect::new(24, 452, 180, 20),
            bpm: UiRect::new(228, 452, 120, 20),
            settings: Some(UiRect::new(20, 26, 76, 76)),
            back: Some(UiRect::new(704, 341, 72, 72)),
        },
        UiClass::Headless => StageLayout {
            preset_number: UiRect::new(0, 0, 0, 0),
            preset_name: UiRect::new(0, 0, 0, 0),
            slot: UiRect::new(0, 0, 0, 0),
            signal_chain: [UiRect::new(0, 0, 0, 0); SIGNAL_BLOCK_COUNT],
            skin: None,
            master: UiRect::new(0, 0, 0, 0),
            bpm: UiRect::new(0, 0, 0, 0),
            settings: None,
            back: None,
        },
    };
    if !has_touch {
        layout.settings = None;
        layout.back = None;
    }
    layout
}

#[must_use]
pub const fn ui_class_for_dimensions(width: u16, height: u16) -> Option<UiClass> {
    match (width, height) {
        (128, 128) => Some(UiClass::Tiny128),
        (320, 170) => Some(UiClass::Compact320x170),
        (240, 280) => Some(UiClass::Portrait240x280),
        (280, 240) => Some(UiClass::Landscape280x240),
        (480, 320) => Some(UiClass::Medium480x320),
        (800, 480) => Some(UiClass::Large800x480),
        _ => None,
    }
}

#[must_use]
pub const fn settings_rect_for_dimensions(width: u16, height: u16) -> Option<UiRect> {
    let Some(class) = ui_class_for_dimensions(width, height) else {
        return None;
    };
    stage_layout(class, true).settings
}

#[must_use]
pub const fn back_rect_for_dimensions(width: u16, height: u16) -> Option<UiRect> {
    let Some(class) = ui_class_for_dimensions(width, height) else {
        return None;
    };
    stage_layout(class, true).back
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetLabel {
    bytes: [u8; PRESET_LABEL_CAPACITY],
    len: usize,
}

impl PresetLabel {
    pub const EMPTY: Self = Self {
        bytes: [0; PRESET_LABEL_CAPACITY],
        len: 0,
    };

    #[must_use]
    pub fn from_bytes(input: &[u8]) -> Self {
        let source_len = input
            .iter()
            .position(|byte| matches!(byte, 0 | 0xff))
            .unwrap_or(input.len());
        let mut len = source_len.min(PRESET_LABEL_CAPACITY);
        while len != 0 && core::str::from_utf8(&input[..len]).is_err() {
            len -= 1;
        }
        let mut bytes = [0; PRESET_LABEL_CAPACITY];
        bytes[..len].copy_from_slice(&input[..len]);
        Self { bytes, len }
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

impl Default for PresetLabel {
    fn default() -> Self {
        Self::from_bytes(b"Preset 1")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
// These are independent hardware bypass states, not mutually exclusive modes.
#[allow(clippy::struct_excessive_bools)]
pub struct FxState {
    pub gate: bool,
    pub compressor: bool,
    pub amp: bool,
    pub cabinet: bool,
    pub modulation: bool,
    pub delay: bool,
    pub reverb: bool,
}

impl Default for FxState {
    fn default() -> Self {
        Self {
            gate: false,
            compressor: false,
            amp: true,
            cabinet: true,
            modulation: false,
            delay: false,
            reverb: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditBlock {
    Dynamics,
    Eq,
    Amp,
    Cabinet,
    Modulation,
    Delay,
    Reverb,
}

pub const EDIT_BLOCKS: [EditBlock; 7] = [
    EditBlock::Dynamics,
    EditBlock::Eq,
    EditBlock::Amp,
    EditBlock::Cabinet,
    EditBlock::Modulation,
    EditBlock::Delay,
    EditBlock::Reverb,
];

impl EditBlock {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dynamics => "DYNAMICS",
            Self::Eq => "EQUALIZER",
            Self::Amp => "AMP",
            Self::Cabinet => "CAB / VIR",
            Self::Modulation => "MODULATION",
            Self::Delay => "DELAY",
            Self::Reverb => "REVERB",
        }
    }

    #[must_use]
    pub const fn contains_parameter(self, raw_id: u16) -> bool {
        match self {
            Self::Dynamics => raw_id <= 9,
            Self::Eq => raw_id >= 10 && raw_id <= 17,
            Self::Amp => (raw_id >= 18 && raw_id <= 22) || (raw_id >= 34 && raw_id <= 35),
            Self::Cabinet => raw_id >= 23 && raw_id <= 33,
            Self::Reverb => raw_id >= 36 && raw_id <= 62,
            Self::Modulation => raw_id >= 63 && raw_id <= 93,
            Self::Delay => raw_id >= 94 && raw_id <= 108,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiPage {
    #[default]
    Stage,
    Presets {
        offset: u8,
    },
    EditMenu {
        offset: u8,
    },
    Edit {
        block: EditBlock,
        offset: u8,
    },
    Global {
        offset: u8,
    },
    Settings,
    QuickConnect,
    Display,
    DeviceInfo,
    Tuner,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TunerState {
    /// Availability and signal quality from the verified pitch source.
    pub signal: TunerSignalState,
    /// MIDI note number when pitch data is available.
    pub note: Option<u8>,
    /// Signed deviation from the target note.
    pub cents: f32,
    /// Measured fundamental frequency from the verified pitch source.
    pub frequency_hz: f32,
    /// Equal-tempered target frequency for `note` and `reference_hz`.
    pub target_hz: f32,
    pub reference_hz: u16,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TunerSignalState {
    /// No verified audio/telemetry adapter exists.
    #[default]
    Unavailable,
    /// The adapter is active but no instrument signal is present.
    Waiting,
    /// Audio is present but below the reliable pitch threshold.
    Weak,
    /// A stable pitch estimate is available.
    Stable,
}

/// Display-ready 24-bit theme colour derived from the active TONEX preset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThemeColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl ThemeColor {
    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    #[must_use]
    pub const fn hex(self) -> u32 {
        (self.red as u32) << 16 | (self.green as u32) << 8 | self.blue as u32
    }
}

impl Default for TunerState {
    fn default() -> Self {
        Self {
            signal: TunerSignalState::Unavailable,
            note: None,
            cents: 0.0,
            frequency_hz: 0.0,
            target_hz: 0.0,
            reference_hz: 440,
            muted: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PerformanceMuteState {
    #[default]
    Inactive,
    Active,
}

impl PerformanceMuteState {
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSnapshot {
    pub connection: ConnectionState,
    pub bluetooth: BluetoothState,
    pub bluetooth_devices: [Option<BluetoothDevice>; 8],
    pub sync: SyncState,
    pub preset: PresetIndex,
    pub slot: Slot,
    pub slot_presets: [PresetIndex; 3],
    pub preset_label: PresetLabel,
    pub preset_labels: [PresetLabel; tonex_domain::PRESET_COUNT],
    pub bpm: f32,
    /// Per-preset model output level (TONEX parameter 21, range 0..10).
    pub preset_volume: f32,
    /// Dedicated global master volume retained for MIDI/global settings.
    pub master_volume_db: f32,
    pub model_gain: f32,
    pub preset_color: Option<ThemeColor>,
    pub bypass: bool,
    /// Controller-managed -40 dB performance mute state.
    pub mute: PerformanceMuteState,
    pub fx: FxState,
    pub page: UiPage,
    pub tuner: TunerState,
    pub board_label: PresetLabel,
    pub display_width: u16,
    pub display_height: u16,
    pub touch_ready: bool,
    pub brightness_dimmable: bool,
    pub display_brightness_percent: u8,
    pub wifi: WifiSettings,
    pub skin: tonex_skins::ResolvedSkin,
    pub skin_selection: tonex_skins::SkinSelection,
    pub parameters: [f32; tonex_parameters::STORED_PARAMETER_COUNT],
}

impl Default for UiSnapshot {
    fn default() -> Self {
        Self {
            connection: ConnectionState::Disconnected,
            bluetooth: BluetoothState::Off,
            bluetooth_devices: [None; 8],
            sync: SyncState::NotStarted,
            preset: PresetIndex::FIRST,
            slot: Slot::A,
            slot_presets: [PresetIndex::FIRST; 3],
            preset_label: PresetLabel::default(),
            preset_labels: [PresetLabel::EMPTY; tonex_domain::PRESET_COUNT],
            bpm: 120.0,
            preset_volume: 5.0,
            master_volume_db: 0.0,
            model_gain: 5.0,
            preset_color: None,
            bypass: false,
            mute: PerformanceMuteState::Inactive,
            fx: FxState::default(),
            page: UiPage::Stage,
            tuner: TunerState::default(),
            board_label: PresetLabel::from_bytes(b"Generic board"),
            display_width: 0,
            display_height: 0,
            touch_ready: false,
            brightness_dimmable: false,
            display_brightness_percent: 100,
            wifi: WifiSettings::default(),
            skin: tonex_skins::ResolvedSkin {
                id: tonex_skins::SkinId::DEFAULT,
                source: tonex_skins::SkinMatchSource::Fallback,
            },
            skin_selection: tonex_skins::SkinSelection::Auto,
            parameters: *tonex_parameters::ParameterStore::with_defaults().values(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BluetoothDevice {
    pub address: [u8; 6],
    pub address_type: u8,
    pub name: PresetLabel,
    pub rssi: i8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToneProfile {
    Clean,
    British,
    American,
    Modern,
    Boutique,
    Bass,
    Acoustic,
    Pedal,
    Neutral,
}

impl ToneProfile {
    #[must_use]
    pub fn infer(label: PresetLabel, model_gain: f32) -> Self {
        let name = label.as_bytes();
        for (profile, keywords) in PROFILE_KEYWORDS {
            if keywords
                .iter()
                .any(|keyword| contains_ascii_case_insensitive(name, keyword))
            {
                return profile;
            }
        }
        if model_gain <= 3.0 {
            Self::Clean
        } else if model_gain >= 7.0 {
            Self::Modern
        } else {
            Self::Neutral
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Clean => "CLEAN",
            Self::British => "BRITISH",
            Self::American => "AMERICAN",
            Self::Modern => "MODERN",
            Self::Boutique => "BOUTIQUE",
            Self::Bass => "BASS",
            Self::Acoustic => "ACOUSTIC",
            Self::Pedal => "PEDAL",
            Self::Neutral => "TONE MODEL",
        }
    }
}

const PROFILE_KEYWORDS: [(ToneProfile, &[&[u8]]); 8] = [
    (
        ToneProfile::Bass,
        &[b"bass", b"ampeg", b"svt", b"ba500", b"b-15"],
    ),
    (ToneProfile::Acoustic, &[b"acoustic", b"nylon", b"piezo"]),
    (
        ToneProfile::British,
        &[b"jcm", b"jtm", b"plexi", b"marshall", b"friedman"],
    ),
    (
        ToneProfile::American,
        &[
            b"fender",
            b"twin",
            b"deluxe",
            b"tweed",
            b"silverface",
            b"hot rod",
        ],
    ),
    (
        ToneProfile::Modern,
        &[
            b"5150", b"recto", b"mesa", b"boogie", b"evh", b"diezel", b"soldano",
        ],
    ),
    (
        ToneProfile::Boutique,
        &[b"dumble", b"vox", b"ac30", b"matchless", b"two rock"],
    ),
    (
        ToneProfile::Pedal,
        &[
            b"fuzz",
            b"muff",
            b"klon",
            b"rat",
            b"overdrive",
            b"distortion",
        ],
    ),
    (
        ToneProfile::Clean,
        &[b"clean", b"studio", b"direct", b"di "],
    ),
];

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(needle)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusText {
    Disconnected,
    Connecting,
    Synchronizing { completed: u8 },
    Ready,
    Faulted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiViewModel {
    pub class: UiClass,
    pub metrics: UiMetrics,
    pub snapshot: UiSnapshot,
    pub status: StatusText,
    pub tone_profile: ToneProfile,
    pub has_touch: bool,
}

impl UiViewModel {
    #[must_use]
    pub fn new(class: UiClass, snapshot: UiSnapshot) -> Self {
        let status = match (snapshot.connection, snapshot.sync) {
            (ConnectionState::Disconnected, _) => StatusText::Disconnected,
            (ConnectionState::Connecting, _) => StatusText::Connecting,
            (ConnectionState::Synchronizing, SyncState::InProgress { completed }) => {
                StatusText::Synchronizing { completed }
            }
            (ConnectionState::Synchronizing, _) => StatusText::Synchronizing { completed: 0 },
            (ConnectionState::Ready, _) => StatusText::Ready,
            (ConnectionState::Faulted, _) => StatusText::Faulted,
        };
        let metrics = class.metrics();
        let tone_profile = ToneProfile::infer(snapshot.preset_label, snapshot.model_gain);
        Self {
            class,
            metrics,
            snapshot,
            status,
            tone_profile,
            has_touch: false,
        }
    }

    #[must_use]
    pub const fn with_touch(mut self, has_touch: bool) -> Self {
        self.has_touch = has_touch;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_classes_have_consistent_metrics() {
        let classes = [
            UiClass::Headless,
            UiClass::Tiny128,
            UiClass::Compact320x170,
            UiClass::Portrait240x280,
            UiClass::Landscape280x240,
            UiClass::Medium480x320,
            UiClass::Large800x480,
        ];
        for class in classes {
            let value = class.metrics();
            if class == UiClass::Headless {
                assert_eq!(value.width, 0);
            } else {
                assert!(value.width > value.margin * 2);
                assert!(value.height > value.title_height);
            }
        }
    }

    #[test]
    fn label_is_bounded_and_tolerates_device_bytes() {
        assert_eq!(
            PresetLabel::from_bytes(&[b'A', b'B', 0xff, b'C']).as_bytes(),
            b"AB"
        );
        assert_eq!(
            PresetLabel::from_bytes(&[b'X'; 64]).as_bytes().len(),
            PRESET_LABEL_CAPACITY
        );
        let mut boundary = [b'X'; 35];
        boundary[30..33].copy_from_slice("톤".as_bytes());
        assert_eq!(
            core::str::from_utf8(PresetLabel::from_bytes(&boundary).as_bytes()),
            Ok("XXXXXXXXXXXXXXXXXXXXXXXXXXXXXX")
        );
    }

    #[test]
    fn medium_chain_uses_one_size_and_seven_pixel_gaps() {
        let layout = stage_layout(UiClass::Medium480x320, true);
        for block in layout.signal_chain {
            assert_eq!((block.width, block.height), (58, 94));
        }
        for pair in layout.signal_chain.windows(2) {
            assert_eq!(pair[1].x - pair[0].right(), 7);
        }
        assert_eq!(layout.settings, Some(UiRect::new(10, 16, 52, 52)));
        assert_eq!(layout.back, Some(UiRect::new(408, 223, 56, 56)));
        assert_eq!(stage_layout(UiClass::Medium480x320, false).settings, None);
        assert_eq!(stage_layout(UiClass::Medium480x320, false).back, None);
    }

    #[test]
    fn settings_cards_are_disjoint_and_share_hit_test_geometry() {
        for class in [
            UiClass::Compact320x170,
            UiClass::Portrait240x280,
            UiClass::Landscape280x240,
            UiClass::Medium480x320,
            UiClass::Large800x480,
        ] {
            let layout = settings_layout(class, true);
            for (index, rect) in layout.items.iter().enumerate() {
                assert!(rect.width >= class.metrics().touch_target);
                assert!(rect.height >= class.metrics().touch_target);
                assert_eq!(
                    settings_item_at(
                        class,
                        true,
                        rect.x + rect.width / 2,
                        rect.y + rect.height / 2
                    ),
                    Some(
                        [
                            SettingsItem::QuickConnect,
                            SettingsItem::Display,
                            SettingsItem::Device,
                            SettingsItem::Tuner,
                        ][index]
                    )
                );
                for other in layout.items.iter().skip(index + 1) {
                    assert!(
                        rect.right() <= other.x
                            || other.right() <= rect.x
                            || rect.bottom() <= other.y
                            || other.bottom() <= rect.y
                    );
                }
            }

            let fixed = settings_layout(class, false);
            assert_eq!(fixed.count, 3);
            assert_eq!(
                fixed.items[0].width,
                class.metrics().width - class.metrics().margin * 2
            );
            assert_eq!(
                settings_item_at(
                    class,
                    false,
                    fixed.items[1].x + fixed.items[1].width / 2,
                    fixed.items[1].y + fixed.items[1].height / 2
                ),
                Some(SettingsItem::Device)
            );
        }
    }

    #[test]
    fn synchronization_status_is_derived_without_mutation() {
        let snapshot = UiSnapshot {
            connection: ConnectionState::Synchronizing,
            sync: SyncState::InProgress { completed: 12 },
            ..UiSnapshot::default()
        };
        let model = UiViewModel::new(UiClass::Portrait240x280, snapshot).with_touch(true);
        assert_eq!(model.status, StatusText::Synchronizing { completed: 12 });
        assert!(model.has_touch);
        assert_eq!(model.snapshot, snapshot);
    }

    #[test]
    fn tone_profile_prefers_explicit_name_evidence() {
        assert_eq!(
            ToneProfile::infer(PresetLabel::from_bytes(b"My JCM 800"), 2.0),
            ToneProfile::British
        );
        assert_eq!(
            ToneProfile::infer(PresetLabel::from_bytes(b"SVT Bass"), 9.0),
            ToneProfile::Bass
        );
        assert_eq!(
            ToneProfile::infer(PresetLabel::from_bytes(b"AC30 CHIME"), 8.0),
            ToneProfile::Boutique
        );
    }

    #[test]
    fn tone_profile_uses_gain_only_when_name_has_no_model_hint() {
        assert_eq!(
            ToneProfile::infer(PresetLabel::from_bytes(b"My preset"), 2.5),
            ToneProfile::Clean
        );
        assert_eq!(
            ToneProfile::infer(PresetLabel::from_bytes(b"My preset"), 8.0),
            ToneProfile::Modern
        );
        assert_eq!(
            ToneProfile::infer(PresetLabel::from_bytes(b"My preset"), 5.0),
            ToneProfile::Neutral
        );
    }
}
