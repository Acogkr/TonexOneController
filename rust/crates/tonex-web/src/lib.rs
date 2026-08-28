#![no_std]

use core::fmt::{self, Write};
use tonex_application::AppCommand;
use tonex_domain::{ConnectionState, FootAction, FootButton, FootGesture, PresetIndex, Slot};
use tonex_parameters::{
    ParameterId, ParameterKind, STORED_PARAMETER_COUNT, TONEX_PARAM_COMP_ENABLE,
    TONEX_PARAM_DELAY_ENABLE, TONEX_PARAM_MODULATION_ENABLE, TONEX_PARAM_NOISE_GATE_ENABLE,
    TONEX_PARAM_REVERB_ENABLE, all_specs, validate,
};
use tonex_settings::{
    BLUETOOTH_NAME_CAPACITY, BluetoothPeer, FootControllerSettings, MidiSettings, WifiMode,
    WifiSettings, WifiText,
};
use tonex_skins::{SkinId, SkinMatchSource, SkinSelection};
use tonex_ui_model::{ToneProfile, UiSnapshot};

pub const INDEX_HTML: &str = include_str!("../assets/index.html");
pub const MAX_COMMAND_BYTES: usize = 128;
// Leave enough headroom for the full preset catalogue and a populated BLE scan.
// The buffer is stack-allocated only while a snapshot is encoded, so this avoids
// truncating Web state without retaining extra heap memory between requests.
pub const MAX_SNAPSHOT_BYTES: usize = 4_096;
pub const MAX_PARAMETER_SNAPSHOT_BYTES: usize = 16 * 1024;
pub const MAX_DNS_PACKET_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebParameterGroup {
    All,
    Gate,
    Compressor,
    Eq,
    Amp,
    Cabinet,
    Reverb,
    Modulation,
    Delay,
    Global,
}

impl WebParameterGroup {
    const fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Gate => "gate",
            Self::Compressor => "compressor",
            Self::Eq => "eq",
            Self::Amp => "amp",
            Self::Cabinet => "cabinet",
            Self::Reverb => "reverb",
            Self::Modulation => "modulation",
            Self::Delay => "delay",
            Self::Global => "global",
        }
    }

    const fn contains(self, id: u16) -> bool {
        match self {
            Self::All => true,
            Self::Gate => id <= 4,
            Self::Compressor => id >= 5 && id <= 9,
            Self::Eq => id >= 10 && id <= 17,
            Self::Amp => (id >= 18 && id <= 22) || (id >= 34 && id <= 35),
            Self::Cabinet => (id >= 23 && id <= 33) || id == 112,
            Self::Reverb => id >= 36 && id <= 62,
            Self::Modulation => id >= 63 && id <= 93,
            Self::Delay => id >= 94 && id <= 108,
            Self::Global => id >= 110 && id <= 116,
        }
    }
}

#[must_use]
pub fn parse_parameter_request(input: &[u8]) -> Option<WebParameterGroup> {
    match input {
        b"parameters" => Some(WebParameterGroup::All),
        b"parameters=gate" => Some(WebParameterGroup::Gate),
        b"parameters=compressor" => Some(WebParameterGroup::Compressor),
        b"parameters=eq" => Some(WebParameterGroup::Eq),
        b"parameters=amp" => Some(WebParameterGroup::Amp),
        b"parameters=cabinet" => Some(WebParameterGroup::Cabinet),
        b"parameters=reverb" => Some(WebParameterGroup::Reverb),
        b"parameters=modulation" => Some(WebParameterGroup::Modulation),
        b"parameters=delay" => Some(WebParameterGroup::Delay),
        b"parameters=global" => Some(WebParameterGroup::Global),
        _ => None,
    }
}

/// Builds a minimal authoritative DNS A response that redirects any single
/// hostname query to the controller's captive-portal address.
#[must_use]
pub fn build_captive_dns_response(
    query: &[u8],
    output: &mut [u8],
    address: [u8; 4],
) -> Option<usize> {
    if query.len() < 12
        || output.len() < query.len().saturating_add(16)
        || u16::from_be_bytes([query[4], query[5]]) != 1
    {
        return None;
    }
    let mut cursor = 12;
    loop {
        let label_len = usize::from(*query.get(cursor)?);
        cursor += 1;
        if label_len == 0 {
            break;
        }
        if label_len > 63 {
            return None;
        }
        cursor = cursor.checked_add(label_len)?;
        if cursor > query.len() {
            return None;
        }
    }
    let question_end = cursor.checked_add(4)?;
    if question_end > query.len() {
        return None;
    }
    output[..question_end].copy_from_slice(&query[..question_end]);
    output[2..4].copy_from_slice(&0x8180_u16.to_be_bytes());
    output[4..6].copy_from_slice(&1_u16.to_be_bytes());
    output[6..8].copy_from_slice(&1_u16.to_be_bytes());
    output[8..12].fill(0);
    let answer = [
        0xc0, 0x0c, // compressed name pointer
        0x00, 0x01, // A
        0x00, 0x01, // IN
        0x00, 0x00, 0x00, 0x3c, // 60 second TTL
        0x00, 0x04, address[0], address[1], address[2], address[3],
    ];
    let response_len = question_end.checked_add(answer.len())?;
    output[question_end..response_len].copy_from_slice(&answer);
    Some(response_len)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebCommandError {
    Empty,
    TooLong,
    InvalidUtf8,
    Unknown,
    MissingValue,
    InvalidNumber,
    InvalidVolume,
    InvalidPreset,
    InvalidSlot,
    InvalidSlotAssignment,
    InvalidFx,
    InvalidParameter,
    InvalidMidi,
    InvalidWifi,
    InvalidSkin,
    InvalidBrightness,
    InvalidFootBinding,
    InvalidBluetooth,
}

/// Converts one bounded WebSocket payload into a validated application command.
///
/// # Errors
///
/// Rejects oversized, malformed, unknown, and out-of-range inputs.
pub fn parse_command(input: &[u8], now_ms: u64) -> Result<AppCommand, WebCommandError> {
    if input.is_empty() {
        return Err(WebCommandError::Empty);
    }
    if input.len() > MAX_COMMAND_BYTES {
        return Err(WebCommandError::TooLong);
    }
    let input = core::str::from_utf8(input).map_err(|_| WebCommandError::InvalidUtf8)?;
    match input {
        "next" => Ok(AppCommand::NextPreset),
        "previous" => Ok(AppCommand::PreviousPreset),
        "slot=A" => Ok(AppCommand::SelectSlot(Slot::A)),
        "slot=B" => Ok(AppCommand::SelectSlot(Slot::B)),
        "slot=C" => Ok(AppCommand::SelectSlot(Slot::C)),
        "tap" => Ok(AppCommand::TapTempo(now_ms)),
        "bluetooth-scan" => Ok(AppCommand::StartBluetoothScan),
        "bluetooth-forget" => Ok(AppCommand::ForgetBluetooth),
        _ if input.starts_with("slot=") => Err(WebCommandError::InvalidSlot),
        _ if input.starts_with("assign=") => parse_slot_assignment(input),
        _ if input.starts_with("wifi\t") => parse_wifi(input),
        _ if input.starts_with("midi-channel=") => parse_midi_channel(input),
        _ if input.starts_with("bluetooth-pair\t") => parse_bluetooth_pair(input),
        _ if input.starts_with("foot\t") => parse_foot_binding(input),
        _ if input.starts_with("preset-volume=") => parse_preset_volume(input),
        _ if input.starts_with("volume=") => parse_volume(input),
        _ if input.starts_with("fx=") => parse_fx(input),
        _ if input.starts_with("param=") => parse_parameter(input),
        _ if input.starts_with("skin=") => parse_skin(input),
        _ if input.starts_with("brightness=") => parse_brightness(input),
        _ => parse_preset(input),
    }
}

fn parse_foot_binding(input: &str) -> Result<AppCommand, WebCommandError> {
    let mut fields = input.split('\t');
    if fields.next() != Some("foot") {
        return Err(WebCommandError::InvalidFootBinding);
    }
    let scope = fields.next().ok_or(WebCommandError::InvalidFootBinding)?;
    let preset = fields.next().ok_or(WebCommandError::InvalidFootBinding)?;
    let button = match fields.next() {
        Some("1") => FootButton::One,
        Some("2") => FootButton::Two,
        Some("3") => FootButton::Three,
        Some("4") => FootButton::Four,
        _ => return Err(WebCommandError::InvalidFootBinding),
    };
    let gesture = match fields.next() {
        Some("short") => FootGesture::ShortPress,
        Some("long") => FootGesture::LongPress,
        _ => return Err(WebCommandError::InvalidFootBinding),
    };
    let action = fields
        .next()
        .and_then(|value| value.parse::<u8>().ok())
        .and_then(FootAction::decode)
        .ok_or(WebCommandError::InvalidFootBinding)?;
    if fields.next().is_some() {
        return Err(WebCommandError::InvalidFootBinding);
    }
    match scope {
        "global" if preset == "-" && action != FootAction::Inherit => {
            Ok(AppCommand::SetGlobalFootBinding {
                button,
                gesture,
                action,
            })
        }
        "preset" => Ok(AppCommand::SetPresetFootBinding {
            preset: preset
                .parse::<u8>()
                .ok()
                .and_then(|value| PresetIndex::new(value).ok())
                .ok_or(WebCommandError::InvalidFootBinding)?,
            button,
            gesture,
            action,
        }),
        _ => Err(WebCommandError::InvalidFootBinding),
    }
}

fn parse_slot_assignment(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input
        .strip_prefix("assign=")
        .ok_or(WebCommandError::Unknown)?;
    let (slot, preset) = value
        .split_once(':')
        .ok_or(WebCommandError::InvalidSlotAssignment)?;
    let slot = match slot {
        "A" => Slot::A,
        "B" => Slot::B,
        "C" => Slot::C,
        _ => return Err(WebCommandError::InvalidSlotAssignment),
    };
    let preset = preset
        .parse::<u8>()
        .ok()
        .and_then(|value| PresetIndex::new(value).ok())
        .ok_or(WebCommandError::InvalidSlotAssignment)?;
    Ok(AppCommand::AssignPresetToSlot { preset, slot })
}

fn parse_preset_volume(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input
        .strip_prefix("preset-volume=")
        .and_then(|value| parse_u8(value.as_bytes()))
        .filter(|value| *value <= 100)
        .ok_or(WebCommandError::InvalidVolume)?;
    Ok(AppCommand::SetPresetVolumeTenths(value))
}

fn parse_brightness(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input
        .strip_prefix("brightness=")
        .and_then(|value| parse_u8(value.as_bytes()))
        .filter(|value| *value <= 100)
        .ok_or(WebCommandError::InvalidBrightness)?;
    Ok(AppCommand::UpdateBrightness(value))
}

fn parse_skin(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input
        .strip_prefix("skin=")
        .ok_or(WebCommandError::Unknown)?;
    let (preset, selection) = value.split_once(':').ok_or(WebCommandError::InvalidSkin)?;
    let preset = parse_u8(preset.as_bytes()).ok_or(WebCommandError::InvalidSkin)?;
    let preset = PresetIndex::new(preset).map_err(|_| WebCommandError::InvalidSkin)?;
    let selection = if selection == "auto" {
        SkinSelection::Auto
    } else {
        let id = parse_u8(selection.as_bytes()).ok_or(WebCommandError::InvalidSkin)?;
        SkinSelection::Specific(SkinId::new(id).ok_or(WebCommandError::InvalidSkin)?)
    };
    Ok(AppCommand::SetPresetSkin { preset, selection })
}

fn parse_parameter(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input
        .strip_prefix("param=")
        .ok_or(WebCommandError::Unknown)?;
    let (id, value) = value
        .split_once(':')
        .ok_or(WebCommandError::InvalidParameter)?;
    let id = id
        .parse::<u16>()
        .map_err(|_| WebCommandError::InvalidParameter)?;
    let id = ParameterId::new(id).map_err(|_| WebCommandError::InvalidParameter)?;
    let value = value
        .parse::<f32>()
        .map_err(|_| WebCommandError::InvalidParameter)?;
    validate(id, value).map_err(|_| WebCommandError::InvalidParameter)?;
    Ok(AppCommand::SetParameter { id, value })
}

fn parse_fx(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input.strip_prefix("fx=").ok_or(WebCommandError::Unknown)?;
    let (name, enabled) = value.split_once(':').ok_or(WebCommandError::InvalidFx)?;
    let id = match name {
        "gate" => TONEX_PARAM_NOISE_GATE_ENABLE,
        "compressor" => TONEX_PARAM_COMP_ENABLE,
        "modulation" => TONEX_PARAM_MODULATION_ENABLE,
        "delay" => TONEX_PARAM_DELAY_ENABLE,
        "reverb" => TONEX_PARAM_REVERB_ENABLE,
        _ => return Err(WebCommandError::InvalidFx),
    };
    let value = match enabled {
        "0" => 0.0,
        "1" => 1.0,
        _ => return Err(WebCommandError::InvalidFx),
    };
    Ok(AppCommand::SetParameter { id, value })
}

fn parse_midi_channel(input: &str) -> Result<AppCommand, WebCommandError> {
    let channel = input
        .strip_prefix("midi-channel=")
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|channel| (1..=16).contains(channel))
        .ok_or(WebCommandError::InvalidMidi)?;
    Ok(AppCommand::UpdateMidiChannel(channel))
}

fn parse_bluetooth_pair(input: &str) -> Result<AppCommand, WebCommandError> {
    let mut fields = input.split('\t');
    if fields.next() != Some("bluetooth-pair") {
        return Err(WebCommandError::InvalidBluetooth);
    }
    let address_type = fields
        .next()
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| *value <= 1)
        .ok_or(WebCommandError::InvalidBluetooth)?;
    let address = fields
        .next()
        .and_then(parse_bluetooth_address)
        .ok_or(WebCommandError::InvalidBluetooth)?;
    let name = fields
        .next()
        .and_then(|name| WifiText::<BLUETOOTH_NAME_CAPACITY>::new(name).ok())
        .ok_or(WebCommandError::InvalidBluetooth)?;
    if fields.next().is_some() {
        return Err(WebCommandError::InvalidBluetooth);
    }
    Ok(AppCommand::PairBluetooth(BluetoothPeer {
        address,
        address_type,
        name,
    }))
}

fn parse_bluetooth_address(value: &str) -> Option<[u8; 6]> {
    if value.len() != 12 {
        return None;
    }
    let bytes = value.as_bytes();
    let mut address = [0_u8; 6];
    for (index, output) in address.iter_mut().enumerate() {
        let offset = index * 2;
        *output = hex_nibble(bytes[offset])?.checked_mul(16)? + hex_nibble(bytes[offset + 1])?;
    }
    Some(address)
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn parse_volume(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input
        .strip_prefix("volume=")
        .ok_or(WebCommandError::Unknown)?;
    let tenths = value
        .parse::<i16>()
        .map_err(|_| WebCommandError::InvalidNumber)?;
    if !(-400..=30).contains(&tenths) {
        return Err(WebCommandError::InvalidVolume);
    }
    Ok(AppCommand::SetMasterVolumeTenths(tenths))
}

fn parse_wifi(input: &str) -> Result<AppCommand, WebCommandError> {
    let mut fields = input.split('\t');
    if fields.next() != Some("wifi") {
        return Err(WebCommandError::InvalidWifi);
    }
    let mode = match fields.next() {
        Some("ap") => WifiMode::AccessPoint,
        Some("station") => WifiMode::Station,
        _ => return Err(WebCommandError::InvalidWifi),
    };
    let ssid = fields.next().ok_or(WebCommandError::InvalidWifi)?;
    let password = fields.next().ok_or(WebCommandError::InvalidWifi)?;
    if fields.next().is_some() {
        return Err(WebCommandError::InvalidWifi);
    }
    let settings = WifiSettings {
        enabled: true,
        mode,
        ssid: WifiText::new(ssid).map_err(|_| WebCommandError::InvalidWifi)?,
        password: WifiText::new(password).map_err(|_| WebCommandError::InvalidWifi)?,
    };
    settings
        .validate()
        .map_err(|_| WebCommandError::InvalidWifi)?;
    Ok(AppCommand::UpdateWifi(settings))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotError {
    BufferTooSmall,
}

/// Encodes the browser snapshot into caller-owned storage without allocation.
///
/// # Errors
///
/// Returns [`SnapshotError::BufferTooSmall`] when the destination cannot hold
/// the complete JSON document.
pub fn encode_snapshot<'a>(
    snapshot: &UiSnapshot,
    destination: &'a mut [u8],
) -> Result<&'a str, SnapshotError> {
    encode_snapshot_with_midi(snapshot, None, destination)
}

/// Encodes a browser snapshot plus non-secret MIDI settings without allocation.
///
/// # Errors
///
/// Returns [`SnapshotError::BufferTooSmall`] when the destination cannot hold
/// the complete JSON document.
pub fn encode_snapshot_with_midi<'a>(
    snapshot: &UiSnapshot,
    midi: Option<MidiSettings>,
    destination: &'a mut [u8],
) -> Result<&'a str, SnapshotError> {
    encode_snapshot_with_controller_settings(snapshot, midi, None, destination)
}

/// Encodes the live browser snapshot and non-secret controller mappings.
///
/// # Errors
///
/// Returns [`SnapshotError::BufferTooSmall`] when `destination` cannot hold
/// the complete JSON document.
pub fn encode_snapshot_with_controller_settings<'a>(
    snapshot: &UiSnapshot,
    midi: Option<MidiSettings>,
    foot: Option<&FootControllerSettings>,
    destination: &'a mut [u8],
) -> Result<&'a str, SnapshotError> {
    let mut writer = SliceWriter::new(destination);
    let connection = match snapshot.connection {
        ConnectionState::Disconnected => "disconnected",
        ConnectionState::Connecting => "connecting",
        ConnectionState::Synchronizing => "synchronizing",
        ConnectionState::Ready => "ready",
        ConnectionState::Faulted => "faulted",
    };
    let bluetooth = match snapshot.bluetooth {
        tonex_ui_model::BluetoothState::Off => "off",
        tonex_ui_model::BluetoothState::Searching => "searching",
        tonex_ui_model::BluetoothState::Connected => "connected",
    };
    let slot = match snapshot.slot {
        Slot::A => 'A',
        Slot::B => 'B',
        Slot::C => 'C',
    };
    let profile = ToneProfile::infer(snapshot.preset_label, snapshot.model_gain);
    write!(
        writer,
        "{{\"connection\":\"{connection}\",\"bluetooth\":\"{bluetooth}\",\"number\":{},\"name\":\"",
        snapshot.preset.get() + 1
    )
    .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_json_ascii(&mut writer, snapshot.preset_label.as_bytes())?;
    write!(
        writer,
        "\",\"slot\":\"{slot}\",\"slotPresets\":[{},{},{}],\"bpm\":{:.1},\"volume\":{:.1},\"modelGain\":{:.1},\"bypass\":{},\"muted\":{},\"brightness\":{},\"brightnessDimmable\":{},\"fx\":{{\"gate\":{},\"compressor\":{},\"amp\":{},\"cabinet\":{},\"modulation\":{},\"delay\":{},\"reverb\":{}}},\"profile\":\"{}\",\"profileLabel\":\"{}\",\"skin\":{},\"skinSource\":\"{}\"",
        snapshot.slot_presets[0].get(),
        snapshot.slot_presets[1].get(),
        snapshot.slot_presets[2].get(),
        snapshot.bpm,
        snapshot.preset_volume,
        snapshot.model_gain,
        snapshot.bypass,
        snapshot.mute.is_active(),
        snapshot.display_brightness_percent,
        snapshot.brightness_dimmable,
        snapshot.fx.gate,
        snapshot.fx.compressor,
        snapshot.fx.amp,
        snapshot.fx.cabinet,
        snapshot.fx.modulation,
        snapshot.fx.delay,
        snapshot.fx.reverb,
        profile_key(profile),
        profile.label(),
        snapshot.skin.id.get(),
        skin_source_key(snapshot.skin.source)
    )
    .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_optional_accent(&mut writer, snapshot.preset_color)?;
    write_midi_settings(&mut writer, midi)?;
    write_bluetooth_devices(&mut writer, snapshot)?;
    if let Some(foot) = foot {
        write_foot_settings(&mut writer, foot, snapshot.preset)?;
    }
    let wifi_mode = wifi_mode_key(snapshot.wifi.mode);
    write!(writer, ",\"wifiMode\":\"{wifi_mode}\",\"wifiSsid\":\"")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_json_ascii(&mut writer, snapshot.wifi.ssid.as_str().as_bytes())?;
    writer
        .write_char('"')
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_preset_labels(&mut writer, snapshot)?;
    writer
        .write_char('}')
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    writer.finish()
}

fn write_midi_settings(
    writer: &mut SliceWriter<'_>,
    midi: Option<MidiSettings>,
) -> Result<(), SnapshotError> {
    let Some(midi) = midi else {
        return Ok(());
    };
    write!(writer, ",\"midiChannel\":{}", midi.channel)
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    writer
        .write_str(",\"bluetoothPaired\":")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    let Some(peer) = midi.paired_peer else {
        return writer
            .write_str("null")
            .map_err(|_| SnapshotError::BufferTooSmall);
    };
    writer
        .write_str("{\"address\":\"")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_bluetooth_address(writer, peer.address)?;
    writer
        .write_str("\",\"name\":\"")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_json_ascii(writer, peer.name.as_str().as_bytes())?;
    writer
        .write_str("\"}")
        .map_err(|_| SnapshotError::BufferTooSmall)
}

fn write_bluetooth_devices(
    writer: &mut SliceWriter<'_>,
    snapshot: &UiSnapshot,
) -> Result<(), SnapshotError> {
    writer
        .write_str(",\"bluetoothDevices\":[")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    let mut first_device = true;
    for device in snapshot.bluetooth_devices.iter().flatten() {
        if !first_device {
            writer
                .write_char(',')
                .map_err(|_| SnapshotError::BufferTooSmall)?;
        }
        first_device = false;
        write!(writer, "{{\"type\":{},\"address\":\"", device.address_type)
            .map_err(|_| SnapshotError::BufferTooSmall)?;
        write_bluetooth_address(writer, device.address)?;
        writer
            .write_str("\",\"name\":\"")
            .map_err(|_| SnapshotError::BufferTooSmall)?;
        write_json_ascii(writer, device.name.as_bytes())?;
        write!(writer, "\",\"rssi\":{}}}", device.rssi)
            .map_err(|_| SnapshotError::BufferTooSmall)?;
    }
    writer
        .write_char(']')
        .map_err(|_| SnapshotError::BufferTooSmall)
}

fn write_foot_settings(
    writer: &mut SliceWriter<'_>,
    foot: &FootControllerSettings,
    preset: PresetIndex,
) -> Result<(), SnapshotError> {
    writer
        .write_str(",\"footGlobal\":[")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_foot_bindings(writer, |button, gesture| foot.global(button, gesture))?;
    writer
        .write_str("],\"footPreset\":[")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    write_foot_bindings(writer, |button, gesture| {
        foot.preset_override(preset, button, gesture)
    })?;
    writer
        .write_char(']')
        .map_err(|_| SnapshotError::BufferTooSmall)
}

fn write_foot_bindings<F>(writer: &mut SliceWriter<'_>, action: F) -> Result<(), SnapshotError>
where
    F: Fn(FootButton, FootGesture) -> FootAction,
{
    let buttons = [
        FootButton::One,
        FootButton::Two,
        FootButton::Three,
        FootButton::Four,
    ];
    let gestures = [FootGesture::ShortPress, FootGesture::LongPress];
    let mut first = true;
    for button in buttons {
        for gesture in gestures {
            if !first {
                writer
                    .write_char(',')
                    .map_err(|_| SnapshotError::BufferTooSmall)?;
            }
            first = false;
            write!(writer, "{}", action(button, gesture).encode())
                .map_err(|_| SnapshotError::BufferTooSmall)?;
        }
    }
    Ok(())
}

fn write_preset_labels(
    writer: &mut SliceWriter<'_>,
    snapshot: &UiSnapshot,
) -> Result<(), SnapshotError> {
    writer
        .write_str(",\"presets\":[")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    for (index, label) in snapshot.preset_labels.iter().enumerate() {
        if index != 0 {
            writer
                .write_char(',')
                .map_err(|_| SnapshotError::BufferTooSmall)?;
        }
        writer
            .write_char('"')
            .map_err(|_| SnapshotError::BufferTooSmall)?;
        write_json_ascii(writer, label.as_bytes())?;
        writer
            .write_char('"')
            .map_err(|_| SnapshotError::BufferTooSmall)?;
    }
    writer
        .write_char(']')
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    Ok(())
}

fn write_optional_accent(
    writer: &mut SliceWriter<'_>,
    color: Option<tonex_ui_model::ThemeColor>,
) -> Result<(), SnapshotError> {
    if let Some(color) = color {
        write!(writer, ",\"accent\":\"#{:06X}\"", color.hex())
            .map_err(|_| SnapshotError::BufferTooSmall)?;
    }
    Ok(())
}

const fn wifi_mode_key(mode: WifiMode) -> &'static str {
    match mode {
        WifiMode::AccessPoint => "ap",
        WifiMode::Station => "station",
    }
}

const fn skin_source_key(source: SkinMatchSource) -> &'static str {
    match source {
        SkinMatchSource::Manual => "manual",
        SkinMatchSource::ToneModelMetadata => "metadata",
        SkinMatchSource::PresetName => "preset",
        SkinMatchSource::Fallback => "fallback",
    }
}

fn write_json_ascii(writer: &mut SliceWriter<'_>, value: &[u8]) -> Result<(), SnapshotError> {
    for byte in value {
        match byte {
            b'"' => writer
                .write_str("\\\"")
                .map_err(|_| SnapshotError::BufferTooSmall)?,
            b'\\' => writer
                .write_str("\\\\")
                .map_err(|_| SnapshotError::BufferTooSmall)?,
            0x20..=0x7e => writer
                .write_char(char::from(*byte))
                .map_err(|_| SnapshotError::BufferTooSmall)?,
            _ => writer
                .write_char('?')
                .map_err(|_| SnapshotError::BufferTooSmall)?,
        }
    }
    Ok(())
}

fn write_bluetooth_address(
    writer: &mut SliceWriter<'_>,
    address: [u8; 6],
) -> Result<(), SnapshotError> {
    for byte in address {
        write!(writer, "{byte:02X}").map_err(|_| SnapshotError::BufferTooSmall)?;
    }
    Ok(())
}

/// Encodes every typed TONEX parameter for the browser editor.
///
/// # Errors
///
/// Returns [`SnapshotError::BufferTooSmall`] when the destination cannot hold
/// the complete parameter registry and current values.
pub fn encode_parameter_snapshot<'a>(
    values: &[f32; STORED_PARAMETER_COUNT],
    destination: &'a mut [u8],
) -> Result<&'a str, SnapshotError> {
    encode_parameter_group_snapshot(values, WebParameterGroup::All, destination)
}

/// Encodes only one editor group so constrained WebSocket clients do not need
/// to receive the complete parameter registry before opening an effect.
///
/// # Errors
///
/// Returns [`SnapshotError::BufferTooSmall`] when `destination` is too small.
pub fn encode_parameter_group_snapshot<'a>(
    values: &[f32; STORED_PARAMETER_COUNT],
    group: WebParameterGroup,
    destination: &'a mut [u8],
) -> Result<&'a str, SnapshotError> {
    let mut writer = SliceWriter::new(destination);
    write!(writer, "{{\"group\":\"{}\",\"parameters\":[", group.key())
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    let mut first = true;
    for (definition, value) in all_specs().iter().zip(values) {
        if !group.contains(definition.id.get()) {
            continue;
        }
        if !first {
            writer
                .write_char(',')
                .map_err(|_| SnapshotError::BufferTooSmall)?;
        }
        first = false;
        let kind = match definition.kind {
            ParameterKind::Switch => "switch",
            ParameterKind::Select => "select",
            ParameterKind::Range => "range",
        };
        write!(
            writer,
            "{{\"id\":{},\"label\":\"{}\",\"kind\":\"{}\",\"min\":{},\"max\":{},\"value\":{}}}",
            definition.id.get(),
            definition.label,
            kind,
            definition.minimum,
            definition.maximum,
            value
        )
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    }
    writer
        .write_str("]}")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    writer.finish()
}

/// Encodes only parameter values that changed since the last published cache.
/// The browser merges this bounded delta into whichever editor is open.
///
/// # Errors
///
/// Returns [`SnapshotError::BufferTooSmall`] when `destination` is too small.
pub fn encode_parameter_delta<'a>(
    previous: &[f32; STORED_PARAMETER_COUNT],
    current: &[f32; STORED_PARAMETER_COUNT],
    destination: &'a mut [u8],
) -> Result<&'a str, SnapshotError> {
    let mut writer = SliceWriter::new(destination);
    writer
        .write_str(r#"{"group":"delta","parameters":["#)
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    let mut first = true;
    for ((definition, before), value) in all_specs().iter().zip(previous).zip(current) {
        if before.to_bits() == value.to_bits() {
            continue;
        }
        if !first {
            writer
                .write_char(',')
                .map_err(|_| SnapshotError::BufferTooSmall)?;
        }
        first = false;
        let kind = match definition.kind {
            ParameterKind::Switch => "switch",
            ParameterKind::Select => "select",
            ParameterKind::Range => "range",
        };
        write!(
            writer,
            "{{\"id\":{},\"label\":\"{}\",\"kind\":\"{}\",\"min\":{},\"max\":{},\"value\":{}}}",
            definition.id.get(),
            definition.label,
            kind,
            definition.minimum,
            definition.maximum,
            value
        )
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    }
    writer
        .write_str("]}")
        .map_err(|_| SnapshotError::BufferTooSmall)?;
    writer.finish()
}

const fn profile_key(profile: ToneProfile) -> &'static str {
    match profile {
        ToneProfile::Clean => "clean",
        ToneProfile::British => "british",
        ToneProfile::American => "american",
        ToneProfile::Modern => "modern",
        ToneProfile::Boutique => "boutique",
        ToneProfile::Bass => "bass",
        ToneProfile::Acoustic => "acoustic",
        ToneProfile::Pedal => "pedal",
        ToneProfile::Neutral => "neutral",
    }
}

struct SliceWriter<'a> {
    destination: &'a mut [u8],
    len: usize,
}

impl<'a> SliceWriter<'a> {
    const fn new(destination: &'a mut [u8]) -> Self {
        Self {
            destination,
            len: 0,
        }
    }

    fn finish(self) -> Result<&'a str, SnapshotError> {
        core::str::from_utf8(&self.destination[..self.len])
            .map_err(|_| SnapshotError::BufferTooSmall)
    }
}

impl Write for SliceWriter<'_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        let output = self.destination.get_mut(self.len..end).ok_or(fmt::Error)?;
        output.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn parse_preset(input: &str) -> Result<AppCommand, WebCommandError> {
    let value = input
        .strip_prefix("preset=")
        .ok_or(WebCommandError::Unknown)?;
    if value.is_empty() {
        return Err(WebCommandError::MissingValue);
    }
    let value = parse_u8(value.as_bytes()).ok_or(WebCommandError::InvalidNumber)?;
    let preset = PresetIndex::new(value).map_err(|_| WebCommandError::InvalidPreset)?;
    Ok(AppCommand::SelectPreset(preset))
}

fn parse_u8(bytes: &[u8]) -> Option<u8> {
    let mut value = 0_u8;
    for byte in bytes {
        let digit = byte.checked_sub(b'0')?;
        if digit > 9 {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(digit)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_and_slots_are_typed_commands() {
        assert_eq!(parse_command(b"next", 123), Ok(AppCommand::NextPreset));
        assert_eq!(
            parse_command(b"previous", 123),
            Ok(AppCommand::PreviousPreset)
        );
        assert_eq!(
            parse_command(b"slot=C", 123),
            Ok(AppCommand::SelectSlot(Slot::C))
        );
        assert_eq!(
            parse_command(b"assign=B:12", 123),
            Ok(AppCommand::AssignPresetToSlot {
                preset: PresetIndex::new(12).expect("preset 12 exists"),
                slot: Slot::B,
            })
        );
        assert_eq!(
            parse_command(b"assign=D:12", 123),
            Err(WebCommandError::InvalidSlotAssignment)
        );
        assert_eq!(
            parse_command(b"assign=A:20", 123),
            Err(WebCommandError::InvalidSlotAssignment)
        );
        assert_eq!(parse_command(b"tap", 123), Ok(AppCommand::TapTempo(123)));
        assert_eq!(
            parse_command(b"fx=delay:1", 123),
            Ok(AppCommand::SetParameter {
                id: TONEX_PARAM_DELAY_ENABLE,
                value: 1.0,
            })
        );
        assert_eq!(
            parse_command(b"fx=unknown:1", 123),
            Err(WebCommandError::InvalidFx)
        );
        assert_eq!(
            parse_command(b"volume=-125", 123),
            Ok(AppCommand::SetMasterVolumeTenths(-125))
        );
    }

    #[test]
    fn skin_selection_supports_auto_and_validated_manual_overrides() {
        let preset = PresetIndex::new(7).expect("preset 7 exists");
        assert_eq!(
            parse_command(b"skin=7:auto", 0),
            Ok(AppCommand::SetPresetSkin {
                preset,
                selection: SkinSelection::Auto
            })
        );
        assert_eq!(
            parse_command(b"skin=7:20", 0),
            Ok(AppCommand::SetPresetSkin {
                preset,
                selection: SkinSelection::Specific(SkinId::AC30)
            })
        );
        assert_eq!(
            parse_command(b"skin=7:50", 0),
            Err(WebCommandError::InvalidSkin)
        );
        assert_eq!(
            parse_command(b"skin=20:auto", 0),
            Err(WebCommandError::InvalidSkin)
        );
    }

    #[test]
    fn brightness_is_bounded_and_typed() {
        assert_eq!(
            parse_command(b"brightness=70", 0),
            Ok(AppCommand::UpdateBrightness(70))
        );
        assert_eq!(
            parse_command(b"brightness=101", 0),
            Err(WebCommandError::InvalidBrightness)
        );
        assert_eq!(
            parse_command(b"brightness=no", 0),
            Err(WebCommandError::InvalidBrightness)
        );
    }

    #[test]
    fn preset_range_is_enforced_at_the_web_boundary() {
        assert_eq!(
            parse_command(b"preset=19", 123),
            Ok(AppCommand::SelectPreset(
                PresetIndex::new(19).expect("valid preset")
            ))
        );
        assert_eq!(
            parse_command(b"preset=20", 123),
            Err(WebCommandError::InvalidPreset)
        );
    }

    #[test]
    fn malformed_and_oversized_inputs_are_rejected() {
        assert_eq!(parse_command(b"", 123), Err(WebCommandError::Empty));
        assert_eq!(
            parse_command(b"preset=", 123),
            Err(WebCommandError::MissingValue)
        );
        assert_eq!(
            parse_command(b"preset=-1", 123),
            Err(WebCommandError::InvalidNumber)
        );
        assert_eq!(
            parse_command(&[b'x'; MAX_COMMAND_BYTES + 1], 123),
            Err(WebCommandError::TooLong)
        );
        assert_eq!(
            parse_command(b"volume=-401", 123),
            Err(WebCommandError::InvalidVolume)
        );
        assert_eq!(
            parse_command(b"volume=31", 123),
            Err(WebCommandError::InvalidVolume)
        );
        assert_eq!(
            parse_command(b"volume=quiet", 123),
            Err(WebCommandError::InvalidNumber)
        );
    }

    #[test]
    fn generic_parameter_writes_use_the_typed_registry() {
        assert_eq!(
            parse_command(b"preset-volume=75", 123),
            Ok(AppCommand::SetPresetVolumeTenths(75))
        );
        assert_eq!(
            parse_command(b"preset-volume=101", 123),
            Err(WebCommandError::InvalidVolume)
        );
        assert_eq!(
            parse_command(b"param=38:5", 123),
            Ok(AppCommand::SetParameter {
                id: ParameterId::new(38).expect("reverb model id"),
                value: 5.0,
            })
        );
        assert_eq!(
            parse_command(b"param=38:6", 123),
            Err(WebCommandError::InvalidParameter)
        );
        assert_eq!(
            parse_command(b"param=37:0.5", 123),
            Err(WebCommandError::InvalidParameter)
        );
        assert_eq!(
            parse_command(b"param=109:0", 123),
            Err(WebCommandError::InvalidParameter)
        );
    }

    #[test]
    fn every_legacy_parameter_is_writable_through_the_web_boundary() {
        for definition in all_specs() {
            let mut input = [0_u8; 64];
            let mut writer = SliceWriter::new(&mut input);
            write!(
                writer,
                "param={}:{}",
                definition.id.get(),
                definition.default
            )
            .expect("test command fits");
            let input = writer.finish().expect("test command is UTF-8");
            let parsed = parse_command(input.as_bytes(), 0)
                .expect("every generated C-reference parameter is a valid Web command");
            assert_eq!(
                parsed,
                AppCommand::SetParameter {
                    id: definition.id,
                    value: definition.default,
                },
                "Web command mapping changed for parameter {}",
                definition.id.get()
            );
        }
    }

    #[test]
    fn wifi_credentials_are_validated_and_remain_typed() {
        let command = parse_command(b"wifi\tstation\tStudioNet\tsecret123", 123)
            .expect("valid station settings");
        let AppCommand::UpdateWifi(wifi) = command else {
            panic!("expected Wi-Fi command");
        };
        assert_eq!(wifi.mode, WifiMode::Station);
        assert_eq!(wifi.ssid.as_str(), "StudioNet");
        assert_eq!(wifi.password.as_str(), "secret123");
        assert_eq!(
            parse_command(b"wifi\tap\tTONEX\tshort", 123),
            Err(WebCommandError::InvalidWifi)
        );
    }

    #[test]
    fn midi_settings_are_typed_and_range_checked() {
        assert_eq!(
            parse_command(b"midi-channel=16", 123),
            Ok(AppCommand::UpdateMidiChannel(16))
        );
        assert_eq!(
            parse_command(b"midi-channel=0", 123),
            Err(WebCommandError::InvalidMidi)
        );
        assert_eq!(
            parse_command(b"midi-channel=17", 123),
            Err(WebCommandError::InvalidMidi)
        );
        assert_eq!(
            parse_command(b"midi\t1\t1\t1", 123),
            Err(WebCommandError::Unknown)
        );

        let paired = parse_command(b"bluetooth-pair\t1\t102030405060\tM-VAVE Chocolate", 0)
            .expect("valid Bluetooth pairing command");
        let AppCommand::PairBluetooth(peer) = paired else {
            panic!("expected Bluetooth pairing command");
        };
        assert_eq!(peer.address, [0x10, 0x20, 0x30, 0x40, 0x50, 0x60]);
        assert_eq!(peer.address_type, 1);
        assert_eq!(peer.name.as_str(), "M-VAVE Chocolate");
        assert_eq!(
            parse_command(b"bluetooth-pair\t4\t102030405060\tBad", 0),
            Err(WebCommandError::InvalidBluetooth)
        );
    }

    #[test]
    fn foot_bindings_are_typed_for_global_and_preset_scopes() {
        assert_eq!(
            parse_command(b"foot\tglobal\t-\t3\tlong\t13", 0),
            Ok(AppCommand::SetGlobalFootBinding {
                button: FootButton::Three,
                gesture: FootGesture::LongPress,
                action: FootAction::ToggleEffect(tonex_domain::EffectBlock::Delay),
            })
        );
        assert_eq!(
            parse_command(b"foot\tpreset\t7\t1\tshort\t255", 0),
            Ok(AppCommand::SetPresetFootBinding {
                preset: PresetIndex::new(7).expect("preset exists"),
                button: FootButton::One,
                gesture: FootGesture::ShortPress,
                action: FootAction::Inherit,
            })
        );
        for invalid in [
            &b"foot\tglobal\t-\t5\tshort\t1"[..],
            &b"foot\tglobal\t-\t1\tshort\t255"[..],
            &b"foot\tpreset\t20\t1\tshort\t1"[..],
            &b"foot\tpreset\t1\t1\tdouble\t1"[..],
        ] {
            assert_eq!(
                parse_command(invalid, 0),
                Err(WebCommandError::InvalidFootBinding)
            );
        }
    }

    #[test]
    fn page_is_self_contained_and_has_no_external_runtime_dependencies() {
        assert!(INDEX_HTML.contains("TONEX ONE"));
        assert!(INDEX_HTML.contains("WebSocket"));
        assert!(INDEX_HTML.contains("aria-label=\"Signal chain\""));
        assert!(INDEX_HTML.contains(r#"id="presetList""#));
        assert!(INDEX_HTML.contains(r#"id="editOpen""#));
        assert!(INDEX_HTML.contains("data-fx=\"reverb\""));
        assert!(INDEX_HTML.contains("updateFx(state)"));
        assert!(INDEX_HTML.contains("preset-volume=${volumeSlider.value}"));
        assert!(!INDEX_HTML.contains("Serial MIDI"));
        assert!(INDEX_HTML.contains("Bluetooth device"));
        assert!(INDEX_HTML.contains("FOOT CONTROLLER"));
        assert!(INDEX_HTML.contains("foot\\t"));
        assert!(INDEX_HTML.contains("midi-channel="));
        assert!(INDEX_HTML.contains(r#"location.protocol==="https:"?"wss":"ws""#));
        assert!(INDEX_HTML.contains("Math.min(reconnectDelay*2,10000)"));
        assert!(INDEX_HTML.contains(r#"addEventListener("pagehide""#));
        assert!(INDEX_HTML.contains(r#"addEventListener("pageshow""#));
        assert!(INDEX_HTML.contains("socket.onclose=null;socket.close()"));
        assert!(INDEX_HTML.contains("socket?.readyState===WebSocket.CONNECTING"));
        assert!(INDEX_HTML.contains("document.hidden?2000:200"));
        assert!(INDEX_HTML.contains("event.data===lastSnapshotPayload"));
        assert!(INDEX_HTML.contains("event.data===lastParameterPayload"));
        assert!(INDEX_HTML.contains("signature===lastSlotsRender"));
        assert!(INDEX_HTML.contains("signature===lastBluetoothRender"));
        assert!(INDEX_HTML.contains("!pendingSnapshot"));
        assert!(INDEX_HTML.contains("!pendingParameters"));
        assert!(INDEX_HTML.contains("visibleForModel"));
        assert!(INDEX_HTML.contains("[hidden]{display:none!important}"));
        assert!(!INDEX_HTML.contains(r#"id="slotAssignments""#));
        assert!(INDEX_HTML.contains(r#"id="slotSwitcher""#));
        assert!(INDEX_HTML.contains(r#"id="quickAssignSelect""#));
        assert!(INDEX_HTML.contains("function updateQuickAssignment(state)"));
        assert!(INDEX_HTML.contains("label.textContent=`${String(index+1)"));
        assert!(!INDEX_HTML.contains("button.innerHTML=`<span>${String(index+1)"));
        assert!(INDEX_HTML.contains("assign=${state.slot}:${quickAssignSelect.value}"));
        assert!(INDEX_HTML.contains("slotButtons.forEach(button=>button.onclick"));
        assert!(INDEX_HTML.contains("lastParameterPayload=\"\";pendingParameters=true"));
        assert!(INDEX_HTML.contains("},900)"));
        assert!(INDEX_HTML.contains(r#"38:["Spring 1","Spring 2""#));
        assert!(INDEX_HTML.contains("++poll%3"));
        assert!(INDEX_HTML.contains("function requestSnapshot()"));
        assert!(INDEX_HTML.contains("function requestParameters(group=activeEditor)"));
        assert!(INDEX_HTML.contains("now-parameterRequestAt<1500"));
        assert!(INDEX_HTML.contains(r#"24:["Tone Model","VIR","Disabled"]"#));
        assert!(INDEX_HTML.contains("editorStatus.hidden=ready"));
        assert!(INDEX_HTML.contains(r#"state.group!=="all""#));
        assert!(INDEX_HTML.contains(r#"state.group==="delta""#));
        assert!(INDEX_HTML.contains("Object.assign(stored,update)"));
        assert!(INDEX_HTML.contains("if(state.accepted)"));
        assert!(INDEX_HTML.contains("setTimeout(requestSnapshot,40)"));
        assert!(INDEX_HTML.contains("setTimeout(()=>requestParameters(activeEditor),120)"));
        assert!(!INDEX_HTML.contains("socket.send(\"parameters\")"));
        assert!(!INDEX_HTML.contains(r#"enableControls(false,true);socket.send("snapshot")"#));
        assert!(INDEX_HTML.contains("JSON.parse"));
        assert!(INDEX_HTML.contains(r#"aria-busy="true""#));
        assert!(INDEX_HTML.contains(r#"controller.setAttribute("aria-busy""#));
        assert!(INDEX_HTML.contains("presetOpen.disabled"));
        assert!(INDEX_HTML.contains("block.disabled=!deviceReady"));
        assert!(INDEX_HTML.contains("closeDeviceSheets"));
        assert!(INDEX_HTML.contains("function initializeRouteHistory()"));
        assert!(INDEX_HTML.contains("history.pushState({tonexRoute:route}"));
        assert!(INDEX_HTML.contains(r#"addEventListener("hashchange""#));
        assert!(INDEX_HTML.contains(r#"addEventListener("popstate""#));
        assert!(INDEX_HTML.contains(r#"id="choiceSheet""#));
        assert!(INDEX_HTML.contains("function enhanceSelect"));
        assert!(INDEX_HTML.contains("native-select"));
        assert!(INDEX_HTML.contains("scrollbar-width:none"));
        assert!(INDEX_HTML.contains("::-webkit-scrollbar"));
        assert!(INDEX_HTML.contains(r#"id="displaySettings" class="settings-group" hidden"#));
        assert!(!INDEX_HTML.contains("Tone model</div>"));
        assert!(!INDEX_HTML.contains("LIVE STATUS"));
        assert!(!INDEX_HTML.contains("jquery"));
        assert!(!INDEX_HTML.contains("https://"));
    }

    #[test]
    fn snapshot_json_is_bounded_and_escapes_device_text() {
        let snapshot = UiSnapshot {
            preset_label: tonex_ui_model::PresetLabel::from_bytes(b"A\"B\\C"),
            slot: Slot::B,
            slot_presets: [
                PresetIndex::new(2).expect("preset 2 exists"),
                PresetIndex::new(7).expect("preset 7 exists"),
                PresetIndex::new(12).expect("preset 12 exists"),
            ],
            ..UiSnapshot::default()
        };
        let mut output = [0_u8; MAX_SNAPSHOT_BYTES];
        let encoded = encode_snapshot(&snapshot, &mut output).expect("buffer is sufficient");
        assert!(encoded.contains("\"name\":\"A\\\"B\\\\C\""));
        assert!(encoded.contains("\"slot\":\"B\""));
        assert!(encoded.contains("\"slotPresets\":[2,7,12]"));
        assert!(encoded.contains("\"profile\":\"neutral\""));
        assert!(encoded.contains("\"modelGain\":5.0"));
        assert!(encoded.contains("\"bypass\":false"));
        assert!(encoded.contains("\"muted\":false"));
        assert!(encoded.contains("\"brightness\":100"));
        assert!(encoded.contains("\"brightnessDimmable\":false"));
        assert!(encoded.contains("\"presets\":["));
        assert!(encoded.contains(
            "\"fx\":{\"gate\":false,\"compressor\":false,\"amp\":true,\"cabinet\":true,\"modulation\":false,\"delay\":false,\"reverb\":false}"
        ));

        let encoded = encode_snapshot_with_midi(
            &snapshot,
            Some(MidiSettings {
                channel: 9,
                serial_enabled: false,
                ble_enabled: true,
                paired_peer: Some(BluetoothPeer {
                    address: [0x10, 0x20, 0x30, 0x40, 0x50, 0x60],
                    address_type: 1,
                    name: WifiText::new("Chocolate").expect("valid name"),
                }),
            }),
            &mut output,
        )
        .expect("MIDI snapshot fits");
        assert!(encoded.contains("\"midiChannel\":9"));
        assert!(
            encoded.contains(
                "\"bluetoothPaired\":{\"address\":\"102030405060\",\"name\":\"Chocolate\"}"
            )
        );
        assert!(encoded.contains("\"bluetoothDevices\":[]"));
        assert!(encoded.contains("\"wifiMode\":\"ap\""));
        assert!(encoded.contains("\"wifiSsid\":\"TONEX-ONE\""));

        let mut wifi_snapshot = snapshot;
        wifi_snapshot.wifi.mode = WifiMode::Station;
        wifi_snapshot.wifi.ssid = WifiText::new("A\"B\\C").expect("printable SSID");
        let encoded = encode_snapshot(&wifi_snapshot, &mut output).expect("Wi-Fi snapshot fits");
        assert!(encoded.contains("\"wifiMode\":\"station\""));
        assert!(encoded.contains("\"wifiSsid\":\"A\\\"B\\\\C\""));
        assert!(!encoded.contains(wifi_snapshot.wifi.password.as_str()));

        let mut short = [0_u8; 8];
        assert_eq!(
            encode_snapshot(&snapshot, &mut short),
            Err(SnapshotError::BufferTooSmall)
        );
    }

    #[test]
    fn snapshot_capacity_holds_twenty_maximum_length_preset_names() {
        let full = tonex_ui_model::PresetLabel::from_bytes(b"12345678901234567890123456789012");
        let device = tonex_ui_model::BluetoothDevice {
            address: [0xFF; 6],
            address_type: 1,
            name: full,
            rssi: -127,
        };
        let snapshot = UiSnapshot {
            preset_label: full,
            preset_labels: [full; tonex_domain::PRESET_COUNT],
            bluetooth_devices: [Some(device); 8],
            ..UiSnapshot::default()
        };
        let mut output = [0_u8; MAX_SNAPSHOT_BYTES];
        let encoded = encode_snapshot(&snapshot, &mut output).expect("capacity covers worst case");
        assert!(encoded.len() < MAX_SNAPSHOT_BYTES);
        assert_eq!(
            encoded.matches("12345678901234567890123456789012").count(),
            29
        );
        assert_eq!(encoded.matches("\"rssi\":-127").count(), 8);
    }

    #[test]
    fn parameter_snapshot_contains_every_registry_value_and_metadata() {
        let values = *tonex_parameters::ParameterStore::with_defaults().values();
        let mut output = [0_u8; MAX_PARAMETER_SNAPSHOT_BYTES];
        let encoded =
            encode_parameter_snapshot(&values, &mut output).expect("parameter snapshot fits");
        assert!(encoded.starts_with(r#"{"group":"all","parameters":[{"id":0,"#));
        assert!(encoded.contains(r#""id":38,"label":"RVB MODEL","kind":"select""#));
        assert!(encoded.contains(r#""id":116,"label":"MVOL","kind":"range""#));
        assert_eq!(encoded.matches(r#""id":"#).count(), STORED_PARAMETER_COUNT);

        let mut short = [0_u8; 32];
        assert_eq!(
            encode_parameter_snapshot(&values, &mut short),
            Err(SnapshotError::BufferTooSmall)
        );
    }

    #[test]
    fn parameter_groups_are_typed_bounded_and_do_not_leak_other_editors() {
        assert_eq!(
            parse_parameter_request(b"parameters=gate"),
            Some(WebParameterGroup::Gate)
        );
        assert_eq!(
            parse_parameter_request(b"parameters=amp"),
            Some(WebParameterGroup::Amp)
        );
        assert_eq!(parse_parameter_request(b"parameters=unknown"), None);

        let values = *tonex_parameters::ParameterStore::with_defaults().values();
        let mut output = [0_u8; MAX_PARAMETER_SNAPSHOT_BYTES];
        let gate = encode_parameter_group_snapshot(&values, WebParameterGroup::Gate, &mut output)
            .expect("gate parameter snapshot fits");
        assert!(gate.starts_with(r#"{"group":"gate","parameters":[{"id":0,"#));
        assert!(gate.contains(r#""id":4,"#));
        assert!(!gate.contains(r#""id":5,"#));
        assert_eq!(gate.matches(r#""id":"#).count(), 5);

        let amp = encode_parameter_group_snapshot(&values, WebParameterGroup::Amp, &mut output)
            .expect("amp parameter snapshot fits");
        assert!(amp.contains(r#""id":18,"#));
        assert!(amp.contains(r#""id":22,"#));
        assert!(amp.contains(r#""id":34,"#));
        assert!(amp.contains(r#""id":35,"#));
        assert!(!amp.contains(r#""id":23,"#));
        assert!(amp.len() < MAX_PARAMETER_SNAPSHOT_BYTES / 4);

        let cabinet =
            encode_parameter_group_snapshot(&values, WebParameterGroup::Cabinet, &mut output)
                .expect("cabinet parameter snapshot fits");
        assert!(cabinet.contains(r#""id":23,"#));
        assert!(cabinet.contains(r#""id":33,"#));
        assert!(cabinet.contains(r#""id":112,"#));

        for (group, expected) in [
            (WebParameterGroup::Gate, 5),
            (WebParameterGroup::Compressor, 5),
            (WebParameterGroup::Eq, 8),
            (WebParameterGroup::Amp, 7),
            (WebParameterGroup::Cabinet, 12),
            (WebParameterGroup::Reverb, 27),
            (WebParameterGroup::Modulation, 31),
            (WebParameterGroup::Delay, 15),
            (WebParameterGroup::Global, 7),
        ] {
            let encoded = encode_parameter_group_snapshot(&values, group, &mut output)
                .expect("every editor group fits the bounded Web response");
            assert_eq!(
                encoded.matches(r#""id":"#).count(),
                expected,
                "{group:?} must expose its complete parameter group"
            );
        }
    }

    #[test]
    fn parameter_delta_contains_only_values_that_changed() {
        let previous = *tonex_parameters::ParameterStore::with_defaults().values();
        let mut current = previous;
        current[usize::from(TONEX_PARAM_NOISE_GATE_ENABLE.get())] = 0.0;
        current[usize::from(TONEX_PARAM_DELAY_ENABLE.get())] = 1.0;
        let mut output = [0_u8; 1024];
        let encoded = encode_parameter_delta(&previous, &current, &mut output)
            .expect("two changed values fit the bounded delta");
        assert!(encoded.starts_with(r#"{"group":"delta","parameters":["#));
        assert_eq!(encoded.matches(r#""id":"#).count(), 2);
        assert!(encoded.contains(r#""id":1,"#));
        assert!(encoded.contains(r#""id":95,"#));
        assert!(!encoded.contains(r#""id":6,"#));

        let unchanged =
            encode_parameter_delta(&previous, &previous, &mut output).expect("empty delta fits");
        assert_eq!(unchanged, r#"{"group":"delta","parameters":[]}"#);
    }

    #[test]
    fn captive_dns_redirects_single_a_query_to_controller() {
        let query = [
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, b't',
            b'o', b'n', b'e', b'x', 0x03, b'o', b'n', b'e', 0x00, 0x00, 0x01, 0x00, 0x01,
        ];
        let mut output = [0_u8; MAX_DNS_PACKET_BYTES];
        let len = build_captive_dns_response(&query, &mut output, [192, 168, 4, 1])
            .expect("valid DNS query");
        assert_eq!(&output[..2], &[0x12, 0x34]);
        assert_eq!(&output[2..4], &[0x81, 0x80]);
        assert_eq!(&output[6..8], &[0x00, 0x01]);
        assert_eq!(&output[len - 4..len], &[192, 168, 4, 1]);
        assert_eq!(
            build_captive_dns_response(&query[..10], &mut output, [192, 168, 4, 1]),
            None
        );
    }
}
