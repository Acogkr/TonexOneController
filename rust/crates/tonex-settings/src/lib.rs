#![no_std]

use tonex_domain::{
    FOOT_BINDING_COUNT, FootAction, FootButton, FootGesture, PRESET_COUNT, PresetIndex, Slot,
    foot_binding_index,
};
use tonex_skins::{PresetSkins, SkinSelection};

pub const CURRENT_SCHEMA_VERSION: u8 = 5;
pub const WIFI_SSID_CAPACITY: usize = 32;
pub const WIFI_PASSWORD_CAPACITY: usize = 63;
pub const BLUETOOTH_NAME_CAPACITY: usize = 24;
const V2_ENCODED_LEN: usize = 43 + WIFI_PASSWORD_CAPACITY;
const V3_ENCODED_LEN: usize = V2_ENCODED_LEN + PRESET_COUNT;
const FOOT_OVERRIDE_COUNT: usize = PRESET_COUNT * FOOT_BINDING_COUNT;
const V4_ENCODED_LEN: usize = V3_ENCODED_LEN + FOOT_BINDING_COUNT + FOOT_OVERRIDE_COUNT;
const BLUETOOTH_PEER_ENCODED_LEN: usize = 1 + 1 + 6 + 1 + BLUETOOTH_NAME_CAPACITY;
pub const ENCODED_LEN: usize = V4_ENCODED_LEN + BLUETOOTH_PEER_ENCODED_LEN;
const V1_ENCODED_LEN: usize = 8;
const MAGIC: [u8; 2] = *b"TX";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiSettings {
    /// MIDI channels are represented as the user-facing range `1..=16`.
    pub channel: u8,
    pub serial_enabled: bool,
    pub ble_enabled: bool,
    pub paired_peer: Option<BluetoothPeer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BluetoothPeer {
    pub address: [u8; 6],
    /// ESP-IDF BLE address type: `0` public, `1` random.
    pub address_type: u8,
    pub name: WifiText<BLUETOOTH_NAME_CAPACITY>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FootControllerSettings {
    global: [FootAction; FOOT_BINDING_COUNT],
    preset: [[FootAction; FOOT_BINDING_COUNT]; PRESET_COUNT],
}

impl Default for FootControllerSettings {
    fn default() -> Self {
        let mut global = [FootAction::Disabled; FOOT_BINDING_COUNT];
        global[foot_binding_index(FootButton::One, FootGesture::ShortPress)] =
            FootAction::SelectSlot(Slot::A);
        global[foot_binding_index(FootButton::Two, FootGesture::ShortPress)] =
            FootAction::SelectSlot(Slot::B);
        global[foot_binding_index(FootButton::Three, FootGesture::ShortPress)] =
            FootAction::SelectSlot(Slot::C);
        global[foot_binding_index(FootButton::Four, FootGesture::ShortPress)] =
            FootAction::TapTempo;
        global[foot_binding_index(FootButton::One, FootGesture::LongPress)] =
            FootAction::PreviousPreset;
        global[foot_binding_index(FootButton::Two, FootGesture::LongPress)] =
            FootAction::NextPreset;
        global[foot_binding_index(FootButton::Three, FootGesture::LongPress)] =
            FootAction::ToggleEffect(tonex_domain::EffectBlock::Delay);
        global[foot_binding_index(FootButton::Four, FootGesture::LongPress)] =
            FootAction::ToggleEffect(tonex_domain::EffectBlock::Reverb);
        Self {
            global,
            preset: [[FootAction::Inherit; FOOT_BINDING_COUNT]; PRESET_COUNT],
        }
    }
}

impl FootControllerSettings {
    #[must_use]
    pub const fn global(&self, button: FootButton, gesture: FootGesture) -> FootAction {
        self.global[foot_binding_index(button, gesture)]
    }

    pub fn set_global(&mut self, button: FootButton, gesture: FootGesture, action: FootAction) {
        self.global[foot_binding_index(button, gesture)] = match action {
            FootAction::Inherit => FootAction::Disabled,
            concrete => concrete,
        };
    }

    #[must_use]
    pub const fn preset_override(
        &self,
        preset: PresetIndex,
        button: FootButton,
        gesture: FootGesture,
    ) -> FootAction {
        self.preset[preset.get() as usize][foot_binding_index(button, gesture)]
    }

    pub fn set_preset_override(
        &mut self,
        preset: PresetIndex,
        button: FootButton,
        gesture: FootGesture,
        action: FootAction,
    ) {
        self.preset[preset.get() as usize][foot_binding_index(button, gesture)] = action;
    }

    #[must_use]
    pub const fn resolve(
        &self,
        preset: PresetIndex,
        button: FootButton,
        gesture: FootGesture,
    ) -> FootAction {
        match self.preset_override(preset, button, gesture) {
            FootAction::Inherit => self.global(button, gesture),
            concrete => concrete,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WifiMode {
    AccessPoint,
    Station,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiText<const CAPACITY: usize> {
    bytes: [u8; CAPACITY],
    len: u8,
}

impl<const CAPACITY: usize> WifiText<CAPACITY> {
    /// Creates bounded printable ASCII configuration text.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, control-character, and non-ASCII values.
    pub fn new(value: &str) -> Result<Self, SettingsError> {
        let bytes = value.as_bytes();
        if bytes.is_empty() || bytes.len() > CAPACITY || bytes.len() > usize::from(u8::MAX) {
            return Err(SettingsError::InvalidWifiTextLength(bytes.len()));
        }
        if !bytes.iter().all(|byte| matches!(byte, 0x20..=0x7e)) {
            return Err(SettingsError::InvalidWifiText);
        }
        let mut storage = [0_u8; CAPACITY];
        storage[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            bytes: storage,
            len: u8::try_from(bytes.len())
                .map_err(|_| SettingsError::InvalidWifiTextLength(bytes.len()))?,
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WifiSettings {
    pub enabled: bool,
    pub mode: WifiMode,
    pub ssid: WifiText<WIFI_SSID_CAPACITY>,
    pub password: WifiText<WIFI_PASSWORD_CAPACITY>,
}

impl Default for WifiSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: WifiMode::AccessPoint,
            ssid: WifiText::new("TONEX-ONE").expect("default SSID is valid"),
            password: WifiText::new("tonex-one").expect("default password is valid"),
        }
    }
}

impl WifiSettings {
    /// Validates WPA2 credential constraints.
    ///
    /// # Errors
    ///
    /// Rejects passwords outside the WPA2 personal range.
    pub const fn validate(self) -> Result<Self, SettingsError> {
        let password_len = self.password.len as usize;
        if password_len < 8 || password_len > WIFI_PASSWORD_CAPACITY {
            return Err(SettingsError::InvalidWifiPasswordLength(password_len));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Settings {
    pub selected_preset: PresetIndex,
    pub selected_slot: Slot,
    pub display_brightness_percent: u8,
    pub midi: MidiSettings,
    pub wifi: WifiSettings,
    pub preset_skins: PresetSkins,
    pub foot_controller: FootControllerSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            selected_preset: PresetIndex::FIRST,
            selected_slot: Slot::A,
            display_brightness_percent: 100,
            midi: MidiSettings {
                channel: 1,
                serial_enabled: false,
                ble_enabled: true,
                paired_peer: None,
            },
            wifi: WifiSettings::default(),
            preset_skins: PresetSkins::default(),
            foot_controller: FootControllerSettings::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsError {
    InvalidLength(usize),
    InvalidMagic,
    UnsupportedVersion(u8),
    InvalidPreset(u8),
    InvalidSlot(u8),
    InvalidBrightness(u8),
    InvalidMidiChannel(u8),
    InvalidFlags(u8),
    InvalidWifiMode(u8),
    InvalidWifiTextLength(usize),
    InvalidWifiText,
    InvalidWifiPasswordLength(usize),
    InvalidSkinSelection(u8),
    InvalidFootAction(u8),
    InvalidBluetoothAddressType(u8),
}

impl Settings {
    /// Validates values that may originate from UI, MIDI, web, or migration.
    ///
    /// # Errors
    ///
    /// Returns the first value outside its explicitly supported range.
    pub const fn validate(self) -> Result<Self, SettingsError> {
        if self.display_brightness_percent > 100 {
            return Err(SettingsError::InvalidBrightness(
                self.display_brightness_percent,
            ));
        }
        if self.midi.channel == 0 || self.midi.channel > 16 {
            return Err(SettingsError::InvalidMidiChannel(self.midi.channel));
        }
        if let Some(peer) = self.midi.paired_peer
            && peer.address_type > 1
        {
            return Err(SettingsError::InvalidBluetoothAddressType(
                peer.address_type,
            ));
        }
        match self.wifi.validate() {
            Ok(_) => {}
            Err(error) => return Err(error),
        }
        Ok(self)
    }

    #[must_use]
    pub fn encode(self) -> [u8; ENCODED_LEN] {
        let flags = u8::from(self.midi.serial_enabled)
            | (u8::from(self.midi.ble_enabled) << 1)
            | (u8::from(self.wifi.enabled) << 2);
        let mut encoded = [0_u8; ENCODED_LEN];
        encoded[..10].copy_from_slice(&[
            MAGIC[0],
            MAGIC[1],
            CURRENT_SCHEMA_VERSION,
            self.selected_preset.get(),
            slot_to_byte(self.selected_slot),
            self.display_brightness_percent,
            self.midi.channel,
            flags,
            wifi_mode_to_byte(self.wifi.mode),
            self.wifi.ssid.len,
        ]);
        encoded[10..42].copy_from_slice(&self.wifi.ssid.bytes);
        encoded[42] = self.wifi.password.len;
        encoded[43..V2_ENCODED_LEN].copy_from_slice(&self.wifi.password.bytes);
        for (encoded, selection) in encoded[V2_ENCODED_LEN..V3_ENCODED_LEN]
            .iter_mut()
            .zip(self.preset_skins.selections())
        {
            *encoded = selection.encode();
        }
        for (encoded, action) in encoded[V3_ENCODED_LEN..V3_ENCODED_LEN + FOOT_BINDING_COUNT]
            .iter_mut()
            .zip(self.foot_controller.global)
        {
            *encoded = action.encode();
        }
        for (encoded, action) in encoded[V3_ENCODED_LEN + FOOT_BINDING_COUNT..]
            [..FOOT_OVERRIDE_COUNT]
            .iter_mut()
            .zip(self.foot_controller.preset.into_iter().flatten())
        {
            *encoded = action.encode();
        }
        let peer = &mut encoded[V4_ENCODED_LEN..];
        if let Some(paired) = self.midi.paired_peer {
            peer[0] = 1;
            peer[1] = paired.address_type;
            peer[2..8].copy_from_slice(&paired.address);
            peer[8] = paired.name.len;
            peer[9..].copy_from_slice(&paired.name.bytes);
        }
        encoded
    }
}

/// Decodes current settings or explicitly migrates the legacy version-zero
/// layout `[preset, slot, brightness, midi_channel]`.
///
/// # Errors
///
/// Returns a typed error for corrupt lengths, headers, versions, flags, or
/// out-of-range fields. Unknown versions are never interpreted as current.
pub fn decode_and_migrate(input: &[u8]) -> Result<Settings, SettingsError> {
    if input.len() == 4 {
        return decode_v0(input);
    }
    if input.len() == V1_ENCODED_LEN {
        return decode_v1(input);
    }
    if input.len() == V2_ENCODED_LEN {
        return decode_v2(input);
    }
    if input.len() == V3_ENCODED_LEN {
        return decode_v3(input);
    }
    if input.len() == V4_ENCODED_LEN {
        return decode_v4(input);
    }
    if input.len() != ENCODED_LEN {
        return Err(SettingsError::InvalidLength(input.len()));
    }
    if input[..2] != MAGIC {
        return Err(SettingsError::InvalidMagic);
    }
    if input[2] != CURRENT_SCHEMA_VERSION {
        return Err(SettingsError::UnsupportedVersion(input[2]));
    }
    if input[7] & !0b111 != 0 {
        return Err(SettingsError::InvalidFlags(input[7]));
    }
    let mode = match input[8] {
        0 => WifiMode::AccessPoint,
        1 => WifiMode::Station,
        invalid => return Err(SettingsError::InvalidWifiMode(invalid)),
    };
    let ssid = decode_wifi_text(&input[10..42], input[9])?;
    let password = decode_wifi_text(&input[43..V2_ENCODED_LEN], input[42])?;
    let preset_skins = decode_preset_skins(&input[V2_ENCODED_LEN..V3_ENCODED_LEN])?;
    let mut foot_controller = FootControllerSettings::default();
    for (index, encoded) in input[V3_ENCODED_LEN..V3_ENCODED_LEN + FOOT_BINDING_COUNT]
        .iter()
        .copied()
        .enumerate()
    {
        foot_controller.global[index] =
            FootAction::decode(encoded).ok_or(SettingsError::InvalidFootAction(encoded))?;
        if foot_controller.global[index] == FootAction::Inherit {
            return Err(SettingsError::InvalidFootAction(encoded));
        }
    }
    for (action, encoded) in foot_controller.preset.iter_mut().flatten().zip(
        input[V3_ENCODED_LEN + FOOT_BINDING_COUNT..V4_ENCODED_LEN]
            .iter()
            .copied(),
    ) {
        *action = FootAction::decode(encoded).ok_or(SettingsError::InvalidFootAction(encoded))?;
    }
    let mut settings = make_settings(
        input[3],
        input[4],
        input[5],
        input[6],
        input[7] & 1 != 0,
        input[7] & 2 != 0,
        WifiSettings {
            enabled: input[7] & 4 != 0,
            mode,
            ssid,
            password,
        },
        preset_skins,
        &foot_controller,
    )?;
    let peer = &input[V4_ENCODED_LEN..];
    settings.midi.paired_peer = match peer[0] {
        0 => None,
        1 => Some(BluetoothPeer {
            address: peer[2..8]
                .try_into()
                .map_err(|_| SettingsError::InvalidLength(input.len()))?,
            address_type: peer[1],
            name: decode_wifi_text(&peer[9..], peer[8])?,
        }),
        invalid => return Err(SettingsError::InvalidFlags(invalid)),
    };
    settings.validate()
}

fn decode_v4(input: &[u8]) -> Result<Settings, SettingsError> {
    if input[..2] != MAGIC {
        return Err(SettingsError::InvalidMagic);
    }
    if input[2] != 4 {
        return Err(SettingsError::UnsupportedVersion(input[2]));
    }
    let mut migrated = [0_u8; ENCODED_LEN];
    migrated[..V4_ENCODED_LEN].copy_from_slice(input);
    migrated[2] = CURRENT_SCHEMA_VERSION;
    migrated[V4_ENCODED_LEN] = 0;
    decode_and_migrate(&migrated)
}

fn decode_preset_skins(input: &[u8]) -> Result<PresetSkins, SettingsError> {
    let mut preset_skins = PresetSkins::default();
    for (index, encoded) in input.iter().copied().enumerate() {
        let selection =
            SkinSelection::decode(encoded).ok_or(SettingsError::InvalidSkinSelection(encoded))?;
        let preset = PresetIndex::new(
            u8::try_from(index).map_err(|_| SettingsError::InvalidSkinSelection(encoded))?,
        )
        .map_err(|_| SettingsError::InvalidSkinSelection(encoded))?;
        preset_skins.set(preset, selection);
    }
    Ok(preset_skins)
}

fn decode_v3(input: &[u8]) -> Result<Settings, SettingsError> {
    if input[..2] != MAGIC {
        return Err(SettingsError::InvalidMagic);
    }
    if input[2] != 3 {
        return Err(SettingsError::UnsupportedVersion(input[2]));
    }
    if input[7] & !0b111 != 0 {
        return Err(SettingsError::InvalidFlags(input[7]));
    }
    let mode = match input[8] {
        0 => WifiMode::AccessPoint,
        1 => WifiMode::Station,
        invalid => return Err(SettingsError::InvalidWifiMode(invalid)),
    };
    make_settings(
        input[3],
        input[4],
        input[5],
        input[6],
        input[7] & 1 != 0,
        // Schema v3's BLE flag controlled the retired peripheral path. The
        // v4 foot-controller feature is a BLE Central, so enable it once on
        // migration instead of silently inheriting an unrelated off value.
        true,
        WifiSettings {
            enabled: input[7] & 4 != 0,
            mode,
            ssid: decode_wifi_text(&input[10..42], input[9])?,
            password: decode_wifi_text(&input[43..V2_ENCODED_LEN], input[42])?,
        },
        decode_preset_skins(&input[V2_ENCODED_LEN..])?,
        &FootControllerSettings::default(),
    )
}

fn decode_v0(input: &[u8]) -> Result<Settings, SettingsError> {
    make_settings(
        input[0],
        input[1],
        input[2],
        input[3],
        false,
        false,
        WifiSettings::default(),
        PresetSkins::default(),
        &FootControllerSettings::default(),
    )
}

fn decode_v1(input: &[u8]) -> Result<Settings, SettingsError> {
    if input[..2] != MAGIC {
        return Err(SettingsError::InvalidMagic);
    }
    if input[2] != 1 {
        return Err(SettingsError::UnsupportedVersion(input[2]));
    }
    if input[7] & !0b11 != 0 {
        return Err(SettingsError::InvalidFlags(input[7]));
    }
    make_settings(
        input[3],
        input[4],
        input[5],
        input[6],
        input[7] & 1 != 0,
        input[7] & 2 != 0,
        WifiSettings::default(),
        PresetSkins::default(),
        &FootControllerSettings::default(),
    )
}

fn decode_v2(input: &[u8]) -> Result<Settings, SettingsError> {
    if input[..2] != MAGIC {
        return Err(SettingsError::InvalidMagic);
    }
    if input[2] != 2 {
        return Err(SettingsError::UnsupportedVersion(input[2]));
    }
    if input[7] & !0b111 != 0 {
        return Err(SettingsError::InvalidFlags(input[7]));
    }
    let mode = match input[8] {
        0 => WifiMode::AccessPoint,
        1 => WifiMode::Station,
        invalid => return Err(SettingsError::InvalidWifiMode(invalid)),
    };
    let ssid = decode_wifi_text(&input[10..42], input[9])?;
    let password = decode_wifi_text(&input[43..], input[42])?;
    make_settings(
        input[3],
        input[4],
        input[5],
        input[6],
        input[7] & 1 != 0,
        input[7] & 2 != 0,
        WifiSettings {
            enabled: input[7] & 4 != 0,
            mode,
            ssid,
            password,
        },
        PresetSkins::default(),
        &FootControllerSettings::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn make_settings(
    preset: u8,
    slot: u8,
    brightness: u8,
    midi_channel: u8,
    serial_enabled: bool,
    ble_enabled: bool,
    wifi: WifiSettings,
    preset_skins: PresetSkins,
    foot_controller: &FootControllerSettings,
) -> Result<Settings, SettingsError> {
    let selected_preset =
        PresetIndex::new(preset).map_err(|_| SettingsError::InvalidPreset(preset))?;
    let selected_slot = match slot {
        0 => Slot::A,
        1 => Slot::B,
        2 => Slot::C,
        invalid => return Err(SettingsError::InvalidSlot(invalid)),
    };
    Settings {
        selected_preset,
        selected_slot,
        display_brightness_percent: brightness,
        midi: MidiSettings {
            channel: midi_channel,
            serial_enabled,
            ble_enabled,
            paired_peer: None,
        },
        wifi,
        preset_skins,
        foot_controller: *foot_controller,
    }
    .validate()
}

fn decode_wifi_text<const CAPACITY: usize>(
    storage: &[u8],
    length: u8,
) -> Result<WifiText<CAPACITY>, SettingsError> {
    let length = usize::from(length);
    if length == 0 || length > CAPACITY || storage.len() != CAPACITY {
        return Err(SettingsError::InvalidWifiTextLength(length));
    }
    let text =
        core::str::from_utf8(&storage[..length]).map_err(|_| SettingsError::InvalidWifiText)?;
    WifiText::new(text)
}

const fn slot_to_byte(slot: Slot) -> u8 {
    match slot {
        Slot::A => 0,
        Slot::B => 1,
        Slot::C => 2,
    }
}

const fn wifi_mode_to_byte(mode: WifiMode) -> u8 {
    match mode {
        WifiMode::AccessPoint => 0,
        WifiMode::Station => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_and_round_trip() {
        let settings = Settings::default().validate().expect("defaults are valid");
        assert_eq!(decode_and_migrate(&settings.encode()), Ok(settings));
    }

    #[test]
    fn paired_bluetooth_peer_round_trips_and_v4_migrates_unpaired() {
        let mut settings = Settings::default();
        settings.midi.paired_peer = Some(BluetoothPeer {
            address: [0x10, 0x20, 0x30, 0x40, 0x50, 0x60],
            address_type: 1,
            name: WifiText::new("M-VAVE Chocolate").expect("valid Bluetooth name"),
        });
        assert_eq!(decode_and_migrate(&settings.encode()), Ok(settings));

        let current = Settings::default().encode();
        let mut legacy = [0_u8; V4_ENCODED_LEN];
        legacy.copy_from_slice(&current[..V4_ENCODED_LEN]);
        legacy[2] = 4;
        let migrated = decode_and_migrate(&legacy).expect("v4 settings migrate");
        assert_eq!(migrated.midi.paired_peer, None);
        assert_eq!(migrated.midi.channel, 1);
    }

    #[test]
    fn version_zero_migrates_with_disabled_optional_transports() {
        let migrated = decode_and_migrate(&[19, 2, 75, 16]).expect("v0 is valid");
        assert_eq!(migrated.selected_preset.get(), 19);
        assert_eq!(migrated.selected_slot, Slot::C);
        assert_eq!(migrated.display_brightness_percent, 75);
        assert_eq!(migrated.midi.channel, 16);
        assert!(!migrated.midi.serial_enabled);
        assert!(!migrated.midi.ble_enabled);
    }

    #[test]
    fn corrupt_and_future_values_are_rejected() {
        assert_eq!(
            decode_and_migrate(&[0; 3]),
            Err(SettingsError::InvalidLength(3))
        );
        let mut encoded = Settings::default().encode();
        encoded[2] = CURRENT_SCHEMA_VERSION + 1;
        assert_eq!(
            decode_and_migrate(&encoded),
            Err(SettingsError::UnsupportedVersion(
                CURRENT_SCHEMA_VERSION + 1
            ))
        );
        encoded = Settings::default().encode();
        encoded[7] = 0x80;
        assert_eq!(
            decode_and_migrate(&encoded),
            Err(SettingsError::InvalidFlags(0x80))
        );
    }

    #[test]
    fn every_persisted_range_is_validated() {
        assert_eq!(
            decode_and_migrate(&[20, 0, 100, 1]),
            Err(SettingsError::InvalidPreset(20))
        );
        assert_eq!(
            decode_and_migrate(&[0, 3, 100, 1]),
            Err(SettingsError::InvalidSlot(3))
        );
        assert_eq!(
            decode_and_migrate(&[0, 0, 101, 1]),
            Err(SettingsError::InvalidBrightness(101))
        );
        assert_eq!(
            decode_and_migrate(&[0, 0, 100, 17]),
            Err(SettingsError::InvalidMidiChannel(17))
        );
    }

    #[test]
    fn version_one_migrates_to_valid_wifi_defaults() {
        let legacy = [b'T', b'X', 1, 7, 1, 80, 4, 0b11];
        let migrated = decode_and_migrate(&legacy).expect("v1 is valid");
        assert_eq!(migrated.selected_preset.get(), 7);
        assert!(migrated.wifi.enabled);
        assert_eq!(migrated.wifi.mode, WifiMode::AccessPoint);
        assert_eq!(migrated.wifi.ssid.as_str(), "TONEX-ONE");
        assert_eq!(decode_and_migrate(&migrated.encode()), Ok(migrated));
    }

    #[test]
    fn version_two_migrates_with_automatic_skins() {
        let current = Settings::default().encode();
        let mut legacy = [0_u8; V2_ENCODED_LEN];
        legacy.copy_from_slice(&current[..V2_ENCODED_LEN]);
        legacy[2] = 2;
        let migrated = decode_and_migrate(&legacy).expect("v2 is valid");
        assert!(
            migrated
                .preset_skins
                .selections()
                .iter()
                .all(|selection| *selection == SkinSelection::Auto)
        );
    }

    #[test]
    fn persisted_skin_selection_is_validated() {
        let mut settings = Settings::default();
        settings.preset_skins.set(
            PresetIndex::new(3).expect("preset 3 exists"),
            SkinSelection::Specific(tonex_skins::SkinId::JCM),
        );
        let encoded = settings.encode();
        assert_eq!(decode_and_migrate(&encoded), Ok(settings));

        let mut invalid = encoded;
        invalid[V2_ENCODED_LEN + 3] = 50;
        assert_eq!(
            decode_and_migrate(&invalid),
            Err(SettingsError::InvalidSkinSelection(50))
        );
    }

    #[test]
    fn wifi_text_and_persisted_lengths_are_validated() {
        assert_eq!(
            WifiText::<WIFI_SSID_CAPACITY>::new(""),
            Err(SettingsError::InvalidWifiTextLength(0))
        );
        assert_eq!(
            WifiText::<WIFI_SSID_CAPACITY>::new("bad\nssid"),
            Err(SettingsError::InvalidWifiText)
        );
        let mut encoded = Settings::default().encode();
        encoded[9] = 33;
        assert_eq!(
            decode_and_migrate(&encoded),
            Err(SettingsError::InvalidWifiTextLength(33))
        );
    }

    #[test]
    fn foot_bindings_use_preset_override_then_global_fallback() {
        let mut settings = Settings::default();
        let preset = PresetIndex::new(7).expect("preset exists");
        assert_eq!(
            settings
                .foot_controller
                .resolve(preset, FootButton::Three, FootGesture::ShortPress),
            FootAction::SelectSlot(Slot::C)
        );
        settings.foot_controller.set_preset_override(
            preset,
            FootButton::Three,
            FootGesture::ShortPress,
            FootAction::ToggleEffect(tonex_domain::EffectBlock::Modulation),
        );
        assert_eq!(
            settings
                .foot_controller
                .resolve(preset, FootButton::Three, FootGesture::ShortPress),
            FootAction::ToggleEffect(tonex_domain::EffectBlock::Modulation)
        );
        assert_eq!(decode_and_migrate(&settings.encode()), Ok(settings));
    }

    #[test]
    fn version_three_migrates_with_default_foot_bindings() {
        let current = Settings::default().encode();
        let mut legacy = [0_u8; V3_ENCODED_LEN];
        legacy.copy_from_slice(&current[..V3_ENCODED_LEN]);
        legacy[2] = 3;
        legacy[7] &= !0b10;
        let migrated = decode_and_migrate(&legacy).expect("v3 is valid");
        assert!(migrated.midi.ble_enabled);
        assert_eq!(
            migrated
                .foot_controller
                .global(FootButton::Four, FootGesture::ShortPress),
            FootAction::TapTempo
        );
    }

    #[test]
    fn corrupt_foot_binding_is_rejected() {
        let mut encoded = Settings::default().encode();
        encoded[V3_ENCODED_LEN] = 200;
        assert_eq!(
            decode_and_migrate(&encoded),
            Err(SettingsError::InvalidFootAction(200))
        );
    }
}
