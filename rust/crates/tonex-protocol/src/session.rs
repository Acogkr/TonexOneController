use alloc::vec::Vec;

use crate::{
    HELLO_REQUEST, MessageType, STATE_REQUEST, encode_frame, encode_preset_details_request,
    message::encode_preset_details_request_unchecked,
};

const PRESET_COUNT: u8 = 20;
const LEGACY_BOOT_SENTINEL_PRESET: u8 = PRESET_COUNT;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionState {
    Disconnected,
    AwaitingHello,
    AwaitingState,
    SynchronizingPresets { requested: u8 },
    AwaitingCurrentPreset { preset: u8 },
    AwaitingGlobalVolume,
    Ready,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionAction {
    None,
    Transmit(Vec<u8>),
    StorePresetName { preset: u8 },
    StoreCurrentPresetDetails { preset: u8 },
    Continue,
    SynchronizationComplete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionError {
    UnexpectedMessage {
        state: SessionState,
        message_type: MessageType,
    },
    InvalidCurrentPreset(u8),
}

#[derive(Debug)]
pub struct Session {
    state: SessionState,
    current_preset: Option<u8>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            state: SessionState::Disconnected,
            current_preset: None,
        }
    }
}

impl Session {
    #[must_use]
    pub const fn state(&self) -> SessionState {
        self.state
    }

    #[must_use]
    pub fn connect(&mut self) -> SessionAction {
        self.state = SessionState::AwaitingHello;
        self.current_preset = None;
        SessionAction::Transmit(encode_frame(HELLO_REQUEST))
    }

    pub fn disconnect(&mut self) {
        self.state = SessionState::Disconnected;
        self.current_preset = None;
    }

    /// Completes boot synchronization after current-preset details are
    /// stored. The legacy controller marks itself ready at this point and
    /// treats the separately requested master-volume response as a later
    /// cache update rather than a connection prerequisite.
    pub fn complete_after_current_preset(&mut self) {
        if self.state == SessionState::AwaitingGlobalVolume {
            self.state = SessionState::Ready;
        }
    }

    /// Supplies the current preset obtained from a validated state update.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::InvalidCurrentPreset`] for values above 19.
    pub fn set_current_preset(&mut self, preset: u8) -> Result<(), SessionError> {
        if preset >= PRESET_COUNT {
            return Err(SessionError::InvalidCurrentPreset(preset));
        }
        self.current_preset = Some(preset);
        Ok(())
    }

    /// Advances the boot synchronization state machine.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::UnexpectedMessage`] when a response does not
    /// match the request currently in flight.
    pub fn receive(&mut self, message_type: MessageType) -> Result<SessionAction, SessionError> {
        match (self.state, message_type) {
            (SessionState::AwaitingHello, MessageType::Hello) => {
                self.state = SessionState::AwaitingState;
                Ok(SessionAction::Transmit(encode_frame(STATE_REQUEST)))
            }
            (SessionState::AwaitingState, MessageType::StateUpdate) => {
                self.state = SessionState::SynchronizingPresets { requested: 0 };
                Ok(SessionAction::Transmit(
                    encode_preset_details_request_unchecked(0),
                ))
            }
            (SessionState::SynchronizingPresets { requested }, MessageType::PresetDetails) => {
                if requested < PRESET_COUNT {
                    self.state = SessionState::SynchronizingPresets {
                        requested: requested + 1,
                    };
                    Ok(SessionAction::StorePresetName { preset: requested })
                } else if requested == LEGACY_BOOT_SENTINEL_PRESET {
                    let current = self.current_preset.unwrap_or(0);
                    self.state = SessionState::AwaitingCurrentPreset { preset: current };
                    Ok(SessionAction::Continue)
                } else {
                    Err(SessionError::UnexpectedMessage {
                        state: self.state,
                        message_type,
                    })
                }
            }
            (SessionState::AwaitingCurrentPreset { preset }, MessageType::PresetDetails) => {
                self.state = SessionState::AwaitingGlobalVolume;
                Ok(SessionAction::StoreCurrentPresetDetails { preset })
            }
            (SessionState::AwaitingGlobalVolume, MessageType::ParameterChanged) => {
                self.state = SessionState::Ready;
                Ok(SessionAction::SynchronizationComplete)
            }
            // TONEX ONE emits live parameter notifications independently of
            // the boot synchronization request/response sequence. The legacy
            // controller accepts and skips these messages while waiting for a
            // requested response; treating one as fatal causes an endless USB
            // close/reopen loop whenever a knob or footswitch is used.
            (state, MessageType::ParameterChanged)
                if state != SessionState::AwaitingGlobalVolume =>
            {
                Ok(SessionAction::None)
            }
            // Changing presets on the pedal emits unsolicited full state
            // updates. They are valid live notifications even when they
            // arrive between two boot-sync responses; only AwaitingState uses
            // one to advance the request sequence.
            (state, MessageType::StateUpdate) if state != SessionState::AwaitingState => {
                Ok(SessionAction::None)
            }
            (_, MessageType::PresetDetailsFull | MessageType::Unknown(_)) => {
                Ok(SessionAction::None)
            }
            (SessionState::Ready, _) => Ok(SessionAction::None),
            (state, message_type) => Err(SessionError::UnexpectedMessage {
                state,
                message_type,
            }),
        }
    }

    /// Returns the next request after an action that stores received data.
    #[must_use]
    pub fn continuation(&self) -> Option<Vec<u8>> {
        match self.state {
            SessionState::SynchronizingPresets { requested } => {
                Some(encode_preset_details_request_unchecked(requested))
            }
            SessionState::AwaitingCurrentPreset { preset } => {
                encode_preset_details_request(preset).ok()
            }
            SessionState::AwaitingGlobalVolume => Some(encode_frame(&[
                0xb9, 0x03, 0x81, 0x0d, 0x03, 0x82, 0x05, 0x00, 0x80, 0x0b, 0x03, 0xb9, 0x03, 0x03,
                0x00, 0x00,
            ])),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode_frame;

    #[test]
    fn complete_boot_sync_matches_legacy_sentinel_request() {
        let mut session = Session::default();
        assert!(matches!(session.connect(), SessionAction::Transmit(_)));
        session
            .receive(MessageType::Hello)
            .expect("hello is expected");
        session
            .set_current_preset(7)
            .expect("current preset is valid");
        session
            .receive(MessageType::StateUpdate)
            .expect("state update is expected");

        for preset in 0..PRESET_COUNT {
            let action = session
                .receive(MessageType::PresetDetails)
                .expect("preset details are expected");
            assert_eq!(action, SessionAction::StorePresetName { preset });
            let request = session
                .continuation()
                .expect("another synchronization request is required");
            let decoded = decode_frame(&request).expect("request must be framed correctly");
            assert_eq!(decoded[15], preset + 1);
        }

        assert_eq!(
            session.state(),
            SessionState::SynchronizingPresets {
                requested: LEGACY_BOOT_SENTINEL_PRESET
            }
        );
        assert_eq!(
            session
                .receive(MessageType::PresetDetails)
                .expect("legacy sentinel response is expected"),
            SessionAction::Continue
        );
        let request = session
            .continuation()
            .expect("current preset request is required");
        let decoded = decode_frame(&request).expect("request must be framed correctly");
        assert_eq!(decoded[15], 7);
        assert_eq!(
            session.state(),
            SessionState::AwaitingCurrentPreset { preset: 7 }
        );
        assert_eq!(
            session
                .receive(MessageType::PresetDetails)
                .expect("current details are expected"),
            SessionAction::StoreCurrentPresetDetails { preset: 7 }
        );
        assert!(session.continuation().is_some());
        assert_eq!(
            session
                .receive(MessageType::ParameterChanged)
                .expect("global volume is expected"),
            SessionAction::SynchronizationComplete
        );
        assert_eq!(session.state(), SessionState::Ready);
    }

    #[test]
    fn disconnect_resets_in_flight_state() {
        let mut session = Session::default();
        let _ = session.connect();
        session.disconnect();
        assert_eq!(session.state(), SessionState::Disconnected);
        assert_eq!(
            session.receive(MessageType::Hello),
            Err(SessionError::UnexpectedMessage {
                state: SessionState::Disconnected,
                message_type: MessageType::Hello,
            })
        );
    }

    #[test]
    fn unsolicited_state_updates_do_not_interrupt_preset_synchronization() {
        let mut session = Session::default();
        let _ = session.connect();
        session
            .receive(MessageType::Hello)
            .expect("hello advances to state request");
        session
            .set_current_preset(7)
            .expect("current preset is valid");
        session
            .receive(MessageType::StateUpdate)
            .expect("requested state starts preset synchronization");

        assert_eq!(
            session.receive(MessageType::StateUpdate),
            Ok(SessionAction::None)
        );
        assert_eq!(
            session.state(),
            SessionState::SynchronizingPresets { requested: 0 }
        );
    }

    #[test]
    fn asynchronous_parameter_change_does_not_break_boot_sync() {
        let mut session = Session::default();
        let _ = session.connect();
        assert_eq!(
            session.receive(MessageType::ParameterChanged),
            Ok(SessionAction::None)
        );
        assert_eq!(session.state(), SessionState::AwaitingHello);
        assert!(matches!(
            session.receive(MessageType::Hello),
            Ok(SessionAction::Transmit(_))
        ));
    }

    #[test]
    fn current_preset_can_complete_before_optional_global_volume_response() {
        let mut session = Session {
            state: SessionState::AwaitingGlobalVolume,
            current_preset: Some(3),
        };
        session.complete_after_current_preset();
        assert_eq!(session.state(), SessionState::Ready);
    }
}
