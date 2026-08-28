#![cfg_attr(not(target_os = "espidf"), no_std)]
#![cfg_attr(target_os = "espidf", allow(unsafe_code))]

#[cfg(target_os = "espidf")]
mod serial {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel},
    };

    use tonex_boards::BoardDescriptor;
    use tonex_midi::{MidiAction, MidiChannel, MidiStreamDecoder};

    const BUFFER_SIZE: usize = 128;
    const QUEUE_DEPTH: usize = 16;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum MidiStartError {
        MissingBoardUart,
        InvalidChannel,
        InvalidBaudRate,
        InvalidBufferSize,
        ThreadStart,
        Esp(i32),
    }

    impl core::fmt::Display for MidiStartError {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self {
                Self::MissingBoardUart => formatter.write_str("board has no Serial MIDI UART"),
                Self::InvalidChannel => formatter.write_str("MIDI channel must be in 1..=16"),
                Self::InvalidBaudRate => formatter.write_str("invalid UART baud rate"),
                Self::InvalidBufferSize => formatter.write_str("invalid UART buffer size"),
                Self::ThreadStart => formatter.write_str("failed to start Serial MIDI task"),
                Self::Esp(code) => write!(formatter, "ESP-IDF UART error {code}"),
            }
        }
    }

    pub struct SerialMidi {
        receiver: Receiver<MidiAction>,
        dropped_actions: Arc<AtomicUsize>,
    }

    impl core::fmt::Debug for SerialMidi {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.debug_struct("SerialMidi").finish_non_exhaustive()
        }
    }

    impl SerialMidi {
        pub fn try_recv(&self) -> Result<MidiAction, TryRecvError> {
            self.receiver.try_recv()
        }

        #[must_use]
        pub fn dropped_actions(&self) -> usize {
            self.dropped_actions.load(Ordering::Relaxed)
        }
    }

    pub fn start(
        board: &'static BoardDescriptor,
        channel: u8,
    ) -> Result<SerialMidi, MidiStartError> {
        let hardware = board
            .hardware
            .serial_midi
            .ok_or(MidiStartError::MissingBoardUart)?;
        let channel = MidiChannel::new(channel).map_err(|_| MidiStartError::InvalidChannel)?;
        install_uart(hardware)?;
        let (sender, receiver) = sync_channel(QUEUE_DEPTH);
        let dropped_actions = Arc::new(AtomicUsize::new(0));
        let task_dropped_actions = Arc::clone(&dropped_actions);
        std::thread::Builder::new()
            .name("serial-midi".into())
            .stack_size(3 * 1_024)
            .spawn(move || {
                receive_loop(hardware.uart_port, channel, sender, &task_dropped_actions);
            })
            .map_err(|_| MidiStartError::ThreadStart)?;
        Ok(SerialMidi {
            receiver,
            dropped_actions,
        })
    }

    fn install_uart(hardware: tonex_boards::SerialMidiHardware) -> Result<(), MidiStartError> {
        let port = u32::from(hardware.uart_port);
        let mut config = esp_idf_sys::uart_config_t::default();
        config.baud_rate =
            i32::try_from(hardware.baud_rate).map_err(|_| MidiStartError::InvalidBaudRate)?;
        config.data_bits = esp_idf_sys::uart_word_length_t_UART_DATA_8_BITS;
        config.parity = esp_idf_sys::uart_parity_t_UART_PARITY_DISABLE;
        config.stop_bits = esp_idf_sys::uart_stop_bits_t_UART_STOP_BITS_1;
        config.flow_ctrl = esp_idf_sys::uart_hw_flowcontrol_t_UART_HW_FLOWCTRL_DISABLE;
        config.__bindgen_anon_1.source_clk =
            esp_idf_sys::soc_periph_uart_clk_src_legacy_t_UART_SCLK_DEFAULT;

        // SAFETY: UART1 is selected by immutable board metadata; the driver
        // copies this configuration and does not retain any Rust reference.
        check(unsafe {
            esp_idf_sys::uart_driver_install(
                port,
                i32::try_from(BUFFER_SIZE * 2).map_err(|_| MidiStartError::InvalidBufferSize)?,
                0,
                0,
                core::ptr::null_mut(),
                0,
            )
        })?;
        // SAFETY: `config` is initialized for the duration of this blocking
        // call and ESP-IDF does not retain its pointer.
        check(unsafe { esp_idf_sys::uart_param_config(port, &config) })?;
        let tx = hardware.tx_gpio.map_or(-1, i32::from);
        // SAFETY: all GPIO numbers come from the compile-time board descriptor
        // and the UART driver is installed before pins are assigned.
        check(unsafe { esp_idf_sys::uart_set_pin(port, tx, i32::from(hardware.rx_gpio), -1, -1) })?;
        // SAFETY: the descriptor's RX GPIO is a valid direct MCU pin and stays
        // assigned to this UART for the firmware lifetime.
        check(unsafe { esp_idf_sys::gpio_pullup_en(i32::from(hardware.rx_gpio)) })?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        // SAFETY: the UART driver owns its RX ring and no other project code
        // consumes UART1.
        check(unsafe { esp_idf_sys::uart_flush_input(port) })
    }

    fn receive_loop(
        port: u8,
        channel: MidiChannel,
        sender: SyncSender<MidiAction>,
        dropped_actions: &AtomicUsize,
    ) {
        use std::time::{Duration, Instant};

        let mut decoder = MidiStreamDecoder::default();
        let mut buffer = [0_u8; BUFFER_SIZE];
        let started = Instant::now();
        let mut next_diagnostics = Duration::from_secs(60);
        loop {
            // SAFETY: `buffer` is writable for its full declared length and
            // remains borrowed until the blocking read returns.
            let received = unsafe {
                esp_idf_sys::uart_read_bytes(
                    u32::from(port),
                    buffer.as_mut_ptr().cast(),
                    u32::try_from(buffer.len()).expect("MIDI buffer length fits u32"),
                    5,
                )
            };
            let Ok(received) = usize::try_from(received) else {
                continue;
            };
            for byte in &buffer[..received] {
                if let Ok(Some(action)) = decoder.push(*byte, channel) {
                    match sender.try_send(action) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => {
                            dropped_actions.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(TrySendError::Disconnected(_)) => return,
                    }
                }
            }
            let elapsed = started.elapsed();
            if elapsed >= next_diagnostics {
                println!(
                    "DIAG_TASK name=serial-midi uptime_s={} stack_hwm={}",
                    elapsed.as_secs(),
                    esp_idf_diagnostics::current_task_stack_high_water_bytes()
                );
                next_diagnostics = elapsed + Duration::from_secs(60);
            }
        }
    }

    const fn check(code: i32) -> Result<(), MidiStartError> {
        if code == esp_idf_sys::ESP_OK {
            Ok(())
        } else {
            Err(MidiStartError::Esp(code))
        }
    }
}

#[cfg(target_os = "espidf")]
pub use serial::{MidiStartError, SerialMidi, start};

#[cfg(all(target_os = "espidf", feature = "ble-midi"))]
mod ble;
#[cfg(all(target_os = "espidf", feature = "ble-midi"))]
pub use ble::{BleCandidate, BleConnectionState, BleInput, BleMidi, BleStartError, start_ble};
