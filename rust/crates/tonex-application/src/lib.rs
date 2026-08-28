#![no_std]

extern crate alloc;

mod runtime;

pub use runtime::{RuntimeError, TonexRuntime};

use tonex_domain::{
    ConnectionState, FootAction, FootButton, FootGesture, PresetIndex, Slot, SyncState,
};
use tonex_parameters::ParameterId;
use tonex_settings::{BluetoothPeer, MidiSettings, Settings, WifiSettings};
use tonex_skins::SkinSelection;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AppCommand {
    UsbConnected,
    UsbDisconnected,
    SynchronizationStarted,
    SynchronizationProgress(u8),
    SynchronizationCompleted,
    SelectPreset(PresetIndex),
    SelectPresetInSlot {
        preset: PresetIndex,
        slot: Slot,
    },
    AssignPresetToSlot {
        preset: PresetIndex,
        slot: Slot,
    },
    NextPreset,
    PreviousPreset,
    NextAbBank,
    PreviousAbBank,
    SelectSlot(Slot),
    TapTempo(u64),
    SetPresetVolumeTenths(u8),
    SetMasterVolumeTenths(i16),
    ToggleMute,
    SetParameter {
        id: ParameterId,
        value: f32,
    },
    ObservedSelection {
        preset: PresetIndex,
        slot: Slot,
    },
    UpdateMidi(MidiSettings),
    UpdateMidiChannel(u8),
    StartBluetoothScan,
    PairBluetooth(BluetoothPeer),
    ForgetBluetooth,
    SetGlobalFootBinding {
        button: FootButton,
        gesture: FootGesture,
        action: FootAction,
    },
    SetPresetFootBinding {
        preset: PresetIndex,
        button: FootButton,
        gesture: FootGesture,
        action: FootAction,
    },
    UpdateWifi(WifiSettings),
    UpdateBrightness(u8),
    SetPresetSkin {
        preset: PresetIndex,
        selection: SkinSelection,
    },
}

impl AppCommand {
    /// True when executing this command requires a synchronized TONEX ONE.
    /// Local controller settings and internal lifecycle events remain usable
    /// while the pedal is absent.
    #[must_use]
    pub const fn requires_tonex(self) -> bool {
        matches!(
            self,
            Self::SelectPreset(_)
                | Self::SelectPresetInSlot { .. }
                | Self::AssignPresetToSlot { .. }
                | Self::NextPreset
                | Self::PreviousPreset
                | Self::NextAbBank
                | Self::PreviousAbBank
                | Self::SelectSlot(_)
                | Self::TapTempo(_)
                | Self::SetPresetVolumeTenths(_)
                | Self::SetMasterVolumeTenths(_)
                | Self::ToggleMute
                | Self::SetParameter { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppEffect {
    None,
    BeginSynchronization,
    SelectPreset(PresetIndex),
    SelectSlot(Slot),
    PersistSettings(Settings),
    PersistSettingsLive(Settings),
    StartBluetoothScan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppSnapshot {
    pub connection: ConnectionState,
    pub selected_preset: PresetIndex,
    pub selected_slot: Slot,
    pub sync: SyncState,
    pub settings: Settings,
}

#[derive(Debug)]
pub struct AppController {
    state: AppSnapshot,
}

impl Default for AppController {
    fn default() -> Self {
        Self {
            state: AppSnapshot {
                connection: ConnectionState::Disconnected,
                selected_preset: PresetIndex::FIRST,
                selected_slot: Slot::A,
                sync: SyncState::NotStarted,
                settings: Settings::default(),
            },
        }
    }
}

impl AppController {
    #[must_use]
    pub fn with_settings(settings: Settings) -> Self {
        let mut controller = Self::default();
        controller.state.selected_preset = settings.selected_preset;
        controller.state.selected_slot = settings.selected_slot;
        controller.state.settings = settings;
        controller
    }

    #[must_use]
    pub const fn snapshot(&self) -> AppSnapshot {
        self.state
    }

    pub fn handle(&mut self, command: AppCommand) -> AppEffect {
        match command {
            AppCommand::UsbConnected => {
                self.state.connection = ConnectionState::Connecting;
                AppEffect::BeginSynchronization
            }
            AppCommand::UsbDisconnected => {
                self.state.connection = ConnectionState::Disconnected;
                self.state.sync = SyncState::NotStarted;
                AppEffect::None
            }
            AppCommand::SynchronizationStarted => {
                self.state.connection = ConnectionState::Synchronizing;
                self.state.sync = SyncState::InProgress { completed: 0 };
                AppEffect::None
            }
            AppCommand::SynchronizationProgress(completed) => {
                self.state.sync = SyncState::InProgress { completed };
                AppEffect::None
            }
            AppCommand::SynchronizationCompleted => {
                self.state.connection = ConnectionState::Ready;
                self.state.sync = SyncState::Complete;
                AppEffect::None
            }
            AppCommand::SelectPreset(preset) | AppCommand::SelectPresetInSlot { preset, .. } => {
                self.state.selected_preset = preset;
                AppEffect::SelectPreset(preset)
            }
            AppCommand::NextPreset => {
                self.state.selected_preset = self.state.selected_preset.next_wrapping();
                AppEffect::SelectPreset(self.state.selected_preset)
            }
            AppCommand::PreviousPreset => {
                self.state.selected_preset = self.state.selected_preset.previous_wrapping();
                AppEffect::SelectPreset(self.state.selected_preset)
            }
            AppCommand::SelectSlot(slot) => {
                self.state.selected_slot = slot;
                AppEffect::SelectSlot(slot)
            }
            AppCommand::NextAbBank
            | AppCommand::PreviousAbBank
            | AppCommand::AssignPresetToSlot { .. }
            | AppCommand::TapTempo(_)
            | AppCommand::SetPresetVolumeTenths(_)
            | AppCommand::SetMasterVolumeTenths(_)
            | AppCommand::ToggleMute
            | AppCommand::SetParameter { .. } => AppEffect::None,
            AppCommand::ObservedSelection { preset, slot } => {
                self.state.selected_preset = preset;
                self.state.selected_slot = slot;
                AppEffect::None
            }
            AppCommand::UpdateMidi(midi) => {
                self.state.settings.midi = midi;
                AppEffect::PersistSettings(self.state.settings)
            }
            AppCommand::UpdateMidiChannel(channel) => {
                self.state.settings.midi.channel = channel;
                self.state.settings.midi.serial_enabled = false;
                self.state.settings.midi.ble_enabled = true;
                AppEffect::PersistSettings(self.state.settings)
            }
            AppCommand::StartBluetoothScan => AppEffect::StartBluetoothScan,
            AppCommand::PairBluetooth(peer) => self.pair_bluetooth(peer),
            AppCommand::ForgetBluetooth => self.forget_bluetooth(),
            AppCommand::SetGlobalFootBinding {
                button,
                gesture,
                action,
            } => self.set_global_foot_binding(button, gesture, action),
            AppCommand::SetPresetFootBinding {
                preset,
                button,
                gesture,
                action,
            } => self.set_preset_foot_binding(preset, button, gesture, action),
            AppCommand::UpdateWifi(wifi) => {
                self.state.settings.wifi = wifi;
                AppEffect::PersistSettings(self.state.settings)
            }
            AppCommand::UpdateBrightness(percent) => {
                self.state.settings.display_brightness_percent = percent.min(100);
                AppEffect::PersistSettingsLive(self.state.settings)
            }
            AppCommand::SetPresetSkin { preset, selection } => {
                self.state.settings.preset_skins.set(preset, selection);
                AppEffect::PersistSettingsLive(self.state.settings)
            }
        }
    }

    fn pair_bluetooth(&mut self, peer: BluetoothPeer) -> AppEffect {
        self.state.settings.midi.serial_enabled = false;
        self.state.settings.midi.ble_enabled = true;
        self.state.settings.midi.paired_peer = Some(peer);
        AppEffect::PersistSettings(self.state.settings)
    }

    fn forget_bluetooth(&mut self) -> AppEffect {
        self.state.settings.midi.paired_peer = None;
        AppEffect::PersistSettings(self.state.settings)
    }

    fn set_global_foot_binding(
        &mut self,
        button: FootButton,
        gesture: FootGesture,
        action: FootAction,
    ) -> AppEffect {
        self.state
            .settings
            .foot_controller
            .set_global(button, gesture, action);
        AppEffect::PersistSettingsLive(self.state.settings)
    }

    fn set_preset_foot_binding(
        &mut self,
        preset: PresetIndex,
        button: FootButton,
        gesture: FootGesture,
        action: FootAction,
    ) -> AppEffect {
        self.state
            .settings
            .foot_controller
            .set_preset_override(preset, button, gesture, action);
        AppEffect::PersistSettingsLive(self.state.settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_lifecycle_resets_sync_on_disconnect() {
        let mut controller = AppController::default();
        assert_eq!(
            controller.handle(AppCommand::UsbConnected),
            AppEffect::BeginSynchronization
        );
        controller.handle(AppCommand::SynchronizationStarted);
        controller.handle(AppCommand::SynchronizationCompleted);
        assert_eq!(controller.snapshot().connection, ConnectionState::Ready);

        controller.handle(AppCommand::UsbDisconnected);
        assert_eq!(
            controller.snapshot(),
            AppSnapshot {
                connection: ConnectionState::Disconnected,
                selected_preset: PresetIndex::FIRST,
                selected_slot: Slot::A,
                sync: SyncState::NotStarted,
                settings: Settings::default(),
            }
        );
    }

    #[test]
    fn only_device_commands_require_a_ready_tonex() {
        assert!(AppCommand::NextPreset.requires_tonex());
        assert!(AppCommand::TapTempo(10).requires_tonex());
        assert!(
            AppCommand::SetParameter {
                id: tonex_parameters::TONEX_PARAM_DELAY_ENABLE,
                value: 1.0,
            }
            .requires_tonex()
        );
        assert!(!AppCommand::UpdateBrightness(50).requires_tonex());
        assert!(!AppCommand::UpdateWifi(WifiSettings::default()).requires_tonex());
        assert!(!AppCommand::UsbDisconnected.requires_tonex());
    }

    #[test]
    fn navigation_emits_effect_and_wraps() {
        let mut controller = AppController::default();
        let effect = controller.handle(AppCommand::PreviousPreset);
        let expected = PresetIndex::new(19).expect("19 is valid");
        assert_eq!(effect, AppEffect::SelectPreset(expected));
        assert_eq!(controller.snapshot().selected_preset, expected);
    }

    #[test]
    fn wifi_update_is_owned_by_controller_and_emits_persistence_effect() {
        let mut controller = AppController::default();
        let wifi = WifiSettings {
            mode: tonex_settings::WifiMode::Station,
            ..WifiSettings::default()
        };
        assert_eq!(
            controller.handle(AppCommand::UpdateWifi(wifi)),
            AppEffect::PersistSettings(Settings {
                wifi,
                ..Settings::default()
            })
        );
        assert_eq!(controller.snapshot().settings.wifi, wifi);
    }

    #[test]
    fn midi_update_is_owned_by_controller_and_emits_persistence_effect() {
        let mut controller = AppController::default();
        let midi = MidiSettings {
            channel: 12,
            serial_enabled: true,
            ble_enabled: false,
            paired_peer: None,
        };
        assert_eq!(
            controller.handle(AppCommand::UpdateMidi(midi)),
            AppEffect::PersistSettings(Settings {
                midi,
                ..Settings::default()
            })
        );
        assert_eq!(controller.snapshot().settings.midi, midi);
    }

    #[test]
    fn skin_override_persists_without_requiring_a_radio_restart() {
        let mut controller = AppController::default();
        let preset = PresetIndex::new(4).expect("preset 4 exists");
        let selection = SkinSelection::Specific(tonex_skins::SkinId::MESA_DUAL);
        let effect = controller.handle(AppCommand::SetPresetSkin { preset, selection });
        let AppEffect::PersistSettingsLive(settings) = effect else {
            panic!("skin changes must use live persistence");
        };
        assert_eq!(settings.preset_skins.get(preset), selection);
        assert_eq!(
            controller.snapshot().settings.preset_skins.get(preset),
            selection
        );
    }

    #[test]
    fn brightness_is_clamped_and_persisted_as_a_live_setting() {
        let mut controller = AppController::default();
        let AppEffect::PersistSettingsLive(settings) =
            controller.handle(AppCommand::UpdateBrightness(140))
        else {
            panic!("brightness changes must use live persistence");
        };
        assert_eq!(settings.display_brightness_percent, 100);
        assert_eq!(
            controller.snapshot().settings.display_brightness_percent,
            100
        );
    }
}
