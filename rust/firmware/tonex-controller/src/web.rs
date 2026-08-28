use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
};
use std::time::{Duration, Instant};

use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::io::EspIOError,
    hal::modem::WifiModemPeripheral,
    http::{
        Method,
        server::{Configuration as HttpConfiguration, EspHttpServer},
    },
    io::Write,
    ipv4::Configuration as IpConfiguration,
    netif::{EspNetif, NetifConfiguration, NetifStack},
    nvs::EspDefaultNvsPartition,
    sys::EspError,
    wifi::{
        AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration,
        Configuration as WifiConfiguration, EspWifi, WifiDriver,
    },
    ws::FrameType,
};
use tonex_application::AppCommand;
use tonex_domain::ConnectionState;
use tonex_settings::{FootControllerSettings, MidiSettings, Settings, WifiMode, WifiSettings};
use tonex_ui_model::UiSnapshot;
use tonex_web::{
    INDEX_HTML, MAX_COMMAND_BYTES, MAX_DNS_PACKET_BYTES, WebParameterGroup,
    build_captive_dns_response, encode_parameter_group_snapshot,
    encode_snapshot_with_controller_settings, parse_command, parse_parameter_request,
};

const CAPTIVE_ADDRESS: [u8; 4] = [192, 168, 4, 1];
// Captive DNS resolves this friendly AP-local name to CAPTIVE_ADDRESS. The
// numeric address remains available as a recovery path without adding mDNS.
const CAPTIVE_URL: &str = "http://tonexone.test/";
const COMMAND_QUEUE_DEPTH: usize = 8;
// Mirrors the proven C TempBuffer. With this board's sdkconfig, allocations
// above 16 KiB are placed in PSRAM first instead of consuming USB/display RAM.
const WEB_RESPONSE_BUFFER_BYTES: usize = 20 * 1024;
const HTTP_STACK_BYTES: usize = 6 * 1024;
// sdkconfig.web.defaults provides eight LWIP sockets. ESP-IDF reserves three
// for the HTTP server internals, leaving five client sockets at most.
const HTTP_MAX_OPEN_SOCKETS: usize = 5;
const INVALID_COMMAND: &[u8] =
    br#"{"error":"Controller rejected this control. Reload and try again."}"#;
const BUSY: &[u8] = br#"{"error":"Controller is busy; try again"}"#;
const DEVICE_OFFLINE: &[u8] = br#"{"error":"TONEX ONE is not ready"}"#;
const COMMAND_ACCEPTED: &[u8] = br#"{"accepted":true}"#;

struct CaptiveDns {
    stop: Arc<AtomicBool>,
    task: Option<std::thread::JoinHandle<()>>,
}

impl CaptiveDns {
    fn start() -> Result<Self, std::io::Error> {
        let socket = std::net::UdpSocket::bind(("0.0.0.0", 53))?;
        socket.set_read_timeout(Some(Duration::from_millis(500)))?;
        let stop = Arc::new(AtomicBool::new(false));
        let task_stop = Arc::clone(&stop);
        let task = std::thread::Builder::new()
            .name("tonex-captive-dns".into())
            .stack_size(4 * 1024)
            .spawn(move || {
                let mut query = [0_u8; MAX_DNS_PACKET_BYTES];
                let mut response = [0_u8; MAX_DNS_PACKET_BYTES];
                while !task_stop.load(Ordering::Acquire) {
                    let Ok((length, peer)) = socket.recv_from(&mut query) else {
                        continue;
                    };
                    if let Some(response_len) =
                        build_captive_dns_response(&query[..length], &mut response, CAPTIVE_ADDRESS)
                    {
                        let _ = socket.send_to(&response[..response_len], peer);
                    }
                }
            })?;
        Ok(Self {
            stop,
            task: Some(task),
        })
    }
}

impl Drop for CaptiveDns {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

pub struct WebControl {
    _wifi: BlockingWifi<EspWifi<'static>>,
    _server: EspHttpServer<'static>,
    _dns: Option<CaptiveDns>,
    commands: Receiver<AppCommand>,
    snapshot: Arc<Mutex<UiSnapshot>>,
    parameters: Arc<Mutex<[f32; tonex_parameters::STORED_PARAMETER_COUNT]>>,
    foot_controller: Arc<Mutex<FootControllerSettings>>,
    effective_wifi: WifiSettings,
}

#[derive(Debug)]
pub enum WebStartError {
    Esp(EspError),
    Io(EspIOError),
}

impl core::fmt::Display for WebStartError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Esp(error) => write!(formatter, "{error}"),
            Self::Io(error) => write!(formatter, "{error}"),
        }
    }
}

impl From<EspError> for WebStartError {
    fn from(error: EspError) -> Self {
        Self::Esp(error)
    }
}

impl From<EspIOError> for WebStartError {
    fn from(error: EspIOError) -> Self {
        Self::Io(error)
    }
}

impl core::fmt::Debug for WebControl {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_struct("WebControl").finish_non_exhaustive()
    }
}

impl WebControl {
    pub fn start<M>(modem: M, controller_settings: Settings) -> Result<Self, WebStartError>
    where
        M: WifiModemPeripheral + 'static,
    {
        let system_loop = EspSystemEventLoop::take()?;
        let nvs = EspDefaultNvsPartition::take()?;
        let driver = WifiDriver::new(modem, system_loop.clone(), Some(nvs))?;
        let station_netif = EspNetif::new(NetifStack::Sta)?;
        let mut access_point_netif = NetifConfiguration::wifi_default_router();
        if let Some(IpConfiguration::Router(router)) = access_point_netif.ip_configuration.as_mut()
        {
            let address = core::net::Ipv4Addr::from(CAPTIVE_ADDRESS);
            router.subnet.gateway = address;
            router.dns = Some(address);
            router.secondary_dns = None;
        }
        let access_point_netif = EspNetif::new_with_conf(&access_point_netif)?;
        let mut wifi = BlockingWifi::wrap(
            EspWifi::wrap_all(driver, station_netif, access_point_netif)?,
            system_loop,
        )?;
        let settings = controller_settings.wifi;
        let midi = Some(controller_settings.midi);
        wifi.set_configuration(&wifi_configuration(settings)?)?;
        wifi.start()?;
        let mut effective_wifi = settings;
        if settings.mode == WifiMode::Station {
            if let Err(error) = wifi.connect().and_then(|()| wifi.wait_netif_up()) {
                println!("Station Wi-Fi unavailable ({error}); starting recovery access point");
                let _ = wifi.disconnect();
                wifi.stop()?;
                effective_wifi = WifiSettings::default();
                wifi.set_configuration(&wifi_configuration(effective_wifi)?)?;
                wifi.start()?;
                wifi.wait_netif_up()?;
            }
        } else {
            wifi.wait_netif_up()?;
        }
        let dns = if effective_wifi.mode == WifiMode::AccessPoint {
            match CaptiveDns::start() {
                Ok(dns) => Some(dns),
                Err(error) => {
                    println!("Captive DNS unavailable ({error}); use http://192.168.4.1/ instead");
                    None
                }
            }
        } else {
            None
        };

        let (sender, commands) = sync_channel(COMMAND_QUEUE_DEPTH);
        let snapshot = Arc::new(Mutex::new(UiSnapshot::default()));
        let default_parameters = *tonex_parameters::ParameterStore::with_defaults().values();
        let parameters = Arc::new(Mutex::new(default_parameters));
        let foot_controller = Arc::new(Mutex::new(controller_settings.foot_controller));
        let response_buffer = Arc::new(Mutex::new(vec![0_u8; WEB_RESPONSE_BUFFER_BYTES]));
        let mut server = EspHttpServer::new(&HttpConfiguration {
            stack_size: HTTP_STACK_BYTES,
            max_open_sockets: HTTP_MAX_OPEN_SOCKETS,
            lru_purge_enable: true,
            ..Default::default()
        })?;
        server.fn_handler("/", Method::Get, |request| {
            let mut response = request.into_response(
                200,
                Some("OK"),
                &[
                    ("Content-Type", "text/html; charset=utf-8"),
                    ("Cache-Control", "no-store"),
                    ("Connection", "close"),
                    ("X-Content-Type-Options", "nosniff"),
                    ("X-Frame-Options", "DENY"),
                    ("Referrer-Policy", "no-referrer"),
                ],
            )?;
            response.write_all(INDEX_HTML.as_bytes())?;
            Ok::<(), EspIOError>(())
        })?;
        let png_skin_snapshot = Arc::clone(&snapshot);
        server.fn_handler("/skin.png", Method::Get, move |request| {
            let skin = png_skin_snapshot
                .lock()
                .map_err(|_| invalid_state())?
                .skin
                .id;
            let image = tonex_ui_renderer::web_amp_skin_png(skin).ok_or_else(invalid_argument)?;
            let mut response = request.into_response(
                200,
                Some("OK"),
                &[
                    ("Content-Type", "image/png"),
                    ("Cache-Control", "no-store"),
                    ("Connection", "close"),
                    ("X-Content-Type-Options", "nosniff"),
                ],
            )?;
            response.write_all(image)?;
            Ok::<(), EspIOError>(())
        })?;
        let skin_snapshot = Arc::clone(&snapshot);
        server.fn_handler("/skin.bmp", Method::Get, move |request| {
            let skin = skin_snapshot.lock().map_err(|_| invalid_state())?.skin.id;
            let mut response = request.into_response(
                200,
                Some("OK"),
                &[
                    ("Content-Type", "image/bmp"),
                    ("Cache-Control", "no-store"),
                    ("Connection", "close"),
                    ("X-Content-Type-Options", "nosniff"),
                ],
            )?;
            const WIDTH: usize = tonex_ui_renderer::SKIN_SOURCE_WIDTH;
            const HEIGHT: usize = tonex_ui_renderer::SKIN_SOURCE_HEIGHT;
            const ROW_BYTES: usize = WIDTH * 3;
            const PIXEL_BYTES: u32 = (ROW_BYTES * HEIGHT) as u32;
            const FILE_BYTES: u32 = 54 + PIXEL_BYTES;
            let mut header = [0_u8; 54];
            header[0..2].copy_from_slice(b"BM");
            header[2..6].copy_from_slice(&FILE_BYTES.to_le_bytes());
            header[10..14].copy_from_slice(&54_u32.to_le_bytes());
            header[14..18].copy_from_slice(&40_u32.to_le_bytes());
            header[18..22].copy_from_slice(&(WIDTH as i32).to_le_bytes());
            header[22..26].copy_from_slice(&(HEIGHT as i32).to_le_bytes());
            header[26..28].copy_from_slice(&1_u16.to_le_bytes());
            header[28..30].copy_from_slice(&24_u16.to_le_bytes());
            header[34..38].copy_from_slice(&PIXEL_BYTES.to_le_bytes());
            response.write_all(&header)?;
            let mut row = [0_u8; ROW_BYTES];
            let mut pixels = [0_u16; WIDTH];
            for y in (0..HEIGHT).rev() {
                if !tonex_ui_renderer::decode_skin_row(skin, y, &mut pixels) {
                    return Err(invalid_state().into());
                }
                for x in 0..WIDTH {
                    let pixel = pixels[x];
                    let target = x * 3;
                    row[target] = ((pixel & 0x1f) as u8) * 255 / 31;
                    row[target + 1] = (((pixel >> 5) & 0x3f) as u8) * 255 / 63;
                    row[target + 2] = (((pixel >> 11) & 0x1f) as u8) * 255 / 31;
                }
                response.write_all(&row)?;
            }
            Ok::<(), EspIOError>(())
        })?;
        if effective_wifi.mode == WifiMode::AccessPoint {
            for path in [
                "/generate_204",
                "/gen_204",
                "/hotspot-detect.html",
                "/library/test/success.html",
                "/ncsi.txt",
                "/connecttest.txt",
                "/redirect",
            ] {
                server.fn_handler(path, Method::Get, |request| {
                    let mut response = request.into_response(
                        302,
                        Some("Found"),
                        &[
                            ("Location", CAPTIVE_URL),
                            ("Cache-Control", "no-store"),
                            ("Connection", "close"),
                        ],
                    )?;
                    response.write_all(b"Open TONEX ONE controller")?;
                    Ok::<(), EspIOError>(())
                })?;
            }
        }
        register_websocket(
            &mut server,
            sender,
            Arc::clone(&snapshot),
            Arc::clone(&parameters),
            Arc::clone(&foot_controller),
            response_buffer,
            midi,
        )?;
        Ok(Self {
            _wifi: wifi,
            _server: server,
            _dns: dns,
            commands,
            snapshot,
            parameters,
            foot_controller,
            effective_wifi,
        })
    }

    #[must_use]
    pub const fn effective_wifi(&self) -> WifiSettings {
        self.effective_wifi
    }

    pub fn try_command(&mut self) -> Option<AppCommand> {
        match self.commands.try_recv() {
            Ok(command) => Some(command),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }

    pub fn publish(
        &self,
        snapshot: UiSnapshot,
        parameters: [f32; tonex_parameters::STORED_PARAMETER_COUNT],
        foot_controller: FootControllerSettings,
    ) -> (bool, bool, bool) {
        let snapshot_published = if let Ok(mut current) = self.snapshot.try_lock() {
            *current = snapshot;
            true
        } else {
            false
        };
        let parameters_published = if let Ok(mut current) = self.parameters.try_lock() {
            *current = parameters;
            true
        } else {
            false
        };
        let foot_controller_published = if let Ok(mut current) = self.foot_controller.try_lock() {
            *current = foot_controller;
            true
        } else {
            false
        };
        (
            snapshot_published,
            parameters_published,
            foot_controller_published,
        )
    }
}

fn wifi_configuration(settings: WifiSettings) -> Result<WifiConfiguration, EspError> {
    let ssid = settings
        .ssid
        .as_str()
        .try_into()
        .map_err(|_| invalid_argument())?;
    let password = settings
        .password
        .as_str()
        .try_into()
        .map_err(|_| invalid_argument())?;
    Ok(match settings.mode {
        WifiMode::AccessPoint => WifiConfiguration::AccessPoint(AccessPointConfiguration {
            ssid,
            password,
            auth_method: AuthMethod::WPA2Personal,
            max_connections: 4,
            ..Default::default()
        }),
        WifiMode::Station => WifiConfiguration::Client(ClientConfiguration {
            ssid,
            password,
            auth_method: AuthMethod::WPA2Personal,
            ..Default::default()
        }),
    })
}

fn register_websocket(
    server: &mut EspHttpServer<'static>,
    sender: SyncSender<AppCommand>,
    snapshot: Arc<Mutex<UiSnapshot>>,
    parameters: Arc<Mutex<[f32; tonex_parameters::STORED_PARAMETER_COUNT]>>,
    foot_controller: Arc<Mutex<FootControllerSettings>>,
    response_buffer: Arc<Mutex<Vec<u8>>>,
    midi: Option<MidiSettings>,
) -> Result<(), EspError> {
    let started = Instant::now();
    server
        .ws_handler("/control", None, move |socket| {
            if socket.is_closed() {
                return Ok(());
            }
            if socket.is_new() {
                return send_snapshot(socket, &snapshot, &foot_controller, &response_buffer, midi);
            }
            let (frame_type, length) = socket.recv(&mut [])?;
            if length > MAX_COMMAND_BYTES {
                socket.send(FrameType::Close, &[])?;
                return Ok(());
            }
            let mut input = [0_u8; MAX_COMMAND_BYTES];
            socket.recv(&mut input)?;
            if frame_type != FrameType::Text(false) {
                return Ok(());
            }
            // esp-idf-svc reports text-frame length as payload bytes plus one
            // byte reserved for a C string terminator. The second recv writes
            // only the actual payload, leaving that final byte as zero. Never
            // pass the terminator to the command parser.
            let payload_length = length.saturating_sub(1);
            let input = &input[..payload_length];
            if input == b"snapshot" {
                send_snapshot(socket, &snapshot, &foot_controller, &response_buffer, midi)
            } else if let Some(group) = parse_parameter_request(input) {
                send_parameters(socket, &parameters, &response_buffer, group)
            } else {
                let now_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                match parse_command(input, now_ms) {
                    Ok(command)
                        if command.requires_tonex()
                            && snapshot.lock().map_err(|_| invalid_state())?.connection
                                != ConnectionState::Ready =>
                    {
                        socket.send(FrameType::Text(false), DEVICE_OFFLINE)
                    }
                    Ok(command) if sender.try_send(command).is_ok() => {
                        socket.send(FrameType::Text(false), COMMAND_ACCEPTED)
                    }
                    Ok(_) => socket.send(FrameType::Text(false), BUSY),
                    Err(_) => socket.send(FrameType::Text(false), INVALID_COMMAND),
                }
            }
        })
        .map(|_| ())
}

fn send_parameters(
    socket: &mut esp_idf_svc::http::server::ws::EspHttpWsConnection,
    parameters: &Mutex<[f32; tonex_parameters::STORED_PARAMETER_COUNT]>,
    response_buffer: &Mutex<Vec<u8>>,
    group: WebParameterGroup,
) -> Result<(), EspError> {
    let parameters = parameters.lock().map_err(|_| invalid_state())?;
    let mut output = response_buffer.lock().map_err(|_| invalid_state())?;
    let encoded = encode_parameter_group_snapshot(&parameters, group, &mut output)
        .map_err(|_| invalid_state())?;
    socket.send(FrameType::Text(false), encoded.as_bytes())
}

fn send_snapshot(
    socket: &mut esp_idf_svc::http::server::ws::EspHttpWsConnection,
    snapshot: &Mutex<UiSnapshot>,
    foot_controller: &Mutex<FootControllerSettings>,
    response_buffer: &Mutex<Vec<u8>>,
    midi: Option<MidiSettings>,
) -> Result<(), EspError> {
    let snapshot = snapshot.lock().map_err(|_| invalid_state())?;
    let foot_controller = foot_controller.lock().map_err(|_| invalid_state())?;
    let mut output = response_buffer.lock().map_err(|_| invalid_state())?;
    let encoded = encode_snapshot_with_controller_settings(
        &snapshot,
        midi,
        Some(&foot_controller),
        &mut output,
    )
    .map_err(|_| invalid_state())?;
    socket.send(FrameType::Text(false), encoded.as_bytes())
}

fn invalid_argument() -> EspError {
    EspError::from(esp_idf_svc::sys::ESP_ERR_INVALID_ARG).expect("ESP error code is non-zero")
}

fn invalid_state() -> EspError {
    EspError::from(esp_idf_svc::sys::ESP_ERR_INVALID_STATE).expect("ESP error code is non-zero")
}
