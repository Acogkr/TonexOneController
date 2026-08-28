use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicUsize, Ordering},
    mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel},
};

use esp_idf_svc::{
    bt::{
        BdAddr, Ble, BtDriver, BtStatus, BtUuid,
        ble::{
            gap::{
                AdvertisingDataType, BleGapEvent, EspBleGap, GapSearchEvent, GapSearchResult,
                ScanParams,
            },
            gatt::{
                GattInterface, GattStatus, Handle, Property,
                client::{
                    CharacteristicElement, ConnectionId, DbAttrType, DescriptorElement, EspGattc,
                    GattAuthReq, GattCreateConnParams, GattWriteType, GattcEvent,
                },
            },
        },
    },
    hal::modem::BluetoothModemPeripheral,
    sys::{ESP_FAIL, EspError},
};
use tonex_domain::FootButton;
use tonex_midi::{MidiAction, MidiChannel, decode_ble_packet};

const APP_ID: u16 = 0;
const QUEUE_DEPTH: usize = 24;
const CLIENT_CONFIGURATION_UUID: u16 = 0x2902;
const MAX_CANDIDATES: usize = 8;
const DEVICE_NAME_CAPACITY: usize = tonex_settings::BLUETOOTH_NAME_CAPACITY;

pub const MIDI_SERVICE_UUID: u128 = 0x03b80e5aede84b33a7516ce34ec4c700;
pub const MIDI_CHARACTERISTIC_UUID: u128 = 0x7772e5db38684112a1a9f2669d106bf3;

type Driver = BtDriver<'static, Ble>;
type Gap = Arc<EspBleGap<'static, Ble, Arc<Driver>>>;
type Gattc = Arc<EspGattc<'static, Ble, Arc<Driver>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleConnectionState {
    Idle,
    Searching,
    Connected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BleCandidate {
    pub address: [u8; 6],
    pub address_type: u8,
    pub name: [u8; DEVICE_NAME_CAPACITY],
    pub name_len: u8,
    pub rssi: i8,
}

impl BleCandidate {
    #[must_use]
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..usize::from(self.name_len)]).unwrap_or("Bluetooth MIDI")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleInput {
    Midi(MidiAction),
    FootEdge {
        button: FootButton,
        pressed: bool,
    },
    /// Program Change based Chocolate modes send no release edge. They remain
    /// usable as short presses; hold mappings require momentary CC mode.
    FootTrigger(FootButton),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BleStartError {
    InvalidChannel,
    Esp(i32),
}

impl core::fmt::Display for BleStartError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidChannel => formatter.write_str("MIDI channel must be in 1..=16"),
            Self::Esp(code) => write!(formatter, "ESP-IDF BLE error {code}"),
        }
    }
}

impl From<EspError> for BleStartError {
    fn from(error: EspError) -> Self {
        Self::Esp(error.code())
    }
}

#[derive(Default)]
struct State {
    gatt_if: Option<GattInterface>,
    conn_id: Option<ConnectionId>,
    remote_addr: Option<BdAddr>,
    connecting: bool,
    service_handles: Option<(Handle, Handle)>,
    midi_handle: Option<Handle>,
}

struct Client {
    gap: Gap,
    gattc: Gattc,
    state: Mutex<State>,
    inputs: SyncSender<BleInput>,
    channel: MidiChannel,
    connection: AtomicU8,
    last_error: AtomicI32,
    dropped_inputs: AtomicUsize,
    target: Mutex<Option<(BdAddr, esp_idf_svc::bt::ble::gap::BleAddrType)>>,
    scanning: AtomicBool,
    candidates: Mutex<[Option<BleCandidate>; MAX_CANDIDATES]>,
}

impl Client {
    fn subscribe(self: &Arc<Self>) -> Result<(), EspError> {
        let weak = Arc::downgrade(self);
        self.gap.subscribe(move |event| {
            if let Some(client) = Weak::upgrade(&weak)
                && let Err(error) = client.on_gap_event(event)
            {
                client.record_error(error.code());
            }
        })?;
        let weak = Arc::downgrade(self);
        self.gattc.subscribe(move |(gatt_if, event)| {
            if let Some(client) = Weak::upgrade(&weak)
                && let Err(error) = client.on_gattc_event(gatt_if, event)
            {
                client.record_error(error.code());
            }
        })
    }

    fn on_gap_event(&self, event: BleGapEvent<'_>) -> Result<(), EspError> {
        match event {
            BleGapEvent::ScanParameterConfigured(BtStatus::Success) => {
                self.gap.start_scanning(30)?
            }
            BleGapEvent::ScanParameterConfigured(_) => self.record_error(ESP_FAIL),
            BleGapEvent::ScanResult(GapSearchEvent::InquiryComplete(_)) => {
                self.scanning.store(false, Ordering::Release);
                if self.connection.load(Ordering::Acquire) == 0
                    && self.target.lock().map_err(|_| invalid_state())?.is_some()
                {
                    self.begin_scan()?;
                }
            }
            BleGapEvent::ScanResult(GapSearchEvent::InquiryResult(result)) => {
                self.handle_scan_result(result)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_scan_result(&self, result: GapSearchResult<'_>) -> Result<(), EspError> {
        let GapSearchResult {
            bda,
            ble_addr_type,
            rssi,
            ble_adv,
            ..
        } = result;
        let name = ble_adv
            .and_then(|advertisement| {
                self.gap
                    .resolve_adv_data_by_type(advertisement, AdvertisingDataType::NameCmpl)
                    .or_else(|| {
                        self.gap
                            .resolve_adv_data_by_type(advertisement, AdvertisingDataType::NameShort)
                    })
            })
            .and_then(|bytes| core::str::from_utf8(bytes).ok());
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            return Ok(());
        };
        self.record_candidate(bda, ble_addr_type, name, rssi)?;
        let target = *self.target.lock().map_err(|_| invalid_state())?;
        if !target
            .is_some_and(|(address, address_type)| address == bda && address_type == ble_addr_type)
        {
            return Ok(());
        }
        let mut state = self.lock_state()?;
        if state.connecting || state.conn_id.is_some() {
            return Ok(());
        }
        state.connecting = true;
        let gatt_if = state.gatt_if.ok_or_else(invalid_state)?;
        drop(state);
        self.gap.stop_scanning()?;
        self.gattc
            .enh_open(gatt_if, &GattCreateConnParams::new(bda, ble_addr_type))
    }

    fn on_gattc_event(
        &self,
        gatt_if: GattInterface,
        event: GattcEvent<'_>,
    ) -> Result<(), EspError> {
        match event {
            GattcEvent::ClientRegistered {
                status: GattStatus::Ok,
                app_id: APP_ID,
            } => {
                self.lock_state()?.gatt_if = Some(gatt_if);
                if self.target.lock().map_err(|_| invalid_state())?.is_some() {
                    self.begin_scan()?;
                }
            }
            GattcEvent::Connected { conn_id, addr, .. } => {
                let mut state = self.lock_state()?;
                state.conn_id = Some(conn_id);
                state.remote_addr = Some(addr);
                state.connecting = false;
                drop(state);
                self.gattc.mtu_req(gatt_if, conn_id)?;
            }
            GattcEvent::DiscoveryCompleted {
                status: GattStatus::Ok,
                conn_id,
            } => self.gattc.search_service(
                gatt_if,
                conn_id,
                Some(&BtUuid::uuid128(MIDI_SERVICE_UUID)),
            )?,
            GattcEvent::SearchResult {
                start_handle,
                end_handle,
                srvc_id,
                ..
            } if srvc_id.uuid == BtUuid::uuid128(MIDI_SERVICE_UUID) => {
                self.lock_state()?.service_handles = Some((start_handle, end_handle));
            }
            GattcEvent::SearchComplete {
                status: GattStatus::Ok,
                conn_id,
                ..
            } => self.register_midi_notifications(gatt_if, conn_id)?,
            GattcEvent::RegisterNotify {
                status: GattStatus::Ok,
                handle,
            } => self.enable_notifications(gatt_if, handle)?,
            GattcEvent::WriteDescriptor {
                status: GattStatus::Ok,
                ..
            } => self.connection.store(1, Ordering::Release),
            GattcEvent::Notify { handle, value, .. }
                if self.lock_state()?.midi_handle == Some(handle) =>
            {
                self.receive_packet(value);
            }
            GattcEvent::Disconnected { .. } | GattcEvent::Close { .. } => {
                self.recover_connection()?;
            }
            GattcEvent::Open { status, .. }
            | GattcEvent::DiscoveryCompleted { status, .. }
            | GattcEvent::SearchComplete { status, .. }
            | GattcEvent::RegisterNotify { status, .. }
            | GattcEvent::WriteDescriptor { status, .. }
                if status != GattStatus::Ok =>
            {
                self.record_error(ESP_FAIL);
                self.recover_connection()?;
            }
            GattcEvent::ClientRegistered { status, .. } if status != GattStatus::Ok => {
                self.record_error(ESP_FAIL);
            }
            _ => {}
        }
        Ok(())
    }

    fn register_midi_notifications(
        &self,
        gatt_if: GattInterface,
        conn_id: ConnectionId,
    ) -> Result<(), EspError> {
        let mut state = self.lock_state()?;
        let (start, end) = state.service_handles.ok_or_else(invalid_state)?;
        let remote = state.remote_addr.ok_or_else(invalid_state)?;
        let mut chars = [CharacteristicElement::new(); 1];
        let count = self
            .gattc
            .get_characteristic_by_uuid(
                gatt_if,
                conn_id,
                start,
                end,
                &BtUuid::uuid128(MIDI_CHARACTERISTIC_UUID),
                &mut chars,
            )
            .map_err(|_| invalid_state())?;
        let characteristic = chars
            .first()
            .filter(|_| count > 0)
            .ok_or_else(invalid_state)?;
        if !characteristic.properties().contains(Property::Notify) {
            return Err(invalid_state());
        }
        state.midi_handle = Some(characteristic.handle());
        drop(state);
        self.gattc
            .register_for_notify(gatt_if, &remote, characteristic.handle())
    }

    fn enable_notifications(
        &self,
        gatt_if: GattInterface,
        midi_handle: Handle,
    ) -> Result<(), EspError> {
        let state = self.lock_state()?;
        let conn_id = state.conn_id.ok_or_else(invalid_state)?;
        let count = self
            .gattc
            .get_attr_count(
                gatt_if,
                conn_id,
                DbAttrType::Descriptor {
                    handle: midi_handle,
                },
            )
            .map_err(|_| invalid_state())?;
        let mut descriptors = [DescriptorElement::new(); 1];
        let found = self
            .gattc
            .get_descriptor_by_char_handle(
                gatt_if,
                conn_id,
                midi_handle,
                &BtUuid::uuid16(CLIENT_CONFIGURATION_UUID),
                &mut descriptors,
            )
            .map_err(|_| invalid_state())?;
        let descriptor = descriptors
            .first()
            .filter(|_| count > 0 && found > 0)
            .ok_or_else(invalid_state)?;
        self.gattc.write_descriptor(
            gatt_if,
            conn_id,
            descriptor.handle(),
            &1_u16.to_le_bytes(),
            GattWriteType::RequireResponse,
            GattAuthReq::None,
        )
    }

    fn receive_packet(&self, value: &[u8]) {
        let _ = decode_ble_packet(value, self.channel, |action| {
            let input = normalize_chocolate(action).unwrap_or(BleInput::Midi(action));
            match self.inputs.try_send(input) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    self.dropped_inputs.fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Disconnected(_)) => {}
            }
        });
    }

    fn begin_scan(&self) -> Result<(), EspError> {
        self.connection.store(0, Ordering::Release);
        self.scanning.store(true, Ordering::Release);
        self.gap.set_scan_params(&ScanParams {
            scan_interval: 0x50,
            scan_window: 0x30,
            ..Default::default()
        })
    }

    fn record_candidate(
        &self,
        address: BdAddr,
        address_type: esp_idf_svc::bt::ble::gap::BleAddrType,
        source_name: &str,
        rssi: i32,
    ) -> Result<(), EspError> {
        let bytes = source_name.as_bytes();
        let length = bytes.len().min(DEVICE_NAME_CAPACITY);
        let mut name = [0_u8; DEVICE_NAME_CAPACITY];
        name[..length].copy_from_slice(&bytes[..length]);
        let candidate = BleCandidate {
            address: address.addr(),
            address_type: address_type as u8,
            name,
            name_len: u8::try_from(length).unwrap_or(DEVICE_NAME_CAPACITY as u8),
            rssi: i8::try_from(rssi).unwrap_or(if rssi < 0 { i8::MIN } else { i8::MAX }),
        };
        let mut candidates = self.candidates.lock().map_err(|_| invalid_state())?;
        if let Some(existing) = candidates
            .iter_mut()
            .find(|entry| entry.is_some_and(|entry| entry.address == candidate.address))
        {
            *existing = Some(candidate);
        } else if let Some(empty) = candidates.iter_mut().find(|entry| entry.is_none()) {
            *empty = Some(candidate);
        }
        Ok(())
    }

    fn reset_connection(&self) -> Result<(), EspError> {
        let mut state = self.lock_state()?;
        *state = State {
            gatt_if: state.gatt_if,
            ..State::default()
        };
        self.connection.store(0, Ordering::Release);
        Ok(())
    }

    fn recover_connection(&self) -> Result<(), EspError> {
        let state = self.lock_state()?;
        let needs_recovery = state.connecting
            || state.conn_id.is_some()
            || state.remote_addr.is_some()
            || state.service_handles.is_some()
            || state.midi_handle.is_some();
        drop(state);
        if !needs_recovery {
            return Ok(());
        }
        self.reset_connection()?;
        self.begin_scan()
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, State>, EspError> {
        self.state.lock().map_err(|_| invalid_state())
    }

    fn record_error(&self, code: i32) {
        self.last_error.store(code, Ordering::Release);
    }
}

fn normalize_chocolate(action: MidiAction) -> Option<BleInput> {
    match action {
        MidiAction::SelectPreset(preset) if preset.get() < 4 => {
            Some(BleInput::FootTrigger(button_from_zero(preset.get())))
        }
        MidiAction::Parameter { controller, value } if (1..=4).contains(&controller) => {
            Some(BleInput::FootEdge {
                button: button_from_zero(controller - 1),
                pressed: value >= 64,
            })
        }
        action @ (MidiAction::PreviousAbBank | MidiAction::NextAbBank) => {
            Some(BleInput::Midi(action))
        }
        _ => None,
    }
}

const fn button_from_zero(value: u8) -> FootButton {
    match value {
        0 => FootButton::One,
        1 => FootButton::Two,
        2 => FootButton::Three,
        _ => FootButton::Four,
    }
}

fn invalid_state() -> EspError {
    EspError::from(esp_idf_svc::sys::ESP_ERR_INVALID_STATE).expect("ESP error is non-zero")
}

pub struct BleMidi {
    client: Arc<Client>,
    inputs: Receiver<BleInput>,
}

impl core::fmt::Debug for BleMidi {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.debug_struct("BleMidi").finish_non_exhaustive()
    }
}

impl BleMidi {
    pub fn try_recv(&self) -> Result<BleInput, TryRecvError> {
        self.inputs.try_recv()
    }

    #[must_use]
    pub fn connection_state(&self) -> BleConnectionState {
        if self.client.connection.load(Ordering::Acquire) == 1 {
            BleConnectionState::Connected
        } else if self.client.scanning.load(Ordering::Acquire) {
            BleConnectionState::Searching
        } else {
            BleConnectionState::Idle
        }
    }

    pub fn start_scan(&self) -> Result<(), i32> {
        *self
            .client
            .candidates
            .lock()
            .map_err(|_| esp_idf_svc::sys::ESP_FAIL)? = [None; MAX_CANDIDATES];
        self.client.begin_scan().map_err(|error| error.code())
    }

    #[must_use]
    pub fn candidates(&self) -> [Option<BleCandidate>; MAX_CANDIDATES] {
        self.client
            .candidates
            .lock()
            .map_or([None; MAX_CANDIDATES], |candidates| *candidates)
    }

    pub fn take_error(&self) -> Option<i32> {
        let code = self.client.last_error.swap(0, Ordering::AcqRel);
        (code != 0).then_some(code)
    }

    #[must_use]
    pub fn dropped_actions(&self) -> usize {
        self.client.dropped_inputs.load(Ordering::Relaxed)
    }
}

pub fn start_ble<B>(
    modem: B,
    channel: u8,
    paired: Option<tonex_settings::BluetoothPeer>,
) -> Result<BleMidi, BleStartError>
where
    B: BluetoothModemPeripheral + 'static,
{
    let channel = MidiChannel::new(channel).map_err(|_| BleStartError::InvalidChannel)?;
    let driver = Arc::new(BtDriver::new(modem, None)?);
    let gap = Arc::new(EspBleGap::new(Arc::clone(&driver))?);
    let gattc = Arc::new(EspGattc::new(driver)?);
    let (sender, inputs) = sync_channel(QUEUE_DEPTH);
    let target = paired.map(|peer| {
        let address_type = if peer.address_type == 0 {
            esp_idf_svc::bt::ble::gap::BleAddrType::Public
        } else {
            esp_idf_svc::bt::ble::gap::BleAddrType::Random
        };
        (BdAddr::from_bytes(peer.address), address_type)
    });
    let client = Arc::new(Client {
        gap,
        gattc,
        state: Mutex::new(State::default()),
        inputs: sender,
        channel,
        connection: AtomicU8::new(0),
        last_error: AtomicI32::new(0),
        dropped_inputs: AtomicUsize::new(0),
        target: Mutex::new(target),
        scanning: AtomicBool::new(false),
        candidates: Mutex::new([None; MAX_CANDIDATES]),
    });
    client.subscribe()?;
    client.gattc.register_app(APP_ID)?;
    Ok(BleMidi { client, inputs })
}
