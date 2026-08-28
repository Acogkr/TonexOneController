use hal_contracts::UsbTransport;
use tonex_domain::{PRESET_COUNT, PresetIndex, TapTempo};
use tonex_parameters::{
    ParameterError, ParameterId, ParameterStore, TONEX_GLOBAL_BPM, TONEX_GLOBAL_BYPASS,
    TONEX_GLOBAL_CABSIM_BYPASS, TONEX_GLOBAL_INPUT_TRIM, TONEX_GLOBAL_MASTER_VOLUME,
    TONEX_GLOBAL_TEMPO_SOURCE, TONEX_GLOBAL_TUNING_REFERENCE, TONEX_PARAM_CABINET_TYPE,
    TONEX_PARAM_COMP_ENABLE, TONEX_PARAM_DELAY_ENABLE, TONEX_PARAM_MODEL_AMP_ENABLE,
    TONEX_PARAM_MODEL_GAIN, TONEX_PARAM_MODEL_VOLUME, TONEX_PARAM_MODULATION_ENABLE,
    TONEX_PARAM_NOISE_GATE_ENABLE, TONEX_PARAM_REVERB_ENABLE,
};
use tonex_protocol::{
    FrameError, GlobalWriteError, Message, MessageError, MessageType, ParameterWriteError,
    PresetColor, PresetName, Session, SessionAction, SessionError, SessionState, StateDocument,
    StateError, StreamDecoder, encode_master_volume, encode_parameter_change,
    encode_preset_details_request, extract_preset_name, parse_master_volume,
    parse_parameter_change, parse_payload, parse_preset_colors, parse_preset_parameters,
    parse_state,
};
use tonex_settings::Settings;
use tonex_skins::resolve_skin;
use tonex_ui_model::{FxState, PerformanceMuteState, PresetLabel, ThemeColor, UiPage, UiSnapshot};

use crate::{AppCommand, AppController, AppSnapshot};

// A full TONEX preset-detail response is roughly 30 KiB. Keep enough room for
// the largest known frame plus escaping without rejecting the sync sentinel.
const MAX_ENCODED_FRAME: usize = 65_536;
const PERFORMANCE_MUTE_DB: f32 = -40.0;

#[derive(Debug, PartialEq)]
pub enum RuntimeError<E> {
    Transport(E),
    Frame(FrameError),
    Message(MessageError),
    State(StateError),
    Session(SessionError),
    Parameter(ParameterError),
    ParameterWrite(ParameterWriteError),
    NotReady(SessionState),
    MissingStateDocument,
    GlobalWrite(GlobalWriteError),
    UnsupportedCommand,
}

/// Headless TONEX ONE orchestration with one mutable application-state owner.
///
/// USB callbacks feed copied bytes to [`Self::ingest_usb`]. Framing, parsing,
/// synchronization, state mutation, and outgoing writes all occur on the
/// application task.
#[derive(Debug)]
pub struct TonexRuntime<T> {
    transport: T,
    controller: AppController,
    session: Session,
    decoder: StreamDecoder,
    preset_names: [Option<PresetName>; PRESET_COUNT],
    preset_colors: Option<[PresetColor; PRESET_COUNT]>,
    parameters: ParameterStore,
    state_document: Option<StateDocument>,
    loaded_parameter_preset: Option<PresetIndex>,
    committed_parameter_selection: Option<(PresetIndex, tonex_domain::Slot)>,
    requested_parameter_preset: Option<PresetIndex>,
    tap_tempo: TapTempo,
    mute_restore_master_volume: Option<f32>,
}

impl<T: UsbTransport> TonexRuntime<T> {
    #[must_use]
    pub fn new(transport: T) -> Self {
        Self::new_with_settings(transport, Settings::default())
    }

    #[must_use]
    pub fn new_with_settings(transport: T, settings: Settings) -> Self {
        Self {
            transport,
            controller: AppController::with_settings(settings),
            session: Session::default(),
            decoder: StreamDecoder::new(MAX_ENCODED_FRAME),
            preset_names: [None; PRESET_COUNT],
            preset_colors: None,
            parameters: ParameterStore::default(),
            state_document: None,
            loaded_parameter_preset: None,
            committed_parameter_selection: None,
            requested_parameter_preset: None,
            tap_tempo: TapTempo::default(),
            mute_restore_master_volume: None,
        }
    }

    #[must_use]
    pub const fn snapshot(&self) -> AppSnapshot {
        self.controller.snapshot()
    }

    #[must_use]
    pub fn preset_name(&self, preset: PresetIndex) -> Option<&PresetName> {
        self.preset_names[usize::from(preset.get())].as_ref()
    }

    #[must_use]
    pub const fn parameter_value(&self, id: ParameterId) -> f32 {
        self.parameters.get(id)
    }

    #[must_use]
    pub fn parameter_values(&self) -> [f32; tonex_parameters::STORED_PARAMETER_COUNT] {
        *self.parameters.values()
    }

    #[must_use]
    pub fn ui_snapshot(&self) -> UiSnapshot {
        let app = self.snapshot();
        // A TONEX selection state arrives before its complete preset parameter
        // block. Keep presenting the last internally consistent preset until
        // the new block is committed, so board and Web never combine a new
        // name/slot with the previous preset's FX state.
        let (displayed_preset, displayed_slot) = self
            .committed_parameter_selection
            .filter(|_| app.connection == tonex_domain::ConnectionState::Ready)
            .unwrap_or((app.selected_preset, app.selected_slot));
        let preset_label = self
            .preset_name(displayed_preset)
            .map_or_else(PresetLabel::default, |name| {
                PresetLabel::from_bytes(name.as_bytes())
            });
        let skin = resolve_skin(
            app.settings.preset_skins.get(displayed_preset),
            None,
            preset_label.as_bytes(),
            self.parameters.get(TONEX_PARAM_MODEL_GAIN),
        );
        let skin_selection = app.settings.preset_skins.get(displayed_preset);
        let preset_labels = core::array::from_fn(|index| {
            self.preset_names[index]
                .as_ref()
                .map_or(PresetLabel::EMPTY, |name| {
                    PresetLabel::from_bytes(name.as_bytes())
                })
        });
        let slot_presets = self
            .state_document
            .as_ref()
            .and_then(|document| document.snapshot().ok())
            .map_or([app.selected_preset; 3], |state| {
                [state.slot_a, state.slot_b, state.slot_c]
            });
        UiSnapshot {
            connection: app.connection,
            bluetooth: if app.settings.midi.paired_peer.is_some() {
                tonex_ui_model::BluetoothState::Searching
            } else {
                tonex_ui_model::BluetoothState::Off
            },
            bluetooth_devices: [None; 8],
            sync: app.sync,
            preset: displayed_preset,
            slot: displayed_slot,
            slot_presets,
            preset_label,
            preset_labels,
            bpm: self.parameters.get(TONEX_GLOBAL_BPM),
            preset_volume: self.parameters.get(TONEX_PARAM_MODEL_VOLUME),
            master_volume_db: self.parameters.get(TONEX_GLOBAL_MASTER_VOLUME),
            model_gain: self.parameters.get(TONEX_PARAM_MODEL_GAIN),
            preset_color: self
                .preset_colors
                .map(|colors| display_preset_color(colors[usize::from(displayed_preset.get())])),
            bypass: self.parameters.get(TONEX_GLOBAL_BYPASS) != 0.0,
            mute: if self.mute_restore_master_volume.is_some() {
                PerformanceMuteState::Active
            } else {
                PerformanceMuteState::Inactive
            },
            fx: FxState {
                gate: self.parameters.get(TONEX_PARAM_NOISE_GATE_ENABLE) != 0.0,
                compressor: self.parameters.get(TONEX_PARAM_COMP_ENABLE) != 0.0,
                amp: self.parameters.get(TONEX_PARAM_MODEL_AMP_ENABLE) != 0.0,
                cabinet: (self.parameters.get(TONEX_PARAM_CABINET_TYPE) - 2.0).abs() > f32::EPSILON
                    && self.parameters.get(TONEX_GLOBAL_CABSIM_BYPASS) == 0.0,
                modulation: self.parameters.get(TONEX_PARAM_MODULATION_ENABLE) != 0.0,
                delay: self.parameters.get(TONEX_PARAM_DELAY_ENABLE) != 0.0,
                reverb: self.parameters.get(TONEX_PARAM_REVERB_ENABLE) != 0.0,
            },
            page: UiPage::Stage,
            tuner: tonex_ui_model::TunerState {
                reference_hz: tuning_reference_hz(
                    self.parameters.get(TONEX_GLOBAL_TUNING_REFERENCE),
                ),
                ..tonex_ui_model::TunerState::default()
            },
            board_label: PresetLabel::from_bytes(b"Generic board"),
            display_width: 0,
            display_height: 0,
            touch_ready: false,
            brightness_dimmable: false,
            display_brightness_percent: app.settings.display_brightness_percent,
            wifi: app.settings.wifi,
            skin,
            skin_selection,
            parameters: *self.parameters.values(),
        }
    }

    /// Applies a user-originated command through the runtime's single state
    /// transition path.
    ///
    /// # Errors
    ///
    /// Returns protocol or transport errors, or rejects lifecycle commands
    /// that are reserved for device adapters.
    pub fn apply_command(
        &mut self,
        command: AppCommand,
    ) -> Result<crate::AppEffect, RuntimeError<T::Error>> {
        if let Some(effect) = self.apply_local_setting(command) {
            return Ok(effect);
        }
        if command.requires_tonex() {
            self.ensure_ready()?;
        }
        let snapshot = self.snapshot();
        let (preset, slot) = match command {
            AppCommand::SelectPreset(preset) => (preset, snapshot.selected_slot),
            AppCommand::SelectPresetInSlot { preset, slot } => (preset, slot),
            AppCommand::AssignPresetToSlot { preset, slot } => {
                self.assign_preset_to_slot(preset, slot)?;
                return Ok(crate::AppEffect::None);
            }
            AppCommand::NextPreset => (
                snapshot.selected_preset.next_wrapping(),
                snapshot.selected_slot,
            ),
            AppCommand::PreviousPreset => (
                snapshot.selected_preset.previous_wrapping(),
                snapshot.selected_slot,
            ),
            AppCommand::NextAbBank => {
                self.shift_ab_bank(true)?;
                return Ok(crate::AppEffect::None);
            }
            AppCommand::PreviousAbBank => {
                self.shift_ab_bank(false)?;
                return Ok(crate::AppEffect::None);
            }
            AppCommand::SelectSlot(slot) => {
                // Selecting A/B/C means recalling the preset already assigned
                // to that slot. Reusing the current preset here would silently
                // overwrite the target slot instead of selecting it.
                let preset = self
                    .state_document
                    .as_ref()
                    .and_then(|document| document.snapshot().ok())
                    .map_or(snapshot.selected_preset, |state| match slot {
                        tonex_domain::Slot::A => state.slot_a,
                        tonex_domain::Slot::B => state.slot_b,
                        tonex_domain::Slot::C => state.slot_c,
                    });
                (preset, slot)
            }
            AppCommand::TapTempo(now_ms) => {
                if let Some(bpm) = self.tap_tempo.tap(now_ms) {
                    self.write_parameter(TONEX_GLOBAL_BPM, bpm)?;
                }
                return Ok(crate::AppEffect::None);
            }
            AppCommand::SetPresetVolumeTenths(value) => {
                self.write_parameter(TONEX_PARAM_MODEL_VOLUME, f32::from(value) / 10.0)?;
                return Ok(crate::AppEffect::None);
            }
            AppCommand::SetMasterVolumeTenths(value) => {
                self.mute_restore_master_volume = None;
                self.write_master_volume(f32::from(value) / 10.0)?;
                return Ok(crate::AppEffect::None);
            }
            AppCommand::ToggleMute => {
                self.toggle_performance_mute()?;
                return Ok(crate::AppEffect::None);
            }
            AppCommand::SetParameter { id, value } => {
                self.write_parameter(id, value)?;
                return Ok(crate::AppEffect::None);
            }
            AppCommand::UpdateWifi(_)
            | AppCommand::UpdateMidi(_)
            | AppCommand::UpdateMidiChannel(_)
            | AppCommand::StartBluetoothScan
            | AppCommand::PairBluetooth(_)
            | AppCommand::ForgetBluetooth
            | AppCommand::SetGlobalFootBinding { .. }
            | AppCommand::SetPresetFootBinding { .. }
            | AppCommand::UpdateBrightness(_)
            | AppCommand::SetPresetSkin { .. } => unreachable!("handled as a local setting"),
            AppCommand::UsbConnected
            | AppCommand::UsbDisconnected
            | AppCommand::SynchronizationStarted
            | AppCommand::SynchronizationProgress(_)
            | AppCommand::SynchronizationCompleted
            | AppCommand::ObservedSelection { .. } => return Err(RuntimeError::UnsupportedCommand),
        };
        self.select_preset_in_slot(preset, slot)?;
        Ok(crate::AppEffect::None)
    }

    fn apply_local_setting(&mut self, command: AppCommand) -> Option<crate::AppEffect> {
        matches!(
            command,
            AppCommand::UpdateWifi(_)
                | AppCommand::UpdateMidi(_)
                | AppCommand::UpdateMidiChannel(_)
                | AppCommand::StartBluetoothScan
                | AppCommand::PairBluetooth(_)
                | AppCommand::ForgetBluetooth
                | AppCommand::SetGlobalFootBinding { .. }
                | AppCommand::SetPresetFootBinding { .. }
                | AppCommand::UpdateBrightness(_)
                | AppCommand::SetPresetSkin { .. }
        )
        .then(|| self.controller.handle(command))
    }

    /// Validates, transmits, and commits any TONEX ONE parameter change.
    ///
    /// # Errors
    ///
    /// Rejects writes before synchronization, invalid values, and
    /// transport failures. The cache changes only after transmission.
    pub fn write_parameter(
        &mut self,
        id: ParameterId,
        value: f32,
    ) -> Result<(), RuntimeError<T::Error>> {
        self.ensure_ready()?;
        if id == TONEX_GLOBAL_MASTER_VOLUME {
            return self.write_master_volume(value);
        }
        if id.is_global() {
            return self.write_state_global(id, value);
        }
        let frame = encode_parameter_change(id, value).map_err(RuntimeError::ParameterWrite)?;
        self.send(&frame)?;
        self.parameters
            .set(id, value)
            .map_err(RuntimeError::Parameter)?;
        Ok(())
    }

    /// Validates, transmits, and commits master volume in dB.
    ///
    /// # Errors
    ///
    /// Rejects writes before synchronization, invalid values, and transport
    /// failures.
    pub fn write_master_volume(&mut self, value_db: f32) -> Result<(), RuntimeError<T::Error>> {
        self.ensure_ready()?;
        let frame = encode_master_volume(value_db).map_err(RuntimeError::ParameterWrite)?;
        self.send(&frame)?;
        self.parameters
            .set(TONEX_GLOBAL_MASTER_VOLUME, value_db)
            .map_err(RuntimeError::Parameter)?;
        Ok(())
    }

    fn toggle_performance_mute(&mut self) -> Result<(), RuntimeError<T::Error>> {
        if let Some(restore) = self.mute_restore_master_volume.take() {
            if let Err(error) = self.write_master_volume(restore) {
                self.mute_restore_master_volume = Some(restore);
                return Err(error);
            }
        } else {
            let restore = self.parameters.get(TONEX_GLOBAL_MASTER_VOLUME);
            self.write_master_volume(PERFORMANCE_MUTE_DB)?;
            self.mute_restore_master_volume = Some(restore);
        }
        Ok(())
    }

    fn write_state_global(
        &mut self,
        id: ParameterId,
        value: f32,
    ) -> Result<(), RuntimeError<T::Error>> {
        let mut candidate = self
            .state_document
            .clone()
            .ok_or(RuntimeError::MissingStateDocument)?;
        candidate
            .set_global(id, value)
            .map_err(RuntimeError::GlobalWrite)?;
        let frame = candidate.encode_write().map_err(RuntimeError::State)?;
        self.send(&frame)?;
        self.state_document = Some(candidate);
        self.parameters
            .set(id, value)
            .map_err(RuntimeError::Parameter)?;
        Ok(())
    }

    /// Assigns a preset to a slot using the pedal's last complete state.
    ///
    /// # Errors
    ///
    /// Rejects calls before synchronization or before a state was received,
    /// and propagates state encoding and transport failures.
    pub fn select_preset_in_slot(
        &mut self,
        preset: PresetIndex,
        slot: tonex_domain::Slot,
    ) -> Result<(), RuntimeError<T::Error>> {
        self.ensure_ready()?;
        let mut candidate = self
            .state_document
            .clone()
            .ok_or(RuntimeError::MissingStateDocument)?;
        candidate.assign_preset(preset, slot, true);
        let frame = candidate.encode_write().map_err(RuntimeError::State)?;
        self.send(&frame)?;
        self.state_document = Some(candidate);
        let _ = self
            .controller
            .handle(AppCommand::ObservedSelection { preset, slot });
        // Do not wait for the pedal's later state notification before asking
        // for the selected preset. USB preserves command ordering, and the
        // proven 0x0304 request can follow the state write immediately. This
        // removes a full notification round-trip from board/Web navigation.
        self.loaded_parameter_preset = None;
        let details = encode_preset_details_request(preset.get()).map_err(RuntimeError::Message)?;
        self.send(&details)?;
        self.requested_parameter_preset = Some(preset);
        Ok(())
    }

    fn assign_preset_to_slot(
        &mut self,
        preset: PresetIndex,
        slot: tonex_domain::Slot,
    ) -> Result<(), RuntimeError<T::Error>> {
        self.ensure_ready()?;
        let mut candidate = self
            .state_document
            .clone()
            .ok_or(RuntimeError::MissingStateDocument)?;
        candidate.assign_preset(preset, slot, false);
        let frame = candidate.encode_write().map_err(RuntimeError::State)?;
        self.send(&frame)?;
        self.state_document = Some(candidate);
        Ok(())
    }

    fn shift_ab_bank(&mut self, forward: bool) -> Result<(), RuntimeError<T::Error>> {
        self.ensure_ready()?;
        let bank_count = u8::try_from(PRESET_COUNT / 2).expect("TONEX ONE bank count fits u8");
        let current = self.snapshot().selected_preset.get() / 2;
        let bank = if forward {
            (current + 1) % bank_count
        } else {
            (current + bank_count - 1) % bank_count
        };
        let preset_a = PresetIndex::new(bank * 2).expect("A/B bank A preset is valid");
        let preset_b = PresetIndex::new(bank * 2 + 1).expect("A/B bank B preset is valid");
        let mut candidate = self
            .state_document
            .clone()
            .ok_or(RuntimeError::MissingStateDocument)?;
        let selected_slot = match self.snapshot().selected_slot {
            tonex_domain::Slot::B => tonex_domain::Slot::B,
            tonex_domain::Slot::A | tonex_domain::Slot::C => tonex_domain::Slot::A,
        };
        candidate.assign_preset(
            preset_a,
            tonex_domain::Slot::A,
            selected_slot == tonex_domain::Slot::A,
        );
        candidate.assign_preset(
            preset_b,
            tonex_domain::Slot::B,
            selected_slot == tonex_domain::Slot::B,
        );
        let frame = candidate.encode_write().map_err(RuntimeError::State)?;
        self.send(&frame)?;
        self.state_document = Some(candidate);
        let (selected_preset, selected_slot) = match selected_slot {
            tonex_domain::Slot::A => (preset_a, tonex_domain::Slot::A),
            tonex_domain::Slot::B => (preset_b, tonex_domain::Slot::B),
            tonex_domain::Slot::C => unreachable!("C is normalized to A above"),
        };
        let _ = self.controller.handle(AppCommand::ObservedSelection {
            preset: selected_preset,
            slot: selected_slot,
        });
        Ok(())
    }

    /// Gives the platform loop short-lived access to transport lifecycle and
    /// receive operations. Application state remains private to this runtime.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Starts a fresh handshake and sends the exact legacy hello request.
    ///
    /// # Errors
    ///
    /// Returns a transport error if the initial request cannot be sent.
    pub fn connect(&mut self) -> Result<(), RuntimeError<T::Error>> {
        self.decoder.reset();
        self.preset_names.fill(None);
        self.preset_colors = None;
        self.state_document = None;
        self.loaded_parameter_preset = None;
        self.committed_parameter_selection = None;
        self.requested_parameter_preset = None;
        let _ = self.controller.handle(AppCommand::UsbConnected);
        let _ = self.controller.handle(AppCommand::SynchronizationStarted);
        let action = self.session.connect();
        self.apply_action(action, None)
    }

    pub fn disconnect(&mut self) {
        self.session.disconnect();
        self.decoder.reset();
        self.tap_tempo.reset();
        self.mute_restore_master_volume = None;
        self.preset_colors = None;
        self.state_document = None;
        self.loaded_parameter_preset = None;
        self.committed_parameter_selection = None;
        self.requested_parameter_preset = None;
        let _ = self.controller.handle(AppCommand::UsbDisconnected);
    }

    /// Drops only an incomplete receive frame while preserving the live USB
    /// session and synchronized pedal state. Used after callback-ring overflow
    /// so a traffic burst does not masquerade as a physical disconnection.
    pub fn recover_receive_stream(&mut self) {
        self.decoder.reset();
    }

    /// Consumes an arbitrary partial or combined USB receive chunk.
    ///
    /// # Errors
    ///
    /// Returns validated framing, message, state-machine, state-body, or
    /// transport errors. Malformed frames are reported but do not hide later
    /// completely validated frames delivered in the same USB callback.
    pub fn ingest_usb(&mut self, chunk: &[u8]) -> Result<(), RuntimeError<T::Error>> {
        let mut first_error = None;
        for decoded in self.decoder.push(chunk) {
            let result = match decoded {
                Ok(payload) => match parse_payload(&payload) {
                    Ok(message) => self.apply_message(&message, &payload),
                    Err(error) => Err(RuntimeError::Message(error)),
                },
                Err(error) => Err(RuntimeError::Frame(error)),
            };
            if first_error.is_none()
                && let Err(error) = result
            {
                first_error = Some(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    #[must_use]
    pub fn into_transport(self) -> T {
        self.transport
    }

    fn apply_message(
        &mut self,
        message: &Message,
        payload: &[u8],
    ) -> Result<(), RuntimeError<T::Error>> {
        if message.header.message_type == MessageType::StateUpdate {
            let state = parse_state(&message.body).map_err(RuntimeError::State)?;
            if let Some(colors) = parse_preset_colors(&message.body) {
                self.preset_colors = Some(colors);
            }
            self.apply_state_parameters(state);
            self.state_document =
                Some(StateDocument::new(&message.body).map_err(RuntimeError::State)?);
            self.session
                .set_current_preset(state.current_preset().get())
                .map_err(RuntimeError::Session)?;
            let _ = self.controller.handle(AppCommand::ObservedSelection {
                preset: state.current_preset(),
                slot: state.current_slot,
            });
        }
        if message.header.message_type == MessageType::ParameterChanged {
            let change = parse_parameter_change(payload).map_err(RuntimeError::State)?;
            if change.index == 0 {
                // Index zero is the dedicated TONEX ONE global-volume
                // response. Observed device values must never abort the
                // connection; retain the last valid cache value if a device
                // firmware reports a transient/sentinel value.
                if let Ok(master_volume) = parse_master_volume(payload) {
                    if self.mute_restore_master_volume.is_some()
                        && (master_volume - PERFORMANCE_MUTE_DB).abs() > f32::EPSILON
                    {
                        self.mute_restore_master_volume = None;
                    }
                    let _ = self
                        .parameters
                        .set(TONEX_GLOBAL_MASTER_VOLUME, master_volume);
                }
                // On TONEX ONE the physical VOL knob is presented to the
                // player as preset volume even though the shared wire
                // protocol reports it through the special index-zero path.
                // Keep the stage/Web preset-volume readout responsive and
                // confirm it with a fresh authoritative preset block.
                let _ = self.parameters.set(TONEX_PARAM_MODEL_VOLUME, change.value);
                self.loaded_parameter_preset = None;
            } else if change.index == 20 || change.index == TONEX_PARAM_MODEL_VOLUME.get() {
                // TONEX ONE live notifications omit the unknown Model SW1
                // entry from their wire index. The physical preset-volume
                // control therefore reports index 20 although the canonical
                // preset block and outbound command registry use id 21.
                // Applying it as id 20 changes Gain and previously caused the
                // amp artwork to jump while turning Volume.
                let _ = self.parameters.set(TONEX_PARAM_MODEL_VOLUME, change.value);
                // A direct notification makes the UI responsive immediately,
                // until a fresh preset block confirms the authoritative value
                // and catches any firmware-specific live-index variation.
                self.loaded_parameter_preset = None;
            } else {
                // Match the proven TONEX ONE C implementation exactly. This
                // message family is only authoritative for index-zero master
                // volume; other indices are not preset-parameter identifiers
                // and must never overwrite Gain, preset Volume, or FX cache.
                // A nonzero notification is still useful as an invalidation
                // signal: fetch the authoritative 0x0304 current-preset block.
                self.loaded_parameter_preset = None;
            }
        }
        if self.session.state() == SessionState::Ready
            && message.header.message_type == MessageType::PresetDetailsFull
        {
            // The legacy controller deliberately does not parse 0x0303. Its
            // layout is not an authoritative 109-value preset block, but its
            // arrival means the live state changed. Re-query 0x0304 instead.
            self.loaded_parameter_preset = None;
        }
        if self.session.state() == SessionState::Ready
            && message.header.message_type == MessageType::PresetDetails
        {
            // TONEX sends fresh preset details after a pedal or controller
            // selection. The boot state machine no longer owns those live
            // messages, but the parameter cache and Web/board views must.
            let current = self.snapshot().selected_preset;
            let response_preset = self.requested_parameter_preset.unwrap_or(current);
            self.requested_parameter_preset = None;
            if response_preset == current {
                if let Ok(values) = parse_preset_parameters(payload) {
                    let _skipped_device_values = self.parameters.observe_preset_values(&values);
                    self.loaded_parameter_preset = Some(current);
                    self.committed_parameter_selection =
                        Some((current, self.snapshot().selected_slot));
                }
                if let Ok(name) = extract_preset_name(payload) {
                    self.preset_names[usize::from(current.get())] = Some(name);
                }
            }
        }

        let action = self
            .session
            .receive(message.header.message_type)
            .map_err(RuntimeError::Session)?;
        self.apply_action(action, Some(payload))?;
        self.request_current_preset_details_if_needed()
    }

    fn apply_action(
        &mut self,
        action: SessionAction,
        payload: Option<&[u8]>,
    ) -> Result<(), RuntimeError<T::Error>> {
        match action {
            SessionAction::None => {}
            SessionAction::Transmit(bytes) => self.send(&bytes)?,
            SessionAction::StorePresetName { preset } => {
                let payload = payload.ok_or(RuntimeError::State(StateError::MarkerNotFound))?;
                // The C controller advances even when a particular response
                // lacks the optional name marker. Preserve that behavior so
                // one unusual preset cannot deadlock all later requests.
                if let Ok(name) = extract_preset_name(payload) {
                    self.preset_names[usize::from(preset)] = Some(name);
                }
                let _ = self
                    .controller
                    .handle(AppCommand::SynchronizationProgress(preset + 1));
                self.send_continuation()?;
            }
            SessionAction::StoreCurrentPresetDetails { preset } => {
                let payload = payload.ok_or(RuntimeError::State(StateError::MarkerNotFound))?;
                // Preset parameter blocks are firmware-dependent. Apply the
                // block when present, but match the legacy implementation by
                // continuing synchronization when the marker is absent.
                if let Ok(values) = parse_preset_parameters(payload) {
                    let _skipped_device_values = self.parameters.observe_preset_values(&values);
                    self.loaded_parameter_preset = PresetIndex::new(preset).ok();
                    self.committed_parameter_selection = self
                        .loaded_parameter_preset
                        .map(|loaded| (loaded, self.snapshot().selected_slot));
                }
                // Match the proven C controller: request global volume, but
                // do not hold the entire device connection hostage waiting
                // for that optional/asynchronous response.
                self.send_continuation()?;
                self.session.complete_after_current_preset();
                let _ = self.controller.handle(AppCommand::SynchronizationCompleted);
            }
            SessionAction::Continue => self.send_continuation()?,
            SessionAction::SynchronizationComplete => {
                let _ = self.controller.handle(AppCommand::SynchronizationCompleted);
            }
        }
        Ok(())
    }

    fn request_current_preset_details_if_needed(&mut self) -> Result<(), RuntimeError<T::Error>> {
        if self.session.state() != SessionState::Ready {
            return Ok(());
        }
        let current = self.snapshot().selected_preset;
        if self.loaded_parameter_preset == Some(current)
            || self.requested_parameter_preset == Some(current)
        {
            return Ok(());
        }
        let frame = encode_preset_details_request(current.get()).map_err(RuntimeError::Message)?;
        self.send(&frame)?;
        self.requested_parameter_preset = Some(current);
        Ok(())
    }

    fn send_continuation(&mut self) -> Result<(), RuntimeError<T::Error>> {
        if let Some(bytes) = self.session.continuation() {
            self.send(&bytes)?;
        }
        Ok(())
    }

    fn send(&mut self, bytes: &[u8]) -> Result<(), RuntimeError<T::Error>> {
        self.transport
            .transmit(bytes)
            .map_err(RuntimeError::Transport)
    }

    fn ensure_ready(&self) -> Result<(), RuntimeError<T::Error>> {
        let state = self.session.state();
        if state == SessionState::Ready {
            Ok(())
        } else {
            Err(RuntimeError::NotReady(state))
        }
    }

    fn apply_state_parameters(&mut self, state: tonex_protocol::StateSnapshot) {
        let values = [
            (TONEX_GLOBAL_BPM, state.bpm),
            (TONEX_GLOBAL_INPUT_TRIM, state.input_trim_db),
            (
                TONEX_GLOBAL_CABSIM_BYPASS,
                f32::from(u8::from(state.cabinet_bypass)),
            ),
            (TONEX_GLOBAL_TEMPO_SOURCE, f32::from(state.tempo_source)),
            (
                TONEX_GLOBAL_TUNING_REFERENCE,
                f32::from(state.tuning_reference_hz),
            ),
            (TONEX_GLOBAL_BYPASS, f32::from(u8::from(state.bypass))),
        ];
        for (id, value) in values {
            // These are observations, not user writes. TONEX firmware can
            // expose sentinel or temporarily out-of-range values while its
            // state changes. Keep the last valid value without making the
            // entire USB session fail; outbound writes remain fully checked.
            let _ = self.parameters.set(id, value);
        }
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn tuning_reference_hz(value: f32) -> u16 {
    value.clamp(400.0, 480.0) as u16
}

fn display_preset_color(raw: PresetColor) -> ThemeColor {
    let mapped = match raw.rgb888() {
        0x00ff_0000 => 0x00ff_0619,
        0x00ff_3f00 => 0x00e7_5116,
        0x009f_ff00 => 0x00ff_e12a,
        0x0000_ff00 => 0x0000_f642,
        0x000f_ff2f => 0x0000_fbcd,
        0x0000_ffff => 0x0000_9dfc,
        0x0000_00ff => 0x0000_44fb,
        0x002f_00ff => 0x006c_64fb,
        0x00ff_00ff | 0x000b_0b0b => 0x0084_5083,
        0x00bf_bfbf => 0x00ff_8bfc,
        0x0011_0000 => 0x0087_1218,
        0x0011_1100 => 0x007a_3616,
        0x0011_2200 => 0x0085_771c,
        0x0000_1100 => 0x0005_802d,
        0x0000_2206 => 0x0000_826e,
        0x0000_1919 => 0x0000_5882,
        0x0000_0011 => 0x0000_2e82,
        0x0005_0011 => 0x0043_3e82,
        0x000a_000a => 0x0085_1a6b,
        0x0000_0000 => 0x0059_5959,
        other => other,
    };
    let [_, red, green, blue] = mapped.to_be_bytes();
    ThemeColor::new(red, green, blue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};
    use tonex_domain::{ConnectionState, Slot, SyncState};
    use tonex_parameters::{PRESET_PARAMETER_COUNT, ParameterId, spec};
    use tonex_protocol::{PRESET_NAME_MARKER, decode_frame, encode_frame};

    #[derive(Default)]
    struct MockUsb {
        writes: Vec<Vec<u8>>,
        fail: bool,
    }

    impl UsbTransport for MockUsb {
        type Error = ();

        fn transmit(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
            if self.fail {
                Err(())
            } else {
                self.writes.push(bytes.to_vec());
                Ok(())
            }
        }
    }

    fn response(message_type: u16, body: &[u8]) -> Vec<u8> {
        let [type_low, type_high] = message_type.to_le_bytes();
        let body_len = u16::try_from(body.len()).expect("test response body fits protocol length");
        let [length_low, length_high] = body_len.to_le_bytes();
        let mut payload = vec![
            0xb9,
            0x03,
            0x81,
            type_low,
            type_high,
            0x82,
            length_low,
            length_high,
            0x80,
            0x0b,
        ];
        payload.extend_from_slice(body);
        encode_frame(&payload)
    }

    fn state_body() -> [u8; 64] {
        let mut body = [0; 64];
        body[15..19].copy_from_slice(&(-3.5_f32).to_le_bytes());
        body[20] = 1;
        body[64 - 18] = 4;
        body[64 - 16] = 5;
        body[64 - 14] = 6;
        body[64 - 12] = 1;
        body[64 - 11] = 1;
        body[64 - 4..64].copy_from_slice(&120.0_f32.to_le_bytes());
        body[64 - 6] = 1;
        body[64 - 9..64 - 7].copy_from_slice(&440_u16.to_le_bytes());
        body
    }

    fn preset_body(index: u8) -> Vec<u8> {
        let mut body = PRESET_NAME_MARKER.to_vec();
        let mut name = [0; 32];
        name[..7].copy_from_slice(b"Preset ");
        name[7] = b'0' + (index % 10);
        body.extend_from_slice(&name);
        body
    }

    fn legacy_boot_sentinel_body() -> Vec<u8> {
        preset_body(u8::try_from(PRESET_COUNT).expect("preset count fits u8"))
    }

    fn master_volume_body(wire_value: f32) -> Vec<u8> {
        parameter_change_body(0, wire_value)
    }

    fn parameter_change_body(index: u16, value: f32) -> Vec<u8> {
        let [low, high] = index.to_le_bytes();
        let mut body = vec![0xb9, 0x04, 0x03, low, high, 0x88];
        body.extend_from_slice(&value.to_le_bytes());
        body
    }

    fn full_preset_body(index: u8) -> Vec<u8> {
        let mut body = preset_body(index);
        body.extend_from_slice(&[0xba, 0x03, 0xba, 0x6d]);
        for index in 0..PRESET_PARAMETER_COUNT {
            let id = ParameterId::new(u16::try_from(index).expect("parameter index fits u16"))
                .expect("preset parameter index is valid");
            body.push(0x88);
            let value = if [
                TONEX_PARAM_NOISE_GATE_ENABLE,
                TONEX_PARAM_MODULATION_ENABLE,
                TONEX_PARAM_REVERB_ENABLE,
            ]
            .contains(&id)
            {
                1.0
            } else if [TONEX_PARAM_COMP_ENABLE, TONEX_PARAM_DELAY_ENABLE].contains(&id) {
                0.0
            } else {
                spec(id).default
            };
            body.extend_from_slice(&value.to_le_bytes());
        }
        body
    }

    fn full_preset_body_with_volume(index: u8, volume: f32) -> Vec<u8> {
        let mut body = full_preset_body(index);
        let parameter_block = PRESET_NAME_MARKER.len() + 32 + 4;
        let value_offset = parameter_block + usize::from(TONEX_PARAM_MODEL_VOLUME.get()) * 5 + 1;
        body[value_offset..value_offset + 4].copy_from_slice(&volume.to_le_bytes());
        body
    }

    fn ready_runtime() -> TonexRuntime<MockUsb> {
        let mut runtime = TonexRuntime::new(MockUsb::default());
        runtime.connect().expect("hello transmit succeeds");
        runtime
            .ingest_usb(&response(0x0002, &[]))
            .expect("hello response is valid");
        runtime
            .ingest_usb(&response(0x0306, &state_body()))
            .expect("state response is valid");
        for preset in 0..u8::try_from(PRESET_COUNT).expect("preset count fits u8") {
            runtime
                .ingest_usb(&response(0x0304, &preset_body(preset)))
                .expect("preset summary is valid");
        }
        runtime
            .ingest_usb(&response(0x0304, &legacy_boot_sentinel_body()))
            .expect("legacy boot sentinel advances to current preset");
        runtime
            .ingest_usb(&response(0x0304, &full_preset_body(5)))
            .expect("current preset details are valid");
        runtime
    }

    #[test]
    fn performance_mute_restores_the_exact_previous_master_volume() {
        let mut runtime = ready_runtime();
        runtime
            .write_master_volume(-12.5)
            .expect("initial master volume is valid");
        runtime.transport_mut().writes.clear();

        runtime
            .apply_command(AppCommand::ToggleMute)
            .expect("mute writes the minimum master volume");
        assert!(runtime.ui_snapshot().mute.is_active());
        assert!((runtime.parameter_value(TONEX_GLOBAL_MASTER_VOLUME) - -40.0).abs() < f32::EPSILON);
        assert_eq!(
            runtime.transport_mut().writes.last(),
            Some(&encode_master_volume(-40.0).expect("mute volume is valid"))
        );

        runtime
            .apply_command(AppCommand::ToggleMute)
            .expect("second toggle restores master volume");
        assert!(!runtime.ui_snapshot().mute.is_active());
        assert!((runtime.parameter_value(TONEX_GLOBAL_MASTER_VOLUME) - -12.5).abs() < f32::EPSILON);
        assert_eq!(
            runtime.transport_mut().writes.last(),
            Some(&encode_master_volume(-12.5).expect("restore volume is valid"))
        );
    }

    fn assert_typed_user_commands(runtime: &mut TonexRuntime<MockUsb>) {
        assert_independent_slot_assignment(runtime);

        let write_count = runtime.transport_mut().writes.len();
        let selected = PresetIndex::new(9).expect("valid preset");
        runtime
            .select_preset_in_slot(selected, Slot::C)
            .expect("ready runtime has a state document");
        assert_eq!(runtime.snapshot().selected_preset, selected);
        assert_eq!(runtime.snapshot().selected_slot, Slot::C);
        assert_eq!(runtime.transport_mut().writes.len(), write_count + 2);
        let state_write = runtime
            .transport_mut()
            .writes
            .iter()
            .rev()
            .nth(1)
            .expect("state write was transmitted");
        let payload = decode_frame(state_write).expect("valid frame");
        assert_eq!(
            &payload[..11],
            &[0xb9, 0x03, 0x81, 0x06, 0x03, 0x82, 64, 0, 0x80, 0x0b, 0x03]
        );
        let state = parse_state(&payload[11..]).expect("valid mutated state");
        assert_eq!(state.current_preset(), selected);
        assert_eq!(state.current_slot, Slot::C);
        assert_eq!(
            runtime.transport_mut().writes.last(),
            Some(&encode_preset_details_request(selected.get()).expect("valid details request"))
        );

        runtime
            .write_parameter(TONEX_GLOBAL_BPM, 96.0)
            .expect("state-backed global writes through the preserved document");
        assert!((runtime.ui_snapshot().bpm - 96.0).abs() < f32::EPSILON);

        runtime
            .apply_command(AppCommand::NextPreset)
            .expect("typed user command uses the state write path");
        assert_eq!(runtime.snapshot().selected_preset.get(), 10);

        let write_count = runtime.transport_mut().writes.len();
        runtime
            .apply_command(AppCommand::TapTempo(1_000))
            .expect("first tap only establishes the tempo origin");
        assert_eq!(runtime.transport_mut().writes.len(), write_count);
        runtime
            .apply_command(AppCommand::TapTempo(1_500))
            .expect("second tap writes the calculated tempo");
        assert_eq!(runtime.transport_mut().writes.len(), write_count + 1);
        assert!((runtime.ui_snapshot().bpm - 120.0).abs() < f32::EPSILON);

        assert_typed_parameter_and_bank_commands(runtime, write_count);
    }

    fn assert_independent_slot_assignment(runtime: &mut TonexRuntime<MockUsb>) {
        let selected_before_assignment = runtime.snapshot().selected_preset;
        let slot_before_assignment = runtime.snapshot().selected_slot;
        runtime
            .apply_command(AppCommand::AssignPresetToSlot {
                preset: PresetIndex::new(12).expect("valid preset"),
                slot: Slot::C,
            })
            .expect("independent slot assignment uses the preserved state document");
        assert_eq!(
            runtime.snapshot().selected_preset,
            selected_before_assignment
        );
        assert_eq!(runtime.snapshot().selected_slot, slot_before_assignment);
        assert_eq!(runtime.ui_snapshot().slot_presets[2].get(), 12);
        let assignment_write = runtime
            .transport_mut()
            .writes
            .last()
            .expect("slot assignment was transmitted");
        let payload = decode_frame(assignment_write).expect("valid assignment frame");
        let state = parse_state(&payload[11..]).expect("valid assigned state");
        assert_eq!(state.slot_c.get(), 12);
        assert_eq!(state.current_preset(), selected_before_assignment);
        assert_eq!(state.current_slot, slot_before_assignment);

        runtime
            .apply_command(AppCommand::SelectSlot(Slot::C))
            .expect("slot selection recalls its assigned preset");
        assert_eq!(runtime.snapshot().selected_preset.get(), 12);
        assert_eq!(runtime.snapshot().selected_slot, Slot::C);
        let selection_write = runtime
            .transport_mut()
            .writes
            .iter()
            .rev()
            .nth(1)
            .expect("slot selection was transmitted");
        let payload = decode_frame(selection_write).expect("valid slot selection frame");
        let state = parse_state(&payload[11..]).expect("valid selected state");
        assert_eq!(state.current_preset().get(), 12);
        assert_eq!(state.current_slot, Slot::C);
        assert_eq!(
            runtime.transport_mut().writes.last(),
            Some(&encode_preset_details_request(12).expect("valid preset-details request"))
        );
    }

    fn assert_typed_parameter_and_bank_commands(
        runtime: &mut TonexRuntime<MockUsb>,
        write_count: usize,
    ) {
        runtime
            .apply_command(AppCommand::SetMasterVolumeTenths(-125))
            .expect("typed volume command uses the master-volume write path");
        assert_eq!(runtime.transport_mut().writes.len(), write_count + 2);
        assert!((runtime.parameter_value(TONEX_GLOBAL_MASTER_VOLUME) - -12.5).abs() < f32::EPSILON);

        runtime
            .apply_command(AppCommand::SetParameter {
                id: tonex_parameters::TONEX_PARAM_EQ_BASS,
                value: 6.5,
            })
            .expect("typed parameter command uses the validated write path");
        assert_eq!(runtime.transport_mut().writes.len(), write_count + 3);
        assert!(
            (runtime.parameter_value(tonex_parameters::TONEX_PARAM_EQ_BASS) - 6.5).abs()
                < f32::EPSILON
        );

        let selected = PresetIndex::new(3).expect("valid preset");
        runtime
            .apply_command(AppCommand::SelectPresetInSlot {
                preset: selected,
                slot: Slot::A,
            })
            .expect("typed slot load uses the preserved state document");
        assert_eq!(runtime.transport_mut().writes.len(), write_count + 5);
        assert_eq!(runtime.snapshot().selected_preset, selected);
        assert_eq!(runtime.snapshot().selected_slot, Slot::A);

        runtime
            .apply_command(AppCommand::NextAbBank)
            .expect("A/B bank up writes both slots and selects A");
        assert_eq!(runtime.snapshot().selected_preset.get(), 4);
        assert_eq!(runtime.snapshot().selected_slot, Slot::A);
        let state_write = runtime
            .transport_mut()
            .writes
            .last()
            .expect("A/B bank state was transmitted");
        let payload = decode_frame(state_write).expect("valid frame");
        let state = parse_state(&payload[11..]).expect("valid A/B bank state");
        assert_eq!(state.slot_a.get(), 4);
        assert_eq!(state.slot_b.get(), 5);

        assert_preset_volume_write(runtime);

        runtime
            .apply_command(AppCommand::PreviousAbBank)
            .expect("A/B bank down wraps through the same typed path");
        assert_eq!(runtime.snapshot().selected_preset.get(), 2);
        assert_eq!(
            runtime.apply_command(AppCommand::UsbConnected),
            Err(RuntimeError::UnsupportedCommand)
        );
    }

    fn assert_preset_volume_write(runtime: &mut TonexRuntime<MockUsb>) {
        let gain_before = runtime.parameter_value(TONEX_PARAM_MODEL_GAIN);
        let skin_before = runtime.ui_snapshot().skin;
        runtime
            .apply_command(AppCommand::SetPresetVolumeTenths(75))
            .expect("preset volume is sent through the live parameter path");
        assert!((runtime.ui_snapshot().preset_volume - 7.5).abs() < f32::EPSILON);
        assert!(
            (runtime.parameter_value(TONEX_PARAM_MODEL_GAIN) - gain_before).abs() < f32::EPSILON
        );
        assert_eq!(runtime.ui_snapshot().skin, skin_before);
        let volume_write = runtime
            .transport_mut()
            .writes
            .last()
            .expect("preset volume write was transmitted");
        let payload = decode_frame(volume_write).expect("preset volume frame is valid");
        assert_eq!(payload[15], TONEX_PARAM_MODEL_VOLUME.get().to_le_bytes()[0]);
        assert_eq!(&payload[17..21], &7.5_f32.to_le_bytes());
    }

    #[test]
    fn complete_sync_handles_partial_chunks_and_stores_all_twenty_names() {
        let mut runtime = TonexRuntime::new(MockUsb::default());
        runtime.connect().expect("hello transmit succeeds");
        assert_eq!(
            runtime.snapshot().connection,
            ConnectionState::Synchronizing
        );

        let hello = response(0x0002, &[]);
        let split = hello.len() / 2;
        runtime
            .ingest_usb(&hello[..split])
            .expect("partial frame is retained");
        runtime
            .ingest_usb(&hello[split..])
            .expect("hello completes");
        runtime
            .ingest_usb(&response(0x0306, &state_body()))
            .expect("state is valid");

        for preset in 0..u8::try_from(PRESET_COUNT).expect("preset count fits u8") {
            runtime
                .ingest_usb(&response(0x0304, &preset_body(preset)))
                .expect("preset summary is valid");
        }
        runtime
            .ingest_usb(&response(0x0304, &legacy_boot_sentinel_body()))
            .expect("legacy boot sentinel advances to current preset");
        runtime
            .ingest_usb(&response(0x0304, &full_preset_body(5)))
            .expect("current preset details are valid");
        assert_eq!(runtime.snapshot().connection, ConnectionState::Ready);
        runtime
            .ingest_usb(&response(0x0309, &master_volume_body(10.0)))
            .expect("global response updates the ready session");

        assert_eq!(runtime.snapshot().connection, ConnectionState::Ready);
        assert_eq!(runtime.snapshot().sync, SyncState::Complete);
        assert_eq!(runtime.snapshot().selected_preset.get(), 5);
        assert_eq!(runtime.snapshot().selected_slot, Slot::B);
        assert!((runtime.parameter_value(TONEX_GLOBAL_MASTER_VOLUME) - 3.0).abs() < f32::EPSILON);
        let ui = runtime.ui_snapshot();
        assert_eq!(ui.preset_label.as_bytes(), b"Preset 5");
        assert!((ui.bpm - 120.0).abs() < f32::EPSILON);
        assert!((ui.master_volume_db - 3.0).abs() < f32::EPSILON);
        assert!(ui.bypass);
        assert!(ui.fx.gate);
        assert!(!ui.fx.compressor);
        assert!(ui.fx.amp);
        assert!(!ui.fx.cabinet);
        assert!(ui.fx.modulation);
        assert!(!ui.fx.delay);
        assert!(ui.fx.reverb);
        for preset in 0..u8::try_from(PRESET_COUNT).expect("preset count fits u8") {
            let preset = PresetIndex::new(preset).expect("fixture index is valid");
            assert!(runtime.preset_name(preset).is_some());
        }

        assert_typed_user_commands(&mut runtime);
    }

    #[test]
    fn rapid_pedal_state_updates_do_not_break_boot_synchronization() {
        let mut runtime = TonexRuntime::new(MockUsb::default());
        runtime.connect().expect("hello transmit succeeds");
        runtime
            .ingest_usb(&response(0x0002, &[]))
            .expect("hello response is valid");
        runtime
            .ingest_usb(&response(0x0306, &state_body()))
            .expect("requested state starts synchronization");

        for preset in 0..u8::try_from(PRESET_COUNT).expect("preset count fits u8") {
            // This is the ordering observed when the hardware preset control
            // is moved quickly while names are still being synchronized.
            runtime
                .ingest_usb(&response(0x0306, &state_body()))
                .expect("unsolicited pedal state is a live event, not a protocol fault");
            runtime
                .ingest_usb(&response(0x0304, &preset_body(preset)))
                .expect("requested preset response still advances synchronization");
        }
        runtime
            .ingest_usb(&response(0x0304, &legacy_boot_sentinel_body()))
            .expect("legacy boot sentinel advances to current preset");
        runtime
            .ingest_usb(&response(0x0304, &full_preset_body(5)))
            .expect("current preset details complete synchronization");

        assert_eq!(runtime.snapshot().connection, ConnectionState::Ready);
        assert_eq!(runtime.snapshot().sync, SyncState::Complete);
    }

    #[test]
    fn rapid_live_selection_discards_stale_details_and_reloads_latest_preset() {
        let mut runtime = ready_runtime();
        let original_volume = runtime.ui_snapshot().preset_volume;
        let writes_before = runtime.transport_mut().writes.len();

        let mut preset_nine = state_body();
        preset_nine[64 - 16] = 9;
        runtime
            .ingest_usb(&response(0x0306, &preset_nine))
            .expect("physical preset change is accepted");
        assert_eq!(runtime.snapshot().selected_preset.get(), 9);
        assert_eq!(runtime.ui_snapshot().preset.get(), 5);
        assert_eq!(runtime.transport_mut().writes.len(), writes_before + 1);
        assert_eq!(
            runtime.transport_mut().writes.last(),
            Some(&encode_preset_details_request(9).expect("valid request"))
        );

        let mut preset_ten = state_body();
        preset_ten[64 - 16] = 10;
        runtime
            .ingest_usb(&response(0x0306, &preset_ten))
            .expect("newer physical preset change is accepted");
        assert_eq!(runtime.snapshot().selected_preset.get(), 10);
        assert_eq!(runtime.ui_snapshot().preset.get(), 5);

        runtime
            .ingest_usb(&response(0x0304, &full_preset_body_with_volume(9, -10.0)))
            .expect("late response is discarded and latest preset is requested");
        assert!((runtime.ui_snapshot().preset_volume - original_volume).abs() < f32::EPSILON);
        assert_eq!(
            runtime.transport_mut().writes.last(),
            Some(&encode_preset_details_request(10).expect("valid request"))
        );

        runtime
            .ingest_usb(&response(0x0304, &full_preset_body_with_volume(10, 8.0)))
            .expect("latest preset details update the live cache");
        assert_eq!(runtime.ui_snapshot().preset.get(), 10);
        assert!((runtime.ui_snapshot().preset_volume - 8.0).abs() < f32::EPSILON);
        assert_eq!(
            runtime
                .preset_name(PresetIndex::new(10).expect("valid preset"))
                .expect("latest preset name is stored")
                .as_bytes(),
            b"Preset 0"
        );
    }

    #[test]
    fn non_authoritative_live_messages_cannot_corrupt_preset_volume_or_amp() {
        let mut runtime = ready_runtime();
        let volume = runtime.ui_snapshot().preset_volume;
        let gain = runtime.ui_snapshot().model_gain;
        let skin = runtime.ui_snapshot().skin;

        runtime
            .ingest_usb(&response(0x0309, &parameter_change_body(42, 9.5)))
            .expect("non-master 0309 notification is safely ignored");
        runtime
            .ingest_usb(&response(0x0303, &full_preset_body_with_volume(5, -10.0)))
            .expect("full notification triggers an authoritative refresh");

        assert!((runtime.ui_snapshot().preset_volume - volume).abs() < f32::EPSILON);
        assert!((runtime.ui_snapshot().model_gain - gain).abs() < f32::EPSILON);
        assert_eq!(runtime.ui_snapshot().skin, skin);
        assert_eq!(
            runtime.transport_mut().writes.last(),
            Some(&encode_preset_details_request(5).expect("valid request"))
        );

        runtime
            .ingest_usb(&response(0x0304, &full_preset_body_with_volume(5, 7.2)))
            .expect("authoritative refresh updates the current preset cache");
        assert!((runtime.ui_snapshot().preset_volume - 7.2).abs() < f32::EPSILON);
    }

    #[test]
    fn physical_tonex_one_volume_notification_updates_volume_without_touching_gain() {
        for wire_index in [0, 20, TONEX_PARAM_MODEL_VOLUME.get()] {
            let mut runtime = ready_runtime();
            let gain = runtime.ui_snapshot().model_gain;

            runtime
                .ingest_usb(&response(0x0309, &parameter_change_body(wire_index, 6.4)))
                .expect("physical preset-volume notification is accepted");

            assert!((runtime.ui_snapshot().preset_volume - 6.4).abs() < f32::EPSILON);
            assert!((runtime.ui_snapshot().model_gain - gain).abs() < f32::EPSILON);
            assert_eq!(
                runtime.transport_mut().writes.last(),
                Some(&encode_preset_details_request(5).expect("valid authoritative refresh"))
            );
        }
    }

    #[test]
    fn malformed_frame_does_not_hide_later_valid_frame_in_same_usb_chunk() {
        let mut runtime = TonexRuntime::new(MockUsb::default());
        runtime.connect().expect("hello transmit succeeds");
        let mut damaged = response(0x0002, &[]);
        damaged[3] ^= 0x01;
        damaged.extend_from_slice(&response(0x0002, &[]));

        assert!(matches!(
            runtime.ingest_usb(&damaged),
            Err(RuntimeError::Frame(_))
        ));
        assert_eq!(
            runtime.transport_mut().writes.len(),
            2,
            "valid hello after the damaged frame still sends the state request"
        );
    }

    #[test]
    fn disconnect_clears_in_flight_decoder_and_sync_state() {
        let mut runtime = TonexRuntime::new(MockUsb::default());
        runtime.connect().expect("hello transmit succeeds");
        let hello = response(0x0002, &[]);
        runtime
            .ingest_usb(&hello[..3])
            .expect("partial frame is retained");
        runtime.disconnect();
        assert_eq!(runtime.snapshot().connection, ConnectionState::Disconnected);
        assert_eq!(runtime.snapshot().sync, SyncState::NotStarted);
        assert!(runtime.ingest_usb(&hello[3..]).is_ok());
        assert_eq!(runtime.snapshot().connection, ConnectionState::Disconnected);
    }

    #[test]
    fn transport_failure_is_propagated_without_fake_success() {
        let mut runtime = TonexRuntime::new(MockUsb {
            writes: Vec::new(),
            fail: true,
        });
        assert_eq!(runtime.connect(), Err(RuntimeError::Transport(())));
    }

    #[test]
    fn parameter_writes_are_rejected_until_ready() {
        let mut runtime = TonexRuntime::new(MockUsb::default());
        assert_eq!(
            runtime.write_parameter(tonex_parameters::TONEX_PARAM_EQ_BASS, 7.0),
            Err(RuntimeError::NotReady(SessionState::Disconnected))
        );
        assert!(runtime.transport_mut().writes.is_empty());
    }

    #[test]
    fn every_user_device_command_is_rejected_before_synchronization() {
        let mut runtime = TonexRuntime::new(MockUsb::default());
        for command in [
            AppCommand::NextPreset,
            AppCommand::SelectSlot(Slot::B),
            AppCommand::TapTempo(1_000),
            AppCommand::SetMasterVolumeTenths(-100),
            AppCommand::SetParameter {
                id: tonex_parameters::TONEX_PARAM_DELAY_ENABLE,
                value: 1.0,
            },
        ] {
            assert_eq!(
                runtime.apply_command(command),
                Err(RuntimeError::NotReady(SessionState::Disconnected))
            );
        }
        assert!(runtime.transport_mut().writes.is_empty());
    }
}
