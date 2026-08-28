use tonex_boards::REGISTRY;

#[cfg(any(target_os = "espidf", test))]
const RECONNECT_DISPLAY_GRACE: std::time::Duration = std::time::Duration::from_secs(3);
#[cfg(any(target_os = "espidf", test))]
const INITIAL_READY_DISPLAY_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

#[cfg(target_os = "espidf")]
mod nvs;
#[cfg(all(target_os = "espidf", feature = "wifi-web"))]
mod web;

#[cfg(not(target_os = "espidf"))]
fn main() {
    println!(
        "TONEX ONE host composition: {} registered build variants",
        REGISTRY.len()
    );
}

#[cfg(target_os = "espidf")]
fn main() {
    use std::time::Duration;

    esp_idf_sys::link_patches();
    let board = selected_board();
    #[cfg(feature = "touch-diagnostics")]
    run_touch_diagnostics(board);
    #[cfg(feature = "display-diagnostics")]
    run_display_diagnostics(board);
    #[cfg(feature = "ui-diagnostics")]
    run_ui_display_diagnostics(board);
    let settings = load_settings();
    let mut i2c_buses = match esp_idf_i2c::EspI2cBuses::new(board.hardware) {
        Ok(buses) => Some(buses),
        Err(error) => {
            println!("I2C bus initialization failed: {error:?}");
            None
        }
    };
    let mut initial = initial_ui_snapshot(board, &settings);
    initial.touch_ready = false;
    #[cfg(feature = "usb-connection-diagnostics")]
    {
        initial.preset_label = tonex_ui_model::PresetLabel::from_bytes(b"WAITING FOR TONEX ONE");
    }
    // Do not let Wi-Fi/BLE consume heap until the display has allocated its
    // buffers and completed the first real frame. The JC3248W535 physical run
    // proved that racing radio startup can leave an otherwise healthy panel
    // black even though its driver initialized successfully.
    let mut display = start_display_task(board, initial);
    // AXS15231B display initialization resets the combined display/touch
    // controller. Start touch only after that reset and the first frame. The
    // Stage pixels do not depend on touch_ready, so acknowledge the actual
    // state without scheduling a redundant identical full-screen transfer.
    #[cfg(not(feature = "touch-disabled-diagnostics"))]
    let mut touch = i2c_buses
        .as_ref()
        .and_then(|buses| start_touch_task(board, buses));
    #[cfg(feature = "touch-disabled-diagnostics")]
    let mut touch: Option<TouchInput> = None;
    initial.touch_ready = touch.is_some();
    let initial_displayed = display.as_ref().map(|_| initial);
    // Reserve the USB host's DMA/internal-memory resources before starting
    // Wi-Fi and the HTTP server. On ESP32-S3, bringing the radio stack up
    // first can leave the TONEX host unable to enumerate even though the Web
    // UI itself remains reachable.
    let usb = loop {
        match esp_idf_usb_host::TonexUsbStack::install(Default::default()) {
            Ok(usb) => break usb,
            Err(error) => {
                println!("USB Host initialization failed: {}", error.0);
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    };
    #[cfg(any(feature = "wifi-web", feature = "ble-midi"))]
    let peripherals = match esp_idf_svc::hal::peripherals::Peripherals::take() {
        Ok(peripherals) => peripherals,
        Err(error) => {
            println!("Radio peripheral acquisition failed: {error}");
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    };
    #[cfg(all(feature = "wifi-web", feature = "ble-midi"))]
    let (wifi_modem, ble_modem) = peripherals.modem.split();
    #[cfg(all(feature = "wifi-web", not(feature = "ble-midi")))]
    let wifi_modem = peripherals.modem;
    #[cfg(all(feature = "ble-midi", not(feature = "wifi-web")))]
    let (_, ble_modem) = peripherals.modem.split();
    #[cfg(feature = "wifi-web")]
    let (mut web, settings) = if settings.wifi.enabled {
        match web::WebControl::start(wifi_modem, settings) {
            Ok(started) => {
                let mut effective_settings = settings;
                effective_settings.wifi = started.effective_wifi();
                (Some(started), effective_settings)
            }
            Err(error) => {
                println!("Wi-Fi/Web initialization failed: {error}");
                (None, settings)
            }
        }
    } else {
        (None, settings)
    };
    // Do not initialize Bluedroid before TONEX ONE has enumerated and synced.
    // On memory-constrained display boards that ordering lets the Bluetooth
    // controller claim internal DMA memory needed by USB Host and the LCD.
    #[cfg(feature = "ble-midi")]
    let mut ble_midi = None;
    println!(
        "TONEX ONE firmware start: {} ({}), preset {}, MIDI channel {}",
        board.id,
        board.hardware.id,
        settings.selected_preset.get(),
        settings.midi.channel
    );

    run_controller(
        usb,
        board,
        settings,
        display.take(),
        initial_displayed,
        i2c_buses.take(),
        touch.take(),
        #[cfg(feature = "wifi-web")]
        web.take(),
        #[cfg(feature = "ble-midi")]
        Some(ble_modem),
        #[cfg(feature = "ble-midi")]
        ble_midi.take(),
    )
}

#[cfg(all(target_os = "espidf", feature = "touch-diagnostics"))]
fn run_touch_diagnostics(board: &'static tonex_boards::BoardDescriptor) -> ! {
    use std::time::Duration;

    let spawn = std::thread::Builder::new()
        .name("touch-diagnostic".into())
        .stack_size(64 * 1024)
        .spawn(move || touch_diagnostics_task(board));
    if let Err(error) = spawn {
        println!("TOUCH_DIAG task=spawn_error detail={error}");
    }
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(all(target_os = "espidf", feature = "touch-diagnostics"))]
fn touch_diagnostics_task(board: &'static tonex_boards::BoardDescriptor) -> ! {
    use std::time::Duration;

    let buses = match esp_idf_i2c::EspI2cBuses::new(board.hardware) {
        Ok(buses) => buses,
        Err(error) => {
            println!("TOUCH_DIAG i2c=error detail={error:?}");
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    };
    let mut snapshot = initial_ui_snapshot(board, &tonex_settings::Settings::default());
    snapshot.touch_ready = false;
    let display = start_display_task(board, snapshot);
    let touch = start_touch_task(board, &buses);
    snapshot.touch_ready = touch.is_some();
    let mut page = tonex_ui_model::UiPage::Stage;
    println!(
        "TOUCH_DIAG ready={} display={} tap_gear_now=true",
        touch.is_some(),
        display.is_some()
    );
    let Some(touch) = touch else {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    };
    loop {
        match touch.try_recv() {
            Ok(action) => {
                println!("TOUCH_DIAG action={action:?} page_before={page:?}");
                if let tonex_controls::TouchAction::Tap { x, y } = action {
                    let _ = handle_ui_tap(&mut page, &snapshot, board, x, y);
                    snapshot.page = page;
                    if let Some(sender) = display.as_ref() {
                        let _ = sender.try_send(snapshot);
                    }
                    println!("TOUCH_DIAG tap=({x},{y}) page_after={page:?}");
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                println!("TOUCH_DIAG task=disconnected");
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[cfg(all(target_os = "espidf", feature = "ui-diagnostics"))]
fn run_ui_display_diagnostics(board: &'static tonex_boards::BoardDescriptor) -> ! {
    use std::time::Duration;

    let spawn = std::thread::Builder::new()
        .name("ui-diagnostic".into())
        .stack_size(64 * 1024)
        .spawn(move || {
            use hal_contracts::DisplayDevice;

            let mut display = match esp_idf_display::EspDisplay::new(board) {
                Ok(display) => display,
                Err(error) => {
                    println!("UI_DIAG init=error detail={error:?}");
                    signal_display_diagnostic_failure(board, Duration::from_secs(2));
                }
            };
            let Some(spec) = board.display else {
                signal_display_diagnostic_failure(board, Duration::from_secs(2));
            };
            let snapshot = initial_ui_snapshot(board, &tonex_settings::Settings::default());
            let model = tonex_ui_model::UiViewModel::new(spec.ui_class, snapshot).with_touch(
                board
                    .capabilities
                    .contains(tonex_boards::Capabilities::TOUCH),
            );
            if let Err(error) = tonex_ui_renderer::render(&model, display.frame_buffer()) {
                println!("UI_DIAG render=error detail={error:?}");
                signal_display_diagnostic_failure(board, Duration::from_millis(250));
            }
            if let Err(error) = display.present() {
                println!("UI_DIAG present=error detail={error:?}");
                signal_display_diagnostic_failure(board, Duration::from_millis(250));
            }
            println!("UI_DIAG ready");
            #[cfg(feature = "usb-display-diagnostics")]
            let _usb = {
                std::thread::sleep(Duration::from_secs(5));
                println!("UI_DIAG usb_host_start");
                match esp_idf_usb_host::TonexUsbStack::install(Default::default()) {
                    Ok(usb) => {
                        println!("UI_DIAG usb_host=ok");
                        Some(usb)
                    }
                    Err(error) => {
                        println!("UI_DIAG usb_host=error code={}", error.0);
                        None
                    }
                }
            };
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    if let Err(error) = spawn {
        println!("UI diagnostic task creation failed: {error}");
        signal_display_diagnostic_failure(board, Duration::from_millis(250));
    }
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(any(target_os = "espidf", test))]
fn initial_ui_snapshot(
    board: &'static tonex_boards::BoardDescriptor,
    settings: &tonex_settings::Settings,
) -> tonex_ui_model::UiSnapshot {
    #[cfg(feature = "ble-midi")]
    let bluetooth = if settings.midi.paired_peer.is_some() {
        tonex_ui_model::BluetoothState::Searching
    } else {
        tonex_ui_model::BluetoothState::Off
    };
    #[cfg(not(feature = "ble-midi"))]
    let bluetooth = tonex_ui_model::BluetoothState::Off;
    let mut snapshot = tonex_ui_model::UiSnapshot {
        preset: settings.selected_preset,
        slot: settings.selected_slot,
        display_brightness_percent: settings.display_brightness_percent,
        wifi: settings.wifi,
        board_label: tonex_ui_model::PresetLabel::from_bytes(board.id.as_bytes()),
        brightness_dimmable: board.hardware.backlight.is_some(),
        bluetooth,
        ..tonex_ui_model::UiSnapshot::default()
    };
    if let Some(display) = board.display {
        snapshot.display_width = display.width;
        snapshot.display_height = display.height;
    }
    snapshot
}

#[cfg(all(target_os = "espidf", feature = "display-diagnostics"))]
fn run_display_diagnostics(board: &'static tonex_boards::BoardDescriptor) -> ! {
    use std::time::Duration;

    use hal_contracts::DisplayDevice;

    println!("DISPLAY_DIAG start board={}", board.id);
    let _ = esp_idf_display::enable_diagnostic_backlight(board);
    let mut display = match esp_idf_display::EspDisplay::new(board) {
        Ok(display) => {
            println!("DISPLAY_DIAG init=ok dimensions={:?}", display.dimensions());
            display
        }
        Err(error) => {
            println!("DISPLAY_DIAG init=error detail={error:?}");
            signal_display_diagnostic_failure(board, Duration::from_secs(2));
        }
    };
    let (width, height) = display.dimensions();
    let width = usize::from(width);
    let height = usize::from(height);
    for (index, pixel) in display.frame_buffer().iter_mut().enumerate() {
        let x = index % width;
        *pixel = if x < width / 3 {
            0xf800
        } else if x < (width * 2) / 3 {
            0x07e0
        } else {
            0x001f
        };
    }
    println!("DISPLAY_DIAG frame=ready pixels={}", width * height);
    match display.present() {
        Ok(()) => println!("DISPLAY_DIAG present=ok"),
        Err(error) => {
            println!("DISPLAY_DIAG present=error detail={error:?}");
            signal_display_diagnostic_failure(board, Duration::from_millis(250));
        }
    }
    std::thread::sleep(Duration::from_secs(2));
    let Some(spec) = board.display else {
        signal_display_diagnostic_failure(board, Duration::from_millis(250));
    };
    let snapshot = initial_ui_snapshot(board, &tonex_settings::Settings::default());
    let model = tonex_ui_model::UiViewModel::new(spec.ui_class, snapshot).with_touch(
        board
            .capabilities
            .contains(tonex_boards::Capabilities::TOUCH),
    );
    match tonex_ui_renderer::render(&model, display.frame_buffer()) {
        Ok(()) => println!("DISPLAY_DIAG ui_render=ok"),
        Err(error) => {
            println!("DISPLAY_DIAG ui_render=error detail={error:?}");
            signal_display_diagnostic_failure(board, Duration::from_millis(250));
        }
    }
    match display.present() {
        Ok(()) => println!("DISPLAY_DIAG ui_present=ok"),
        Err(error) => {
            println!("DISPLAY_DIAG ui_present=error detail={error:?}");
            signal_display_diagnostic_failure(board, Duration::from_millis(250));
        }
    }
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(all(
    target_os = "espidf",
    any(feature = "display-diagnostics", feature = "ui-diagnostics")
))]
fn signal_display_diagnostic_failure(
    board: &'static tonex_boards::BoardDescriptor,
    interval: std::time::Duration,
) -> ! {
    loop {
        let _ = esp_idf_display::set_diagnostic_backlight(board, true);
        std::thread::sleep(interval);
        let _ = esp_idf_display::set_diagnostic_backlight(board, false);
        std::thread::sleep(interval);
    }
}

#[cfg(target_os = "espidf")]
fn load_settings() -> tonex_settings::Settings {
    use hal_contracts::SettingsStore;
    use tonex_settings::{CURRENT_SCHEMA_VERSION, ENCODED_LEN, Settings, decode_and_migrate};

    let mut store = match nvs::EspNvsSettingsStore::open() {
        Ok(store) => store,
        Err(error) => {
            println!("NVS initialization failed, using defaults: {error}");
            return Settings::default();
        }
    };
    let mut encoded = [0_u8; ENCODED_LEN];
    let length = match store.load(&mut encoded) {
        Ok(length) => length,
        Err(error) => {
            println!("Settings read failed, using defaults: {error}");
            return Settings::default();
        }
    };
    let settings = if length == 0 {
        Settings::default()
    } else {
        match decode_and_migrate(&encoded[..length]) {
            Ok(settings) => settings,
            Err(error) => {
                println!("Stored settings are invalid, restoring defaults: {error:?}");
                Settings::default()
            }
        }
    };
    let needs_rewrite = length == 0
        || length != ENCODED_LEN
        || encoded.get(2).copied() != Some(CURRENT_SCHEMA_VERSION);
    if needs_rewrite && let Err(error) = store.save(&settings.encode()) {
        println!("Settings migration/default save failed: {error}");
    }
    settings
}

#[cfg(target_os = "espidf")]
fn selected_board() -> &'static tonex_boards::BoardDescriptor {
    let board_id = env!("TONEX_BOARD_ID");
    REGISTRY
        .iter()
        .find(|candidate| candidate.id == board_id)
        .unwrap_or_else(|| panic!("build selected unregistered board: {board_id}"))
}

#[cfg(target_os = "espidf")]
#[derive(Default)]
struct RuntimeDiagnostics {
    command_not_ready: usize,
    command_errors: usize,
    midi_mapping_errors: usize,
    switch_read_errors: usize,
    led_errors: usize,
}

#[cfg(target_os = "espidf")]
impl RuntimeDiagnostics {
    fn record_command<E: core::fmt::Debug>(
        &mut self,
        source: &'static str,
        result: Result<tonex_application::AppEffect, tonex_application::RuntimeError<E>>,
    ) {
        match result {
            Ok(_) => {}
            Err(tonex_application::RuntimeError::NotReady(_)) => {
                self.command_not_ready = self.command_not_ready.saturating_add(1);
            }
            Err(error) => {
                self.command_errors = self.command_errors.saturating_add(1);
                if self.command_errors.is_power_of_two() {
                    println!(
                        "{source} command failed ({count} total): {error:?}",
                        count = self.command_errors
                    );
                }
            }
        }
    }

    #[cfg(any(feature = "serial-midi", feature = "ble-midi"))]
    fn record_midi_mapping_error(&mut self, error: tonex_midi::MidiError) {
        self.midi_mapping_errors = self.midi_mapping_errors.saturating_add(1);
        if self.midi_mapping_errors.is_power_of_two() {
            println!(
                "MIDI action mapping failed ({count} total): {error:?}",
                count = self.midi_mapping_errors
            );
        }
    }
}

#[cfg(target_os = "espidf")]
#[cfg(feature = "ble-midi")]
fn bluetooth_ui_state(ble: Option<&esp_idf_midi::BleMidi>) -> tonex_ui_model::BluetoothState {
    match ble.map(esp_idf_midi::BleMidi::connection_state) {
        Some(esp_idf_midi::BleConnectionState::Connected) => {
            tonex_ui_model::BluetoothState::Connected
        }
        Some(esp_idf_midi::BleConnectionState::Searching) => {
            tonex_ui_model::BluetoothState::Searching
        }
        Some(esp_idf_midi::BleConnectionState::Idle) => tonex_ui_model::BluetoothState::Off,
        None => tonex_ui_model::BluetoothState::Off,
    }
}

#[cfg(target_os = "espidf")]
fn run_controller(
    usb: esp_idf_usb_host::TonexUsbStack,
    board: &'static tonex_boards::BoardDescriptor,
    settings: tonex_settings::Settings,
    mut display: Option<std::sync::mpsc::SyncSender<tonex_ui_model::UiSnapshot>>,
    initial_displayed: Option<tonex_ui_model::UiSnapshot>,
    i2c_buses: Option<esp_idf_i2c::EspI2cBuses>,
    mut touch: Option<TouchInput>,
    #[cfg(feature = "wifi-web")] mut web: Option<web::WebControl>,
    #[cfg(feature = "ble-midi")] mut ble_modem: Option<
        esp_idf_svc::hal::modem::BluetoothModem<'static>,
    >,
    #[cfg(feature = "ble-midi")] mut ble_midi: Option<esp_idf_midi::BleMidi>,
) -> ! {
    use std::time::Duration;
    use std::time::Instant;

    use esp_idf_board_io::{EspBoardSwitches, EspLedStrip, EspLp5562Backlight};
    use hal_contracts::{LedDevice, SwitchDevice};
    use tonex_application::{AppCommand, AppEffect, TonexRuntime};
    use tonex_controls::SwitchDebouncer;
    use tonex_domain::ConnectionState;

    const OPEN_RETRY_TICKS: u16 = 500;
    const USB_RX_CHUNKS_PER_TICK: usize = 8;
    const USB_RX_CHUNK_BYTES: usize = 8_192;
    #[cfg(any(feature = "serial-midi", feature = "ble-midi"))]
    const MIDI_ACTIONS_PER_TICK: usize = 8;
    let started = Instant::now();
    let mut next_diagnostics = Duration::from_secs(60);
    #[cfg(not(feature = "touch-disabled-diagnostics"))]
    let mut next_touch_retry = Duration::from_secs(5);
    #[cfg(feature = "ble-midi")]
    let mut ble_auto_start_pending = settings.midi.paired_peer.is_some();
    #[cfg(feature = "ble-midi")]
    let mut ble_scan_pending = false;
    let mut runtime = TonexRuntime::new_with_settings(usb, settings);
    let mut diagnostics = RuntimeDiagnostics::default();
    let mut retry_ticks = 0_u16;
    #[cfg(feature = "usb-connection-diagnostics")]
    let mut usb_handshake_failures = 0_u16;
    #[cfg(feature = "usb-connection-diagnostics")]
    let mut next_usb_diagnostic_summary = Duration::ZERO;
    #[cfg(feature = "usb-connection-diagnostics")]
    let mut usb_diagnostic_summary = tonex_ui_model::PresetLabel::from_bytes(b"E0 O0 G0 H0 X0");
    let mut receive_buffer = vec![0_u8; USB_RX_CHUNK_BYTES];
    let mut auxiliary_backlight = board.hardware.backlight.and_then(|hardware| {
        let buses = i2c_buses.as_ref()?;
        match EspLp5562Backlight::new(hardware, buses) {
            Ok(mut backlight) => {
                if let Err(error) =
                    backlight.set_percent(runtime.snapshot().settings.display_brightness_percent)
                {
                    println!("Auxiliary backlight brightness failed: {error:?}");
                }
                Some(backlight)
            }
            Err(error) => {
                println!("Auxiliary backlight initialization failed: {error:?}");
                None
            }
        }
    });
    let mut switches = match EspBoardSwitches::new(board.hardware.controls, i2c_buses.as_ref()) {
        Ok(switches) => Some(switches),
        Err(error) => {
            println!("Footswitch initialization failed: {error:?}");
            None
        }
    };
    let mut switch_debouncer = SwitchDebouncer::<4>::new(20);
    #[cfg(feature = "ble-midi")]
    let mut ble_press_tracker = tonex_midi::FootPressTracker::default();
    let mut leds = board.leds.and_then(|spec| match EspLedStrip::new(spec) {
        Ok(leds) => Some(leds),
        Err(error) => {
            println!("LED initialization failed: {error:?}");
            None
        }
    });
    let mut displayed_connection = None;
    let mut displayed_ui = initial_displayed;
    let startup_display = initial_displayed;
    let mut last_ready_display = None;
    let mut previous_runtime_connection = runtime.snapshot().connection;
    let mut reconnect_display_started = None;
    let mut initial_ready_started = None;
    let mut ui_page = tonex_ui_model::UiPage::Stage;
    #[cfg(feature = "usb-connection-diagnostics")]
    let mut usb_status_label = Some(tonex_ui_model::PresetLabel::from_bytes(
        b"USB WAITING FOR TONEX",
    ));
    #[cfg(not(feature = "usb-connection-diagnostics"))]
    let mut usb_status_label: Option<tonex_ui_model::PresetLabel> = None;
    let mut next_touch_page_change = Duration::ZERO;
    #[cfg(feature = "wifi-web")]
    let mut published_ui = None;
    #[cfg(feature = "wifi-web")]
    let mut published_parameters = None;
    #[cfg(feature = "wifi-web")]
    let mut published_foot_controller = None;
    #[cfg(feature = "serial-midi")]
    let mut midi = if settings.midi.serial_enabled {
        match esp_idf_midi::start(board, settings.midi.channel) {
            Ok(receiver) => Some(receiver),
            Err(error) => {
                println!("Serial MIDI initialization failed: {error}");
                None
            }
        }
    } else {
        None
    };

    loop {
        let runtime_connection = runtime.snapshot().connection;
        if runtime_connection == ConnectionState::Ready {
            reconnect_display_started = None;
            if initial_ready_started.is_none() {
                initial_ready_started = Some(started.elapsed());
            }
        } else if previous_runtime_connection == ConnectionState::Ready {
            reconnect_display_started = Some(started.elapsed());
        } else if last_ready_display.is_none() {
            initial_ready_started = None;
        }
        previous_runtime_connection = runtime_connection;

        if runtime.snapshot().connection != ConnectionState::Ready && page_requires_tonex(ui_page) {
            ui_page = tonex_ui_model::UiPage::Stage;
        }
        let mut ui_snapshot = runtime.ui_snapshot();
        #[cfg(feature = "ble-midi")]
        {
            ui_snapshot.bluetooth = bluetooth_ui_state(ble_midi.as_ref());
        }
        #[cfg(not(feature = "ble-midi"))]
        {
            ui_snapshot.bluetooth = tonex_ui_model::BluetoothState::Off;
        }
        ui_snapshot.page = ui_page;
        if ui_snapshot.connection != ConnectionState::Ready
            && let Some(label) = usb_status_label
        {
            ui_snapshot.preset_label = label;
        }
        ui_snapshot.board_label = tonex_ui_model::PresetLabel::from_bytes(board.id.as_bytes());
        if let Some(spec) = board.display {
            ui_snapshot.display_width = spec.width;
            ui_snapshot.display_height = spec.height;
        }
        ui_snapshot.touch_ready = touch.is_some();
        ui_snapshot.brightness_dimmable = board.hardware.backlight.is_some();
        let reconnect_elapsed = reconnect_display_started
            .map(|started_at| started.elapsed().saturating_sub(started_at));
        let initial_ready_elapsed =
            initial_ready_started.map(|started_at| started.elapsed().saturating_sub(started_at));
        #[cfg(not(feature = "usb-connection-diagnostics"))]
        let display_snapshot = display_snapshot_with_startup_stability(
            &ui_snapshot,
            startup_display.as_ref(),
            last_ready_display.as_ref(),
            reconnect_elapsed,
            initial_ready_elapsed,
        );
        #[cfg(feature = "usb-connection-diagnostics")]
        let display_snapshot = ui_snapshot;
        if display_snapshot.connection == ConnectionState::Ready {
            last_ready_display = Some(display_snapshot);
        }
        if displayed_ui != Some(display_snapshot)
            && let Some(sender) = display.as_ref()
        {
            match sender.try_send(display_snapshot) {
                Ok(()) => displayed_ui = Some(display_snapshot),
                Err(std::sync::mpsc::TrySendError::Full(_)) => {}
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => display = None,
            }
        }

        #[cfg(feature = "wifi-web")]
        if let Some(web) = web.as_mut() {
            let snapshot = runtime.snapshot();
            let commands = coalesce_command_batch(
                (snapshot.selected_preset, snapshot.selected_slot),
                std::iter::from_fn(|| web.try_command()),
            );

            for command in commands {
                if command.requires_tonex()
                    && runtime.snapshot().connection != ConnectionState::Ready
                {
                    continue;
                }
                match runtime.apply_command(command) {
                    Ok(AppEffect::PersistSettings(settings)) => match persist_settings(settings) {
                        Ok(()) => esp_idf_svc::hal::reset::restart(),
                        Err(error) => println!("Settings save failed: {error}"),
                    },
                    Ok(AppEffect::PersistSettingsLive(settings)) => {
                        if let Err(error) = persist_settings(settings) {
                            println!("Settings save failed: {error}");
                        }
                        if let Some(backlight) = auxiliary_backlight.as_mut()
                            && let Err(error) =
                                backlight.set_percent(settings.display_brightness_percent)
                        {
                            println!("Auxiliary backlight brightness failed: {error:?}");
                        }
                    }
                    Ok(AppEffect::StartBluetoothScan) => {
                        #[cfg(feature = "ble-midi")]
                        {
                            if let Some(ble) = ble_midi.as_ref() {
                                if let Err(code) = ble.start_scan() {
                                    println!("BLE scan start failed: {code}");
                                }
                            } else {
                                // Defer radio allocation until TONEX USB has
                                // completed its handshake and owns its buffers.
                                ble_scan_pending = true;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(error) => println!("Web command failed: {error:?}"),
                }
            }
            let snapshot = runtime.ui_snapshot();
            #[cfg(feature = "ble-midi")]
            let snapshot = {
                let mut snapshot = snapshot;
                snapshot.bluetooth = bluetooth_ui_state(ble_midi.as_ref());
                if let Some(ble) = ble_midi.as_ref() {
                    snapshot.bluetooth_devices = ble.candidates().map(|candidate| {
                        candidate.map(|candidate| tonex_ui_model::BluetoothDevice {
                            address: candidate.address,
                            address_type: candidate.address_type,
                            name: tonex_ui_model::PresetLabel::from_bytes(
                                candidate.name().as_bytes(),
                            ),
                            rssi: candidate.rssi,
                        })
                    });
                }
                snapshot
            };
            #[cfg(not(feature = "ble-midi"))]
            let snapshot = {
                let mut snapshot = snapshot;
                snapshot.bluetooth = tonex_ui_model::BluetoothState::Off;
                snapshot
            };
            let parameters = runtime.parameter_values();
            let foot_controller = runtime.snapshot().settings.foot_controller;
            if published_ui != Some(snapshot)
                || published_parameters != Some(parameters)
                || published_foot_controller != Some(foot_controller)
            {
                let (snapshot_published, parameters_published, foot_controller_published) =
                    web.publish(snapshot, parameters, foot_controller);
                if snapshot_published {
                    published_ui = Some(snapshot);
                }
                if parameters_published {
                    published_parameters = Some(parameters);
                }
                if foot_controller_published {
                    published_foot_controller = Some(foot_controller);
                }
            }
        }

        #[cfg(feature = "serial-midi")]
        let mut serial_midi_commands = Vec::new();
        #[cfg(feature = "serial-midi")]
        for _ in 0..MIDI_ACTIONS_PER_TICK {
            match midi.as_ref().map(esp_idf_midi::SerialMidi::try_recv) {
                Some(Ok(action)) => {
                    if runtime.snapshot().connection != ConnectionState::Ready {
                        continue;
                    }
                    let now_ms = started.elapsed().as_millis() as u64;
                    let command = tonex_midi::resolve_action(action, now_ms, |id| {
                        runtime.parameter_value(id)
                    });
                    match command {
                        Ok(command) => serial_midi_commands.push(command),
                        Err(error) => diagnostics.record_midi_mapping_error(error),
                    }
                }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    midi = None;
                    break;
                }
                Some(Err(std::sync::mpsc::TryRecvError::Empty)) | None => break,
            }
        }
        #[cfg(feature = "serial-midi")]
        {
            let snapshot = runtime.snapshot();
            for command in coalesce_command_batch(
                (snapshot.selected_preset, snapshot.selected_slot),
                serial_midi_commands,
            ) {
                diagnostics.record_command("Serial MIDI", runtime.apply_command(command));
            }
        }

        #[cfg(feature = "ble-midi")]
        if let Some(code) = ble_midi
            .as_ref()
            .and_then(esp_idf_midi::BleMidi::take_error)
        {
            println!("BLE MIDI asynchronous error: {code}");
        }

        #[cfg(feature = "ble-midi")]
        let mut ble_midi_commands = Vec::new();
        #[cfg(feature = "ble-midi")]
        for _ in 0..MIDI_ACTIONS_PER_TICK {
            match ble_midi.as_ref().map(esp_idf_midi::BleMidi::try_recv) {
                Some(Ok(input)) => {
                    if runtime.snapshot().connection != ConnectionState::Ready {
                        continue;
                    }
                    let now_ms = started.elapsed().as_millis() as u64;
                    match input {
                        esp_idf_midi::BleInput::Midi(action) => {
                            match tonex_midi::resolve_action(action, now_ms, |id| {
                                runtime.parameter_value(id)
                            }) {
                                Ok(command) => ble_midi_commands.push(command),
                                Err(error) => diagnostics.record_midi_mapping_error(error),
                            }
                        }
                        esp_idf_midi::BleInput::FootTrigger(button) => {
                            let snapshot = runtime.snapshot();
                            let action = snapshot.settings.foot_controller.resolve(
                                snapshot.selected_preset,
                                button,
                                tonex_domain::FootGesture::ShortPress,
                            );
                            if let Some(command) =
                                tonex_midi::resolve_foot_action(action, now_ms, |id| {
                                    runtime.parameter_value(id)
                                })
                            {
                                ble_midi_commands.push(command);
                            }
                        }
                        esp_idf_midi::BleInput::FootEdge { button, pressed } => {
                            if let Some(gesture) = ble_press_tracker.edge(button, pressed, now_ms) {
                                let snapshot = runtime.snapshot();
                                let action = snapshot.settings.foot_controller.resolve(
                                    snapshot.selected_preset,
                                    button,
                                    gesture,
                                );
                                if let Some(command) =
                                    tonex_midi::resolve_foot_action(action, now_ms, |id| {
                                        runtime.parameter_value(id)
                                    })
                                {
                                    ble_midi_commands.push(command);
                                }
                            }
                        }
                    }
                }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    ble_midi = None;
                    break;
                }
                Some(Err(std::sync::mpsc::TryRecvError::Empty)) | None => break,
            }
        }
        #[cfg(feature = "ble-midi")]
        {
            let now_ms = started.elapsed().as_millis() as u64;
            if runtime.snapshot().connection == ConnectionState::Ready
                && let Some(button) = ble_press_tracker.poll_long(now_ms)
            {
                let snapshot = runtime.snapshot();
                let action = snapshot.settings.foot_controller.resolve(
                    snapshot.selected_preset,
                    button,
                    tonex_domain::FootGesture::LongPress,
                );
                if let Some(command) = tonex_midi::resolve_foot_action(action, now_ms, |id| {
                    runtime.parameter_value(id)
                }) {
                    ble_midi_commands.push(command);
                }
            }
            let snapshot = runtime.snapshot();
            for command in coalesce_command_batch(
                (snapshot.selected_preset, snapshot.selected_slot),
                ble_midi_commands,
            ) {
                diagnostics.record_command("BLE MIDI", runtime.apply_command(command));
            }
        }

        let mut connected = false;
        let mut disconnected = false;
        let mut dropped_rx_bytes = 0;

        {
            let usb = runtime.transport_mut();
            let _ = usb.poll(0);

            if usb.take_device_disconnected() {
                let _ = usb.close_disconnected_tonex_one();
                disconnected = true;
                retry_ticks = OPEN_RETRY_TICKS;
            }

            if usb.is_device_open() {
                dropped_rx_bytes = usb.take_dropped_rx_bytes();
            } else if usb.has_seen_tonex_one() {
                if retry_ticks == 0 {
                    match usb.open_tonex_one() {
                        Ok(()) => {
                            println!("USB TONEX ONE opened");
                            #[cfg(not(feature = "usb-connection-diagnostics"))]
                            {
                                usb_status_label = Some(tonex_ui_model::PresetLabel::from_bytes(
                                    b"SYNCING TONEX ONE",
                                ));
                            }
                            connected = true;
                        }
                        Err(error) => {
                            println!("USB TONEX ONE open failed: {}", error.0);
                            // Keep transport diagnostics on the serial log.
                            // The stage display should remain product-facing
                            // when no pedal is attached or enumeration fails.
                            #[cfg(not(feature = "usb-connection-diagnostics"))]
                            {
                                usb_status_label = None;
                            }
                        }
                    }
                    retry_ticks = OPEN_RETRY_TICKS;
                } else {
                    retry_ticks -= 1;
                }
            } else {
                // Wait for the dedicated enumerator to own and validate the
                // exact raw TONEX device before installing/opening CDC.
                retry_ticks = 0;
            }
        }

        if disconnected {
            runtime.disconnect();
        }

        if connected {
            if let Err(error) = runtime.connect() {
                println!("USB TONEX ONE handshake failed: {error:?}");
                #[cfg(feature = "usb-connection-diagnostics")]
                {
                    usb_handshake_failures = usb_handshake_failures.saturating_add(1);
                }
                #[cfg(not(feature = "usb-connection-diagnostics"))]
                {
                    usb_status_label = None;
                }
                runtime.disconnect();
                let _ = runtime.transport_mut().close_tonex_one();
            }
        }

        if dropped_rx_bytes != 0 {
            // A burst of state notifications is not a physical disconnect.
            // Discard the damaged tail and let the framed protocol decoder
            // lock onto the next complete message while retaining Ready state.
            println!("USB TONEX ONE receive burst recovered: dropped {dropped_rx_bytes} bytes");
            runtime.transport_mut().discard_received();
            if runtime.snapshot().connection == ConnectionState::Ready {
                runtime.recover_receive_stream();
            } else {
                // If the dropped bytes contained a response required by boot
                // synchronization, merely resetting framing would wait for a
                // reply that will never arrive. Restart the handshake on the
                // still-open physical connection instead.
                #[cfg(not(feature = "usb-connection-diagnostics"))]
                {
                    usb_status_label = Some(tonex_ui_model::PresetLabel::from_bytes(
                        b"SYNCING TONEX ONE",
                    ));
                }
                if let Err(error) = runtime.connect() {
                    println!("USB TONEX ONE resynchronization failed: {error:?}");
                    #[cfg(feature = "usb-connection-diagnostics")]
                    {
                        usb_handshake_failures = usb_handshake_failures.saturating_add(1);
                    }
                    runtime.disconnect();
                    let _ = runtime.transport_mut().close_tonex_one();
                    retry_ticks = OPEN_RETRY_TICKS;
                }
            }
        } else {
            for _ in 0..USB_RX_CHUNKS_PER_TICK {
                let received = runtime.transport_mut().receive(&mut receive_buffer);
                if received == 0 {
                    break;
                }
                if let Err(error) = runtime.ingest_usb(&receive_buffer[..received]) {
                    // A malformed or asynchronous protocol message must not
                    // tear down an otherwise healthy USB connection. TONEX
                    // can emit live notifications during synchronization.
                    println!("USB TONEX ONE protocol message skipped: {error:?}");
                    #[cfg(feature = "usb-connection-diagnostics")]
                    {
                        usb_handshake_failures = usb_handshake_failures.saturating_add(1);
                    }
                    // Protocol detail remains available in the diagnostic
                    // log and must not replace the preset/title area.
                }
            }
        }

        let device_ready = runtime.snapshot().connection == ConnectionState::Ready;
        #[cfg(not(feature = "usb-connection-diagnostics"))]
        if device_ready {
            usb_status_label = None;
        }

        #[cfg(feature = "ble-midi")]
        if device_ready
            && ble_midi.is_none()
            && (ble_auto_start_pending || ble_scan_pending)
            && let Some(modem) = ble_modem.take()
        {
            let midi = runtime.snapshot().settings.midi;
            // A user-requested scan must list every device instead of
            // immediately reconnecting the previously saved address.
            let target = if ble_scan_pending {
                None
            } else {
                midi.paired_peer
            };
            match esp_idf_midi::start_ble(modem, midi.channel, target) {
                Ok(ble) => {
                    if ble_scan_pending && let Err(code) = ble.start_scan() {
                        println!("BLE scan start failed: {code}");
                    }
                    ble_midi = Some(ble);
                }
                Err(error) => println!("BLE MIDI deferred initialization failed: {error}"),
            }
            ble_auto_start_pending = false;
            ble_scan_pending = false;
        }
        if !device_ready && page_requires_tonex(ui_page) {
            ui_page = tonex_ui_model::UiPage::Stage;
        }

        if let Some(switches) = switches.as_mut()
            && let Ok(raw) = switches.pressed_mask()
        {
            let edges = switch_debouncer.update(raw);
            let command = if device_ready && edges.pressed & 0b0001 != 0 {
                Some(AppCommand::PreviousPreset)
            } else if device_ready && edges.pressed & 0b0010 != 0 {
                Some(AppCommand::NextPreset)
            } else if edges.pressed & 0b0100 != 0 {
                ui_page = match ui_page {
                    tonex_ui_model::UiPage::Stage => tonex_ui_model::UiPage::Settings,
                    _ => tonex_ui_model::UiPage::Stage,
                };
                None
            } else {
                None
            };
            if let Some(command) = command {
                diagnostics.record_command("Footswitch", runtime.apply_command(command));
            }
        } else if switches.is_some() {
            diagnostics.switch_read_errors = diagnostics.switch_read_errors.saturating_add(1);
        }

        let touch_event = touch.as_ref().map(TouchInput::try_recv);
        match touch_event {
            Some(Ok(action)) => match action {
                tonex_controls::TouchAction::PreviousPreset => {
                    let snapshot = runtime.ui_snapshot();
                    if let Some(command) = handle_ui_swipe(&mut ui_page, &snapshot, true, board) {
                        diagnostics.record_command("Touch", runtime.apply_command(command));
                    }
                }
                tonex_controls::TouchAction::NextPreset => {
                    let snapshot = runtime.ui_snapshot();
                    if let Some(command) = handle_ui_swipe(&mut ui_page, &snapshot, false, board) {
                        diagnostics.record_command("Touch", runtime.apply_command(command));
                    }
                }
                tonex_controls::TouchAction::Tap { x, y } => {
                    let elapsed = started.elapsed();
                    if elapsed >= next_touch_page_change {
                        let snapshot = runtime.ui_snapshot();
                        if let Some(command) = handle_ui_tap(&mut ui_page, &snapshot, board, x, y) {
                            match runtime.apply_command(command) {
                                Ok(AppEffect::PersistSettingsLive(settings)) => {
                                    if let Err(error) = persist_settings(settings) {
                                        println!("Settings save failed: {error}");
                                    }
                                    if let Some(backlight) = auxiliary_backlight.as_mut()
                                        && let Err(error) = backlight
                                            .set_percent(settings.display_brightness_percent)
                                    {
                                        println!(
                                            "Auxiliary backlight brightness failed: {error:?}"
                                        );
                                    }
                                }
                                result => diagnostics.record_command("Touch", result),
                            }
                        }
                        // AXS15231B can briefly report a release while a finger is
                        // still settling. Keep the same physical press from opening
                        // Settings and immediately activating its back button.
                        next_touch_page_change = elapsed + Duration::from_millis(220);
                    }
                }
            },
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => touch = None,
            Some(Err(std::sync::mpsc::TryRecvError::Empty)) | None => {}
        }

        let connection = runtime.snapshot().connection;
        if displayed_connection != Some(connection) {
            let mut led_update_succeeded = true;
            if let (Some(led_device), Some(spec)) = (leds.as_mut(), board.leds) {
                let (red, green, blue) = match connection {
                    ConnectionState::Disconnected => (32, 0, 0),
                    ConnectionState::Connecting | ConnectionState::Synchronizing => (0, 0, 32),
                    ConnectionState::Ready => (0, 32, 0),
                    ConnectionState::Faulted => (32, 0, 32),
                };
                for index in 0..usize::from(spec.count) {
                    if let Err(error) = led_device.set_rgb(index, red, green, blue) {
                        println!("LED color update failed at index {index}: {error:?}");
                        led_update_succeeded = false;
                        break;
                    }
                }
                if led_update_succeeded && let Err(error) = led_device.present() {
                    println!("LED transfer failed: {error:?}");
                    led_update_succeeded = false;
                }
                if !led_update_succeeded {
                    diagnostics.led_errors = diagnostics.led_errors.saturating_add(1);
                }
            }
            if !led_update_succeeded {
                leds = None;
            }
            if led_update_succeeded {
                displayed_connection = Some(connection);
            }
        }

        let elapsed = started.elapsed();
        #[cfg(feature = "usb-connection-diagnostics")]
        {
            if elapsed >= next_usb_diagnostic_summary {
                let usb_diagnostics = runtime.transport_mut().diagnostic_snapshot();
                usb_diagnostic_summary = tonex_ui_model::PresetLabel::from_bytes(
                    format!(
                        "E{} O{} G{} H{} X{}",
                        usb_diagnostics.enumerated,
                        usb_diagnostics.opened,
                        usb_diagnostics.gone,
                        usb_handshake_failures,
                        usb_diagnostics.last_error
                    )
                    .as_bytes(),
                );
                next_usb_diagnostic_summary = elapsed + Duration::from_secs(1);
            }
            // Diagnostic builds deliberately replace all transient offline
            // titles with one latched summary. This keeps reconnect churn from
            // repainting the whole header and makes the failure layer visible.
            usb_status_label = Some(usb_diagnostic_summary);
        }
        #[cfg(not(feature = "touch-disabled-diagnostics"))]
        if elapsed >= next_touch_retry && touch.is_none() {
            println!("Touch unavailable; retrying initialization");
            touch = i2c_buses
                .as_ref()
                .and_then(|buses| start_touch_task(board, buses));
            next_touch_retry = elapsed + Duration::from_secs(5);
        }
        if elapsed >= next_diagnostics {
            let resources = esp_idf_diagnostics::snapshot();
            #[cfg(feature = "serial-midi")]
            let serial_midi_drops = midi
                .as_ref()
                .map_or(0, esp_idf_midi::SerialMidi::dropped_actions);
            #[cfg(not(feature = "serial-midi"))]
            let serial_midi_drops = 0;
            #[cfg(feature = "ble-midi")]
            let ble_midi_drops = ble_midi
                .as_ref()
                .map_or(0, esp_idf_midi::BleMidi::dropped_actions);
            #[cfg(not(feature = "ble-midi"))]
            let ble_midi_drops = 0;
            let touch_drops = touch.as_ref().map_or(0, TouchInput::dropped_actions);
            println!(
                "DIAG uptime_s={} heap_free={} heap_min={} largest_block={} main_stack_hwm={} serial_midi_drops={} ble_midi_drops={} touch_drops={} command_not_ready={} command_errors={} midi_mapping_errors={} switch_read_errors={} led_errors={}",
                elapsed.as_secs(),
                resources.free_heap_bytes,
                resources.minimum_free_heap_bytes,
                resources.largest_free_block_bytes,
                resources.current_task_stack_high_water_bytes,
                serial_midi_drops,
                ble_midi_drops,
                touch_drops,
                diagnostics.command_not_ready,
                diagnostics.command_errors,
                diagnostics.midi_mapping_errors,
                diagnostics.switch_read_errors,
                diagnostics.led_errors
            );
            next_diagnostics = elapsed + Duration::from_secs(60);
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(any(target_os = "espidf", test))]
fn display_snapshot_with_reconnect_grace(
    current: &tonex_ui_model::UiSnapshot,
    last_ready: Option<&tonex_ui_model::UiSnapshot>,
    reconnect_elapsed: Option<std::time::Duration>,
) -> tonex_ui_model::UiSnapshot {
    if current.connection == tonex_domain::ConnectionState::Ready
        || reconnect_elapsed.is_none_or(|elapsed| elapsed >= RECONNECT_DISPLAY_GRACE)
    {
        return *current;
    }

    let Some(preserved) = last_ready else {
        return *current;
    };
    let mut preserved = *preserved;
    preserved.connection = match current.connection {
        tonex_domain::ConnectionState::Disconnected => tonex_domain::ConnectionState::Connecting,
        connection => connection,
    };
    preserved.sync = current.sync;
    preserved.page = current.page;
    preserved.board_label = current.board_label;
    preserved.display_width = current.display_width;
    preserved.display_height = current.display_height;
    preserved.touch_ready = current.touch_ready;
    preserved.brightness_dimmable = current.brightness_dimmable;
    preserved.display_brightness_percent = current.display_brightness_percent;
    preserved.wifi = current.wifi;
    preserved
}

#[cfg(any(target_os = "espidf", test))]
fn display_snapshot_with_startup_stability(
    current: &tonex_ui_model::UiSnapshot,
    startup: Option<&tonex_ui_model::UiSnapshot>,
    last_ready: Option<&tonex_ui_model::UiSnapshot>,
    reconnect_elapsed: Option<std::time::Duration>,
    initial_ready_elapsed: Option<std::time::Duration>,
) -> tonex_ui_model::UiSnapshot {
    // Do not present protocol progress as a sequence of full LCD frames. The
    // AXS15231B panel remains on one product-facing offline frame until the
    // first complete, internally consistent TONEX snapshot is ready.
    if last_ready.is_none()
        && (current.connection != tonex_domain::ConnectionState::Ready
            || initial_ready_elapsed.is_none_or(|elapsed| elapsed < INITIAL_READY_DISPLAY_SETTLE))
    {
        return startup.copied().unwrap_or(*current);
    }
    display_snapshot_with_reconnect_grace(current, last_ready, reconnect_elapsed)
}

#[cfg(any(target_os = "espidf", test))]
fn handle_ui_swipe(
    page: &mut tonex_ui_model::UiPage,
    snapshot: &tonex_ui_model::UiSnapshot,
    toward_previous: bool,
    board: &'static tonex_boards::BoardDescriptor,
) -> Option<tonex_application::AppCommand> {
    use tonex_application::AppCommand;
    use tonex_ui_model::UiPage;

    if snapshot.connection != tonex_domain::ConnectionState::Ready {
        if page_requires_tonex(*page) {
            *page = UiPage::Stage;
        }
        return None;
    }
    if *page == UiPage::Stage {
        return Some(if toward_previous {
            AppCommand::PreviousPreset
        } else {
            AppCommand::NextPreset
        });
    }
    let display = board.display?;
    let class = tonex_ui_model::ui_class_for_dimensions(display.width, display.height)?;
    let rows = ui_page_rows(class, *page);
    let total = ui_page_item_count(*page);
    let offset = match page {
        UiPage::Presets { offset }
        | UiPage::EditMenu { offset }
        | UiPage::Edit { offset, .. }
        | UiPage::Global { offset } => offset,
        UiPage::Stage
        | UiPage::Settings
        | UiPage::QuickConnect
        | UiPage::Display
        | UiPage::DeviceInfo
        | UiPage::Tuner => return None,
    };
    if toward_previous {
        *offset = offset.saturating_sub(rows);
    } else if usize::from(*offset).saturating_add(usize::from(rows)) < total {
        *offset = offset.saturating_add(rows);
    }
    None
}

#[cfg(any(
    test,
    all(
        target_os = "espidf",
        any(feature = "wifi-web", feature = "serial-midi", feature = "ble-midi")
    )
))]
fn coalesce_preset_command(
    command: tonex_application::AppCommand,
    current: (tonex_domain::PresetIndex, tonex_domain::Slot),
) -> Option<(tonex_domain::PresetIndex, tonex_domain::Slot)> {
    use tonex_application::AppCommand;

    match command {
        AppCommand::SelectPreset(preset) => Some((preset, current.1)),
        AppCommand::SelectPresetInSlot { preset, slot } => Some((preset, slot)),
        AppCommand::NextPreset => Some((current.0.next_wrapping(), current.1)),
        AppCommand::PreviousPreset => Some((current.0.previous_wrapping(), current.1)),
        // Slot recall is intentionally not folded here: it depends on the
        // assignment stored in TonexRuntime's latest state document.
        _ => None,
    }
}

#[cfg(any(
    test,
    all(
        target_os = "espidf",
        any(feature = "wifi-web", feature = "serial-midi", feature = "ble-midi")
    )
))]
fn coalesce_command_batch(
    initial: (tonex_domain::PresetIndex, tonex_domain::Slot),
    commands: impl IntoIterator<Item = tonex_application::AppCommand>,
) -> Vec<tonex_application::AppCommand> {
    use tonex_application::AppCommand;

    let mut output = Vec::new();
    let mut projected_selection = initial;
    let mut pending_selection = None;
    for command in commands {
        if let Some(selection) = coalesce_preset_command(command, projected_selection) {
            projected_selection = selection;
            pending_selection = Some(selection);
            continue;
        }
        if let Some((preset, slot)) = pending_selection.take() {
            output.push(AppCommand::SelectPresetInSlot { preset, slot });
        }

        let replace_last = match (output.last(), command) {
            (
                Some(AppCommand::SetParameter { id: previous, .. }),
                AppCommand::SetParameter { id, .. },
            ) => *previous == id,
            (Some(AppCommand::SetMasterVolumeTenths(_)), AppCommand::SetMasterVolumeTenths(_))
            | (Some(AppCommand::SetPresetVolumeTenths(_)), AppCommand::SetPresetVolumeTenths(_)) => {
                true
            }
            _ => false,
        };
        if replace_last {
            let last = output
                .last_mut()
                .expect("replace_last requires an existing command");
            *last = command;
        } else {
            output.push(command);
        }
        if matches!(command, AppCommand::NextAbBank | AppCommand::PreviousAbBank) {
            let bank_count =
                u8::try_from(tonex_domain::PRESET_COUNT / 2).expect("TONEX ONE bank count fits u8");
            let current_bank = projected_selection.0.get() / 2;
            let bank = if matches!(command, AppCommand::NextAbBank) {
                (current_bank + 1) % bank_count
            } else {
                (current_bank + bank_count - 1) % bank_count
            };
            let slot = match projected_selection.1 {
                tonex_domain::Slot::B => tonex_domain::Slot::B,
                tonex_domain::Slot::A | tonex_domain::Slot::C => tonex_domain::Slot::A,
            };
            let offset = u8::from(slot == tonex_domain::Slot::B);
            projected_selection = (
                tonex_domain::PresetIndex::new(bank * 2 + offset)
                    .expect("A/B bank preset is valid"),
                slot,
            );
        }
    }
    if let Some((preset, slot)) = pending_selection {
        output.push(AppCommand::SelectPresetInSlot { preset, slot });
    }
    output
}

#[cfg(any(target_os = "espidf", test))]
#[allow(clippy::too_many_lines)]
fn handle_ui_tap(
    page: &mut tonex_ui_model::UiPage,
    snapshot: &tonex_ui_model::UiSnapshot,
    board: &'static tonex_boards::BoardDescriptor,
    x: u16,
    y: u16,
) -> Option<tonex_application::AppCommand> {
    use tonex_application::AppCommand;
    use tonex_ui_model::{
        EditBlock, UiPage, settings_item_at, stage_layout, ui_class_for_dimensions,
    };

    let display = board.display?;
    let class = ui_class_for_dimensions(display.width, display.height)?;
    let layout = stage_layout(class, true);
    if snapshot.connection != tonex_domain::ConnectionState::Ready && page_requires_tonex(*page) {
        *page = UiPage::Stage;
        return None;
    }
    match *page {
        UiPage::Stage => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Settings;
                return None;
            }
            if snapshot.connection != tonex_domain::ConnectionState::Ready {
                return None;
            }
            if layout.preset_name.contains(x, y)
                || layout.preset_number.contains(x, y)
                || layout.slot.contains(x, y)
            {
                let rows = ui_page_rows(class, UiPage::Presets { offset: 0 }).max(1);
                *page = UiPage::Presets {
                    offset: snapshot.preset.get() / rows * rows,
                };
                return None;
            }
            for (index, rect) in layout.signal_chain.iter().enumerate() {
                if rect.contains(x, y) {
                    if snapshot.connection != tonex_domain::ConnectionState::Ready {
                        return None;
                    }
                    return signal_block_toggle_command(snapshot, index);
                }
            }
            if layout.master.contains(x, y) || layout.bpm.contains(x, y) {
                *page = UiPage::Global { offset: 0 };
            }
            None
        }
        UiPage::Settings => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Stage;
                return None;
            }
            let item = settings_item_at(class, snapshot.brightness_dimmable, x, y)?;
            *page = match item {
                tonex_ui_model::SettingsItem::QuickConnect => UiPage::QuickConnect,
                tonex_ui_model::SettingsItem::Display => UiPage::Display,
                tonex_ui_model::SettingsItem::Device => UiPage::DeviceInfo,
                tonex_ui_model::SettingsItem::Tuner => UiPage::Tuner,
            };
            None
        }
        UiPage::QuickConnect => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Settings;
            }
            None
        }
        UiPage::Display => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Settings;
                return None;
            }
            if !snapshot.brightness_dimmable || y < ui_header_height(class) {
                return None;
            }
            let current = snapshot.display_brightness_percent;
            let next = if x < display.width / 2 {
                current.saturating_sub(10)
            } else {
                current.saturating_add(10).min(100)
            };
            (next != current).then_some(AppCommand::UpdateBrightness(next))
        }
        UiPage::DeviceInfo | UiPage::Tuner => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Settings;
            }
            None
        }
        UiPage::Presets { offset } => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Stage;
                return None;
            }
            let row = ui_row_at(class, *page, x, y)?;
            let index = usize::from(offset).saturating_add(row);
            let preset = tonex_domain::PresetIndex::new(u8::try_from(index).ok()?).ok()?;
            *page = UiPage::Stage;
            Some(AppCommand::SelectPreset(preset))
        }
        UiPage::EditMenu { offset } => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Stage;
                return None;
            }
            let row = ui_row_at(class, *page, x, y)?;
            let block =
                *tonex_ui_model::EDIT_BLOCKS.get(usize::from(offset).saturating_add(row))?;
            *page = UiPage::Edit { block, offset: 0 };
            None
        }
        UiPage::Edit { block, offset } => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::EditMenu { offset: 0 };
                return None;
            }
            parameter_tap_command(snapshot, *page, block, offset, class, x, y, false)
        }
        UiPage::Global { offset } => {
            if layout.settings.is_some_and(|rect| rect.contains(x, y)) {
                *page = UiPage::Stage;
                return None;
            }
            parameter_tap_command(
                snapshot,
                *page,
                EditBlock::Dynamics,
                offset,
                class,
                x,
                y,
                true,
            )
        }
    }
}

#[cfg(any(target_os = "espidf", test))]
const fn page_requires_tonex(page: tonex_ui_model::UiPage) -> bool {
    matches!(
        page,
        tonex_ui_model::UiPage::Presets { .. }
            | tonex_ui_model::UiPage::EditMenu { .. }
            | tonex_ui_model::UiPage::Edit { .. }
            | tonex_ui_model::UiPage::Global { .. }
    )
}

#[cfg(any(target_os = "espidf", test))]
fn signal_block_toggle_command(
    snapshot: &tonex_ui_model::UiSnapshot,
    index: usize,
) -> Option<tonex_application::AppCommand> {
    use tonex_application::AppCommand;
    use tonex_parameters::{
        TONEX_GLOBAL_CABSIM_BYPASS, TONEX_PARAM_CABINET_TYPE, TONEX_PARAM_COMP_ENABLE,
        TONEX_PARAM_DELAY_ENABLE, TONEX_PARAM_MODEL_AMP_ENABLE, TONEX_PARAM_MODULATION_ENABLE,
        TONEX_PARAM_NOISE_GATE_ENABLE, TONEX_PARAM_REVERB_ENABLE,
    };

    let (id, value) = match index {
        0 => (
            TONEX_PARAM_NOISE_GATE_ENABLE,
            toggle_value(snapshot.fx.gate),
        ),
        1 => (
            TONEX_PARAM_COMP_ENABLE,
            toggle_value(snapshot.fx.compressor),
        ),
        2 => (TONEX_PARAM_MODEL_AMP_ENABLE, toggle_value(snapshot.fx.amp)),
        3 if snapshot.fx.cabinet => (TONEX_GLOBAL_CABSIM_BYPASS, 1.0),
        3 => {
            let cabinet_type = snapshot_parameter(snapshot, TONEX_PARAM_CABINET_TYPE)?;
            if (cabinet_type - 2.0).abs() < f32::EPSILON {
                (TONEX_PARAM_CABINET_TYPE, 0.0)
            } else {
                (TONEX_GLOBAL_CABSIM_BYPASS, 0.0)
            }
        }
        4 => (
            TONEX_PARAM_MODULATION_ENABLE,
            toggle_value(snapshot.fx.modulation),
        ),
        5 => (TONEX_PARAM_DELAY_ENABLE, toggle_value(snapshot.fx.delay)),
        6 => (TONEX_PARAM_REVERB_ENABLE, toggle_value(snapshot.fx.reverb)),
        _ => return None,
    };
    Some(AppCommand::SetParameter { id, value })
}

#[cfg(any(target_os = "espidf", test))]
const fn toggle_value(enabled: bool) -> f32 {
    if enabled { 0.0 } else { 1.0 }
}

#[cfg(any(target_os = "espidf", test))]
fn snapshot_parameter(
    snapshot: &tonex_ui_model::UiSnapshot,
    id: tonex_parameters::ParameterId,
) -> Option<f32> {
    let index = tonex_parameters::all_specs()
        .iter()
        .position(|definition| definition.id == id)?;
    snapshot.parameters.get(index).copied()
}

#[cfg(any(target_os = "espidf", test))]
#[allow(clippy::too_many_arguments)]
fn parameter_tap_command(
    snapshot: &tonex_ui_model::UiSnapshot,
    page: tonex_ui_model::UiPage,
    block: tonex_ui_model::EditBlock,
    offset: u8,
    class: tonex_ui_model::UiClass,
    x: u16,
    y: u16,
    globals: bool,
) -> Option<tonex_application::AppCommand> {
    let row = ui_row_at(class, page, x, y)?;
    let wanted = usize::from(offset).saturating_add(row);
    let (definition, current) = tonex_parameters::all_specs()
        .iter()
        .zip(snapshot.parameters)
        .filter(|(definition, _)| {
            if globals {
                definition.id.get() >= 110
            } else {
                block.contains_parameter(definition.id.get())
            }
        })
        .nth(wanted)?;
    let increase = x >= snapshot_width(class) / 2;
    let value = match definition.kind {
        tonex_parameters::ParameterKind::Switch => {
            if current == 0.0 {
                1.0
            } else {
                0.0
            }
        }
        tonex_parameters::ParameterKind::Select => {
            if increase {
                current + 1.0
            } else {
                current - 1.0
            }
        }
        tonex_parameters::ParameterKind::Range => {
            let span = definition.maximum - definition.minimum;
            let step = if span <= 2.0 {
                0.01
            } else if span <= 20.0 {
                0.1
            } else {
                1.0
            };
            if increase {
                current + step
            } else {
                current - step
            }
        }
    }
    .clamp(definition.minimum, definition.maximum);
    Some(tonex_application::AppCommand::SetParameter {
        id: definition.id,
        value,
    })
}

#[cfg(any(target_os = "espidf", test))]
const fn snapshot_width(class: tonex_ui_model::UiClass) -> u16 {
    class.metrics().width
}

#[cfg(any(target_os = "espidf", test))]
fn ui_row_at(
    class: tonex_ui_model::UiClass,
    page: tonex_ui_model::UiPage,
    x: u16,
    y: u16,
) -> Option<usize> {
    let metrics = class.metrics();
    if x < metrics.margin || x >= metrics.width.saturating_sub(metrics.margin) {
        return None;
    }
    let content_top = ui_header_height(class);
    let content_bottom = metrics.height.saturating_sub(metrics.margin);
    if y < content_top || y >= content_bottom {
        return None;
    }
    let rows = usize::from(ui_page_rows(class, page));
    if rows == 0 {
        return None;
    }
    let gap: u16 = if class == tonex_ui_model::UiClass::Tiny128 {
        2
    } else if matches!(
        page,
        tonex_ui_model::UiPage::Presets { .. } | tonex_ui_model::UiPage::EditMenu { .. }
    ) {
        5
    } else {
        6
    };
    let row_count = u16::try_from(rows).ok()?.max(1);
    let row_height = content_bottom
        .saturating_sub(content_top)
        .saturating_sub(gap.saturating_mul(row_count.saturating_sub(1)))
        / row_count;
    for row in 0..rows {
        let top = content_top.saturating_add(
            u16::try_from(row)
                .ok()?
                .saturating_mul(row_height.saturating_add(gap)),
        );
        if y >= top && y < top.saturating_add(row_height) {
            return Some(row);
        }
    }
    None
}

#[cfg(any(target_os = "espidf", test))]
const fn ui_header_height(class: tonex_ui_model::UiClass) -> u16 {
    match class {
        tonex_ui_model::UiClass::Tiny128 => 24,
        tonex_ui_model::UiClass::Compact320x170 => 42,
        tonex_ui_model::UiClass::Portrait240x280 | tonex_ui_model::UiClass::Landscape280x240 => 56,
        tonex_ui_model::UiClass::Medium480x320 => 70,
        tonex_ui_model::UiClass::Large800x480 => 108,
        tonex_ui_model::UiClass::Headless => 0,
    }
}

#[cfg(any(target_os = "espidf", test))]
const fn ui_page_rows(class: tonex_ui_model::UiClass, page: tonex_ui_model::UiPage) -> u8 {
    let base = match class {
        tonex_ui_model::UiClass::Tiny128 | tonex_ui_model::UiClass::Landscape280x240 => 4,
        tonex_ui_model::UiClass::Compact320x170 => 3,
        tonex_ui_model::UiClass::Portrait240x280 | tonex_ui_model::UiClass::Medium480x320 => 5,
        tonex_ui_model::UiClass::Large800x480 => 7,
        tonex_ui_model::UiClass::Headless => 0,
    };
    match page {
        tonex_ui_model::UiPage::Stage
        | tonex_ui_model::UiPage::QuickConnect
        | tonex_ui_model::UiPage::Display
        | tonex_ui_model::UiPage::DeviceInfo
        | tonex_ui_model::UiPage::Tuner => 0,
        tonex_ui_model::UiPage::Settings => 4,
        tonex_ui_model::UiPage::Presets { .. } | tonex_ui_model::UiPage::EditMenu { .. } => base,
        tonex_ui_model::UiPage::Edit { .. } | tonex_ui_model::UiPage::Global { .. } => {
            if base > 5 {
                5
            } else {
                base
            }
        }
    }
}

#[cfg(any(target_os = "espidf", test))]
fn ui_page_item_count(page: tonex_ui_model::UiPage) -> usize {
    match page {
        tonex_ui_model::UiPage::Presets { .. } => tonex_domain::PRESET_COUNT,
        tonex_ui_model::UiPage::EditMenu { .. } => tonex_ui_model::EDIT_BLOCKS.len(),
        tonex_ui_model::UiPage::Edit { block, .. } => tonex_parameters::all_specs()
            .iter()
            .filter(|definition| block.contains_parameter(definition.id.get()))
            .count(),
        tonex_ui_model::UiPage::Global { .. } => tonex_parameters::all_specs()
            .iter()
            .filter(|definition| definition.id.get() >= 110)
            .count(),
        tonex_ui_model::UiPage::Stage
        | tonex_ui_model::UiPage::QuickConnect
        | tonex_ui_model::UiPage::Display
        | tonex_ui_model::UiPage::DeviceInfo
        | tonex_ui_model::UiPage::Tuner => 0,
        tonex_ui_model::UiPage::Settings => 4,
    }
}

#[cfg(target_os = "espidf")]
struct TouchInput {
    receiver: std::sync::mpsc::Receiver<tonex_controls::TouchAction>,
    dropped_actions: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(target_os = "espidf")]
impl TouchInput {
    fn try_recv(&self) -> Result<tonex_controls::TouchAction, std::sync::mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    fn dropped_actions(&self) -> usize {
        self.dropped_actions
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(target_os = "espidf")]
fn start_touch_task(
    board: &'static tonex_boards::BoardDescriptor,
    buses: &esp_idf_i2c::EspI2cBuses,
) -> Option<TouchInput> {
    if !supports_touch_task(board.hardware.display?.touch) {
        return None;
    }
    let touch = match esp_idf_touch::EspTouch::new(board, buses) {
        Ok(touch) => touch,
        Err(error) => {
            println!("Touch initialization failed: {error:?}");
            return None;
        }
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let dropped_actions = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let task_dropped_actions = std::sync::Arc::clone(&dropped_actions);
    let _pthread_config =
        match esp_idf_diagnostics::PthreadConfigGuard::for_next_thread(1, 1, false) {
            Ok(config) => config,
            Err(error) => {
                println!("Touch task configuration failed: {error}");
                return None;
            }
        };
    let spawn = std::thread::Builder::new()
        .name("tonex-touch".into())
        .stack_size(8 * 1024)
        .spawn(move || touch_task(board, touch, &sender, &task_dropped_actions));
    match spawn {
        Ok(_) => Some(TouchInput {
            receiver,
            dropped_actions,
        }),
        Err(error) => {
            println!("Touch task creation failed: {error}");
            None
        }
    }
}

#[cfg(any(target_os = "espidf", test))]
const fn supports_touch_task(controller: tonex_boards::TouchController) -> bool {
    use tonex_boards::TouchController;
    matches!(
        controller,
        TouchController::Cst816
            | TouchController::Cst816OrCst328
            | TouchController::Axs15231b
            | TouchController::Gt911
    )
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{
        INITIAL_READY_DISPLAY_SETTLE, RECONNECT_DISPLAY_GRACE, coalesce_command_batch,
        coalesce_preset_command, display_frame_hash, display_snapshot_with_reconnect_grace,
        display_snapshot_with_startup_stability, handle_ui_swipe, handle_ui_tap,
        initial_ui_snapshot, supports_display_task, supports_touch_task,
    };
    use std::time::Duration;
    use tonex_boards::{DisplayController, TouchController};
    use tonex_domain::ConnectionState;
    use tonex_ui_model::{EditBlock, PresetLabel, UiPage, UiSnapshot, stage_layout};

    fn validation_board() -> &'static tonex_boards::BoardDescriptor {
        tonex_boards::REGISTRY
            .iter()
            .find(|board| board.id == "jc3248w535")
            .expect("JC3248W535 validation board is registered")
    }

    #[test]
    fn display_hash_is_stable_and_changes_with_pixels() {
        let first = [0x0000, 0xffff, 0x1234];
        let second = [0x0000, 0xffff, 0x1235];
        assert_eq!(display_frame_hash(&first), display_frame_hash(&first));
        assert_ne!(display_frame_hash(&first), display_frame_hash(&second));
    }

    #[test]
    fn startup_display_stays_fixed_until_first_ready_snapshot() {
        let startup = UiSnapshot {
            preset_label: PresetLabel::from_bytes(b"WAITING"),
            ..UiSnapshot::default()
        };
        let syncing = UiSnapshot {
            connection: ConnectionState::Synchronizing,
            preset_label: PresetLabel::from_bytes(b"SYNC 12"),
            ..UiSnapshot::default()
        };
        assert_eq!(
            display_snapshot_with_startup_stability(&syncing, Some(&startup), None, None, None),
            startup
        );

        let ready = UiSnapshot {
            connection: ConnectionState::Ready,
            preset_label: PresetLabel::from_bytes(b"REAL PRESET"),
            ..UiSnapshot::default()
        };
        assert_eq!(
            display_snapshot_with_startup_stability(
                &ready,
                Some(&startup),
                None,
                None,
                Some(INITIAL_READY_DISPLAY_SETTLE / 2),
            ),
            startup
        );
        assert_eq!(
            display_snapshot_with_startup_stability(
                &ready,
                Some(&startup),
                None,
                None,
                Some(INITIAL_READY_DISPLAY_SETTLE),
            ),
            ready
        );
    }

    #[test]
    fn short_reconnect_keeps_the_last_stage_without_unlocking_the_runtime() {
        let ready = UiSnapshot {
            connection: ConnectionState::Ready,
            preset_label: PresetLabel::from_bytes(b"LAST PRESET"),
            ..UiSnapshot::default()
        };
        let current = UiSnapshot {
            connection: ConnectionState::Disconnected,
            preset_label: PresetLabel::from_bytes(b"WAITING"),
            ..UiSnapshot::default()
        };

        let displayed = display_snapshot_with_reconnect_grace(
            &current,
            Some(&ready),
            Some(Duration::from_millis(500)),
        );
        assert_eq!(displayed.preset_label, ready.preset_label);
        assert_eq!(displayed.connection, ConnectionState::Connecting);
        assert_eq!(current.connection, ConnectionState::Disconnected);
    }

    #[test]
    fn sustained_disconnect_returns_to_the_initial_stage() {
        let ready = UiSnapshot {
            connection: ConnectionState::Ready,
            preset_label: PresetLabel::from_bytes(b"LAST PRESET"),
            ..UiSnapshot::default()
        };
        let current = UiSnapshot {
            connection: ConnectionState::Disconnected,
            preset_label: PresetLabel::from_bytes(b"WAITING"),
            ..UiSnapshot::default()
        };

        assert_eq!(
            display_snapshot_with_reconnect_grace(
                &current,
                Some(&ready),
                Some(RECONNECT_DISPLAY_GRACE),
            ),
            current
        );
        assert_eq!(
            display_snapshot_with_reconnect_grace(&current, None, Some(Duration::from_millis(100)),),
            current
        );
    }

    #[test]
    fn every_supported_touch_controller_starts_a_task() {
        for controller in [
            TouchController::Cst816,
            TouchController::Cst816OrCst328,
            TouchController::Axs15231b,
            TouchController::Gt911,
        ] {
            assert!(supports_touch_task(controller));
        }
        assert!(!supports_touch_task(TouchController::None));
    }

    #[test]
    fn rapid_relative_preset_commands_fold_to_the_final_selection() {
        use tonex_application::AppCommand;
        use tonex_domain::{PresetIndex, Slot};

        let mut selection = (PresetIndex::new(18).expect("valid preset"), Slot::B);
        for _ in 0..4 {
            selection = coalesce_preset_command(AppCommand::NextPreset, selection)
                .expect("preset navigation can be folded");
        }
        assert_eq!(
            selection,
            (PresetIndex::new(2).expect("valid preset"), Slot::B)
        );

        selection = coalesce_preset_command(
            AppCommand::SelectPreset(PresetIndex::new(11).expect("valid preset")),
            selection,
        )
        .expect("explicit selection can be folded");
        assert_eq!(
            selection,
            (PresetIndex::new(11).expect("valid preset"), Slot::B)
        );
    }

    #[test]
    fn rapid_web_commands_keep_only_final_equivalent_device_writes() {
        use tonex_application::AppCommand;
        use tonex_domain::{PresetIndex, Slot};
        use tonex_parameters::TONEX_PARAM_EQ_BASS;

        let commands = coalesce_command_batch(
            (PresetIndex::new(4).expect("valid preset"), Slot::A),
            [
                AppCommand::NextPreset,
                AppCommand::NextPreset,
                AppCommand::NextPreset,
                AppCommand::SetParameter {
                    id: TONEX_PARAM_EQ_BASS,
                    value: 2.0,
                },
                AppCommand::SetParameter {
                    id: TONEX_PARAM_EQ_BASS,
                    value: 7.0,
                },
            ],
        );

        assert_eq!(
            commands,
            vec![
                AppCommand::SelectPresetInSlot {
                    preset: PresetIndex::new(7).expect("valid preset"),
                    slot: Slot::A,
                },
                AppCommand::SetParameter {
                    id: TONEX_PARAM_EQ_BASS,
                    value: 7.0,
                },
            ]
        );
    }

    #[test]
    fn ab_bank_navigation_preserves_a_or_b_and_normalizes_c_to_a() {
        use tonex_application::AppCommand;
        use tonex_domain::{PresetIndex, Slot};

        for (starting_slot, expected_slot, expected_preset) in [
            (Slot::A, Slot::A, 6),
            (Slot::B, Slot::B, 7),
            (Slot::C, Slot::A, 6),
        ] {
            let commands = coalesce_command_batch(
                (PresetIndex::new(4).expect("valid preset"), starting_slot),
                [AppCommand::NextAbBank, AppCommand::NextPreset],
            );
            assert_eq!(
                commands.last(),
                Some(&AppCommand::SelectPresetInSlot {
                    preset: PresetIndex::new(expected_preset + 1).expect("valid preset"),
                    slot: expected_slot,
                })
            );
        }
    }

    #[test]
    fn every_display_controller_starts_a_task() {
        for controller in [
            DisplayController::Gc9107,
            DisplayController::St7789,
            DisplayController::Sh8601,
            DisplayController::Axs15231b,
            DisplayController::St7796,
            DisplayController::RgbPanel,
        ] {
            assert!(supports_display_task(controller));
        }
    }

    #[test]
    fn initial_display_snapshot_is_available_before_usb_host_startup() {
        let board = validation_board();
        let settings = tonex_settings::Settings::default();
        let snapshot = initial_ui_snapshot(board, &settings);
        assert_eq!(
            snapshot.connection,
            tonex_domain::ConnectionState::Disconnected
        );
        assert_eq!(snapshot.preset, settings.selected_preset);
        assert_eq!(snapshot.slot, settings.selected_slot);
        assert_eq!(
            (snapshot.display_width, snapshot.display_height),
            (480, 320)
        );
        assert_eq!(snapshot.board_label.as_bytes(), b"jc3248w535");
    }

    #[test]
    fn medium_touch_navigation_reaches_every_top_level_page() {
        let board = validation_board();
        let display = board.display.expect("validation board has a display");
        let class = tonex_ui_model::ui_class_for_dimensions(display.width, display.height)
            .expect("display has a UI class");
        let layout = stage_layout(class, true);
        let snapshot = UiSnapshot {
            connection: tonex_domain::ConnectionState::Ready,
            ..UiSnapshot::default()
        };
        let mut page = UiPage::Stage;

        let settings = layout.settings.expect("touch layout has settings");
        assert_eq!(
            handle_ui_tap(&mut page, &snapshot, board, settings.x + 2, settings.y + 2),
            None
        );
        assert_eq!(page, UiPage::Settings);

        handle_ui_tap(&mut page, &snapshot, board, settings.x + 2, settings.y + 2);
        assert_eq!(page, UiPage::Stage);

        handle_ui_tap(
            &mut page,
            &snapshot,
            board,
            layout.preset_name.x + 10,
            layout.preset_name.y + 10,
        );
        assert_eq!(page, UiPage::Presets { offset: 0 });

        page = UiPage::Stage;
        let amp = layout.signal_chain[2];
        let command = handle_ui_tap(&mut page, &snapshot, board, amp.x + 2, amp.y + 2);
        assert_eq!(
            command,
            Some(tonex_application::AppCommand::SetParameter {
                id: tonex_parameters::TONEX_PARAM_MODEL_AMP_ENABLE,
                value: 0.0,
            })
        );
        assert_eq!(page, UiPage::Stage);

        let disconnected = UiSnapshot::default();
        assert_eq!(
            handle_ui_tap(&mut page, &disconnected, board, amp.x + 2, amp.y + 2),
            None
        );
        assert_eq!(page, UiPage::Stage);

        page = UiPage::Stage;
        let skin = layout.skin.expect("medium layout shows a skin");
        handle_ui_tap(&mut page, &snapshot, board, skin.x + 2, skin.y + 2);
        assert_eq!(page, UiPage::Stage);

        page = UiPage::Stage;
        handle_ui_tap(
            &mut page,
            &snapshot,
            board,
            layout.master.x + 2,
            layout.master.y + 2,
        );
        assert_eq!(page, UiPage::Global { offset: 0 });
    }

    #[test]
    fn list_and_parameter_taps_emit_typed_commands() {
        let board = validation_board();
        let snapshot = UiSnapshot {
            connection: tonex_domain::ConnectionState::Ready,
            ..UiSnapshot::default()
        };
        let mut page = UiPage::Presets { offset: 5 };
        let command = handle_ui_tap(&mut page, &snapshot, board, 100, 185);
        assert_eq!(
            command,
            Some(tonex_application::AppCommand::SelectPreset(
                tonex_domain::PresetIndex::new(7).expect("preset 7 exists")
            ))
        );
        assert_eq!(page, UiPage::Stage);

        page = UiPage::EditMenu { offset: 0 };
        handle_ui_tap(&mut page, &snapshot, board, 100, 138);
        assert_eq!(
            page,
            UiPage::Edit {
                block: EditBlock::Eq,
                offset: 0
            }
        );

        page = UiPage::Edit {
            block: EditBlock::Amp,
            offset: 0,
        };
        let command = handle_ui_tap(&mut page, &snapshot, board, 400, 185);
        assert!(matches!(
            command,
            Some(tonex_application::AppCommand::SetParameter { id, value })
                if id == tonex_parameters::TONEX_PARAM_MODEL_GAIN && value > snapshot.model_gain
        ));
    }

    #[test]
    fn contextual_swipes_page_lists_without_changing_the_tonex_preset() {
        let board = validation_board();
        let snapshot = UiSnapshot {
            connection: tonex_domain::ConnectionState::Ready,
            ..UiSnapshot::default()
        };
        let mut page = UiPage::Presets { offset: 0 };
        assert_eq!(handle_ui_swipe(&mut page, &snapshot, false, board), None);
        assert_eq!(page, UiPage::Presets { offset: 5 });
        assert_eq!(handle_ui_swipe(&mut page, &snapshot, true, board), None);
        assert_eq!(page, UiPage::Presets { offset: 0 });

        page = UiPage::Stage;
        assert_eq!(
            handle_ui_swipe(&mut page, &snapshot, false, board),
            Some(tonex_application::AppCommand::NextPreset)
        );

        let offline = UiSnapshot::default();
        page = UiPage::Presets { offset: 5 };
        assert_eq!(handle_ui_swipe(&mut page, &offline, false, board), None);
        assert_eq!(page, UiPage::Stage);
    }

    #[test]
    fn display_taps_adjust_only_a_real_dimmable_backlight() {
        let board = validation_board();
        let mut snapshot = UiSnapshot {
            display_brightness_percent: 50,
            brightness_dimmable: true,
            ..UiSnapshot::default()
        };
        let mut page = UiPage::Display;
        assert_eq!(
            handle_ui_tap(&mut page, &snapshot, board, 400, 200),
            Some(tonex_application::AppCommand::UpdateBrightness(60))
        );
        assert_eq!(
            handle_ui_tap(&mut page, &snapshot, board, 40, 200),
            Some(tonex_application::AppCommand::UpdateBrightness(40))
        );

        snapshot.brightness_dimmable = false;
        assert_eq!(handle_ui_tap(&mut page, &snapshot, board, 400, 200), None);
    }
}

#[cfg(target_os = "espidf")]
fn touch_task(
    board: &'static tonex_boards::BoardDescriptor,
    mut touch: esp_idf_touch::EspTouch,
    sender: &std::sync::mpsc::SyncSender<tonex_controls::TouchAction>,
    dropped_actions: &std::sync::atomic::AtomicUsize,
) {
    use std::time::Duration;
    use std::time::Instant;

    use hal_contracts::TouchDevice;
    use tonex_controls::DebouncedTouchInterpreter;

    let Some(spec) = board.display else {
        return;
    };
    let mut interpreter = DebouncedTouchInterpreter::new(spec.width, spec.height);
    let mut failures = 0_u8;
    let started = Instant::now();
    let mut next_diagnostics = Duration::from_secs(60);
    loop {
        match touch.poll() {
            Ok(point) => {
                failures = 0;
                let point = point.map(|point| (point.x, point.y));
                if let Some(action) = interpreter.update(point) {
                    match sender.try_send(action) {
                        Ok(()) => {}
                        Err(std::sync::mpsc::TrySendError::Full(_)) => {
                            dropped_actions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return,
                    }
                }
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                if failures >= 5 {
                    println!("Touch polling failed: {error:?}");
                    return;
                }
            }
        }
        let elapsed = started.elapsed();
        if elapsed >= next_diagnostics {
            println!(
                "DIAG_TASK name=touch uptime_s={} stack_hwm={}",
                elapsed.as_secs(),
                esp_idf_diagnostics::current_task_stack_high_water_bytes()
            );
            next_diagnostics = elapsed + Duration::from_secs(60);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(target_os = "espidf")]
fn start_display_task(
    board: &'static tonex_boards::BoardDescriptor,
    initial: tonex_ui_model::UiSnapshot,
) -> Option<std::sync::mpsc::SyncSender<tonex_ui_model::UiSnapshot>> {
    use std::time::Duration;

    if !supports_display_task(board.hardware.display?.controller) {
        return None;
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
    let _pthread_config = match esp_idf_diagnostics::PthreadConfigGuard::for_next_thread(2, 1, true)
    {
        Ok(config) => config,
        Err(error) => {
            println!("Display task configuration failed: {error}");
            return None;
        }
    };
    let spawn = std::thread::Builder::new()
        .name("tonex-display".into())
        // The full stage renderer includes the legacy skin resolver and font
        // pipeline. JC3248W535 hardware diagnostics proved that 32 KiB can
        // overflow before the first present, leaving a black panel.
        .stack_size(64 * 1024)
        .spawn(move || display_task(board, receiver, initial, ready_sender));
    match spawn {
        Ok(_) => match ready_receiver.recv_timeout(Duration::from_secs(10)) {
            Ok(true) => Some(sender),
            Ok(false) => {
                println!("Display task failed before its first frame");
                None
            }
            Err(error) => {
                println!("Display first-frame timeout: {error}");
                None
            }
        },
        Err(error) => {
            println!("Display task creation failed: {error}");
            None
        }
    }
}

#[cfg(any(target_os = "espidf", test))]
const fn supports_display_task(controller: tonex_boards::DisplayController) -> bool {
    use tonex_boards::DisplayController;
    matches!(
        controller,
        DisplayController::Gc9107
            | DisplayController::St7789
            | DisplayController::Sh8601
            | DisplayController::Axs15231b
            | DisplayController::St7796
            | DisplayController::RgbPanel
    )
}

#[cfg(target_os = "espidf")]
fn display_task(
    board: &'static tonex_boards::BoardDescriptor,
    receiver: std::sync::mpsc::Receiver<tonex_ui_model::UiSnapshot>,
    initial: tonex_ui_model::UiSnapshot,
    ready: std::sync::mpsc::SyncSender<bool>,
) {
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    use hal_contracts::DisplayDevice;
    use tonex_ui_model::UiViewModel;

    let mut display = match esp_idf_display::EspDisplay::new(board) {
        Ok(display) => display,
        Err(error) => {
            println!("Display initialization failed: {error:?}");
            let _ = ready.send(false);
            return;
        }
    };
    let Some(spec) = board.display else {
        let _ = ready.send(false);
        return;
    };
    let started = Instant::now();
    let mut next_diagnostics = Duration::from_secs(60);
    let initial_model = UiViewModel::new(spec.ui_class, initial).with_touch(
        board
            .capabilities
            .contains(tonex_boards::Capabilities::TOUCH),
    );
    let render_started = Instant::now();
    if let Err(error) = tonex_ui_renderer::render(&initial_model, display.frame_buffer()) {
        println!("Initial display render failed: {error:?}");
        let _ = ready.send(false);
        return;
    }
    let mut presented_frame_hash = display_frame_hash(display.frame_buffer());
    let mut render_last_us = render_started.elapsed().as_micros();
    let mut render_max_us = render_last_us;
    let present_started = Instant::now();
    if let Err(error) = display.present() {
        println!("Initial display transfer failed: {error:?}");
        let _ = ready.send(false);
        return;
    }
    let mut present_last_us = present_started.elapsed().as_micros();
    let mut present_max_us = present_last_us;
    let mut frames = 1_u64;
    let _ = ready.send(true);
    loop {
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(mut snapshot) => {
                // USB synchronization can publish several intermediate
                // states within one LCD transfer. Wait for that short burst
                // and render only its newest snapshot, avoiding a train of
                // full-screen QSPI writes at the panel edge.
                std::thread::sleep(Duration::from_millis(30));
                while let Ok(newer) = receiver.try_recv() {
                    snapshot = newer;
                }
                let model = UiViewModel::new(spec.ui_class, snapshot).with_touch(
                    board
                        .capabilities
                        .contains(tonex_boards::Capabilities::TOUCH),
                );
                let render_started = Instant::now();
                if let Err(error) = tonex_ui_renderer::render(&model, display.frame_buffer()) {
                    println!("Display render failed: {error:?}");
                    return;
                }
                render_last_us = render_started.elapsed().as_micros();
                render_max_us = render_max_us.max(render_last_us);
                let candidate_frame_hash = display_frame_hash(display.frame_buffer());
                if candidate_frame_hash == presented_frame_hash {
                    continue;
                }
                let present_started = Instant::now();
                if let Err(error) = display.present() {
                    println!("Display transfer failed: {error:?}");
                    return;
                }
                present_last_us = present_started.elapsed().as_micros();
                present_max_us = present_max_us.max(present_last_us);
                presented_frame_hash = candidate_frame_hash;
                frames = frames.saturating_add(1);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let elapsed = started.elapsed();
        if elapsed >= next_diagnostics {
            println!(
                "DIAG_TASK name=display uptime_s={} stack_hwm={} frames={} render_last_us={} render_max_us={} present_last_us={} present_max_us={}",
                elapsed.as_secs(),
                esp_idf_diagnostics::current_task_stack_high_water_bytes(),
                frames,
                render_last_us,
                render_max_us,
                present_last_us,
                present_max_us
            );
            next_diagnostics = elapsed + Duration::from_secs(60);
        }
    }
}

#[cfg(any(target_os = "espidf", test))]
fn display_frame_hash(pixels: &[u16]) -> u64 {
    // FNV-1a over RGB565 bytes. The hash is used only as a transfer de-dup key;
    // it does not replace the authoritative UI state or persist any data.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for pixel in pixels {
        for byte in pixel.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[cfg(target_os = "espidf")]
fn persist_settings(settings: tonex_settings::Settings) -> Result<(), esp_idf_svc::sys::EspError> {
    use hal_contracts::SettingsStore;

    nvs::EspNvsSettingsStore::open()?.save(&settings.encode())
}
