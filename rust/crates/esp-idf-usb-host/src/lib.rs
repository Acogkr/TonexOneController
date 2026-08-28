#![no_std]
//! Narrow ownership boundary around the ESP-IDF USB Host library.
//!
//! All direct FFI and `unsafe` code for the host-library lifecycle lives in
//! this crate. Class clients and TONEX ONE transport are built above this
//! boundary and must not call `esp-idf-sys` directly.

#[cfg(any(target_os = "espidf", test))]
mod ring;

#[cfg(target_os = "espidf")]
extern crate std;

pub const IK_MULTIMEDIA_VENDOR_ID: u16 = 0x1963;
pub const TONEX_ONE_PRODUCT_ID: u16 = 0x00d1;
pub const TONEX_ONE_CDC_INTERFACE: u8 = 0;
#[cfg(any(target_os = "espidf", test))]
const TONEX_TX_CHUNK_BYTES: usize = 512;
#[cfg(any(target_os = "espidf", test))]
// TONEX ONE requires the same large CDC receive transfer used by the proven
// legacy controller. Protocol framing may span callbacks, but reducing this
// buffer changes the device/host transfer cadence during synchronization.
const TONEX_RX_BUFFER_BYTES: usize = 8_192;
#[cfg(any(target_os = "espidf", test))]
// Preserve the exact contiguous DMA budget established by the legacy host:
// one 8 KiB IN buffer, one 512-byte OUT buffer, and its 256-byte margin.
const TONEX_DMA_DRIVER_OVERHEAD_BYTES: usize = 256;
#[cfg(any(target_os = "espidf", test))]
const TONEX_DMA_RESERVE_BYTES: usize =
    TONEX_RX_BUFFER_BYTES + TONEX_TX_CHUNK_BYTES + TONEX_DMA_DRIVER_OVERHEAD_BYTES;
#[cfg(any(target_os = "espidf", test))]
const TONEX_USB_HOST_STARTUP_SETTLE_MS: u64 = 500;
#[cfg(any(target_os = "espidf", test))]
const TONEX_POST_OPEN_SETTLE_MS: u64 = 100;
#[cfg(any(target_os = "espidf", test))]
const TONEX_POST_LINE_STATE_SETTLE_MS: u64 = 250;
#[cfg(any(target_os = "espidf", test))]
const TONEX_OPEN_TIMEOUT_MS: u32 = 1_000;
const TONEX_TX_TIMEOUT_MS: u32 = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceIdentity {
    pub vendor_id: u16,
    pub product_id: u16,
}

impl DeviceIdentity {
    #[must_use]
    pub const fn is_tonex_one(self) -> bool {
        self.vendor_id == IK_MULTIMEDIA_VENDOR_ID && self.product_id == TONEX_ONE_PRODUCT_ID
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EspError(pub i32);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HostEvents {
    pub no_clients: bool,
    pub all_devices_free: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UsbDiagnosticSnapshot {
    pub enumerated: u16,
    pub opened: u16,
    pub gone: u16,
    pub last_error: i32,
}

impl HostEvents {
    #[cfg(target_os = "espidf")]
    const fn from_flags(flags: u32) -> Self {
        Self {
            no_clients: flags & esp_idf_sys::USB_HOST_LIB_EVENT_FLAGS_NO_CLIENTS != 0,
            all_devices_free: flags & esp_idf_sys::USB_HOST_LIB_EVENT_FLAGS_ALL_FREE != 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub skip_phy_setup: bool,
    pub root_port_unpowered: bool,
    pub interrupt_flags: i32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            skip_phy_setup: false,
            // Match the proven legacy controller: installing USB Host also
            // enables the root port. Delaying power and enabling it manually
            // left this board at E0 (no raw enumeration) on real hardware.
            root_port_unpowered: false,
            #[cfg(target_os = "espidf")]
            interrupt_flags: esp_idf_sys::ESP_INTR_FLAG_LEVEL2 as i32,
            #[cfg(not(target_os = "espidf"))]
            interrupt_flags: 4,
        }
    }
}

/// Unique installed USB Host library owner.
///
/// Event handling must be performed serially by one task. Ownership can be
/// moved once into the dedicated daemon before polling begins.
pub struct UsbHost {
    _installed: (),
}

impl UsbHost {
    /// Installs the ESP-IDF USB Host library and acquires its lifecycle.
    ///
    /// # Errors
    ///
    /// Returns the ESP-IDF error code when installation fails. Host builds
    /// always return an unsupported sentinel because no hardware exists.
    #[cfg(target_os = "espidf")]
    pub fn install(config: Config) -> Result<Self, EspError> {
        let raw = esp_idf_sys::usb_host_config_t {
            skip_phy_setup: config.skip_phy_setup,
            root_port_unpowered: config.root_port_unpowered,
            intr_flags: config.interrupt_flags,
            ..Default::default()
        };
        // SAFETY: `raw` is initialized for ESP-IDF 5.5 and remains alive for
        // the duration of the call. Successful installation is represented by
        // the unique returned owner.
        esp_result(unsafe { esp_idf_sys::usb_host_install(&raw) })?;
        Ok(Self { _installed: () })
    }

    /// Reports that USB Host installation is unavailable on a host build.
    ///
    /// # Errors
    ///
    /// Always returns the unsupported sentinel.
    #[cfg(not(target_os = "espidf"))]
    pub fn install(_config: Config) -> Result<Self, EspError> {
        Err(EspError(-1))
    }

    /// Handles pending host-library events on the owning task.
    ///
    /// # Errors
    ///
    /// Returns the ESP-IDF error code when the library is not installed or
    /// event handling fails. Host builds always return unsupported.
    #[cfg(target_os = "espidf")]
    pub fn poll(&mut self, timeout_ticks: u32) -> Result<HostEvents, EspError> {
        let mut flags = 0;
        // SAFETY: ownership of `&mut self` serializes calls, and `flags` is a
        // valid output pointer for the duration of the call.
        esp_result(unsafe { esp_idf_sys::usb_host_lib_handle_events(timeout_ticks, &mut flags) })?;
        Ok(HostEvents::from_flags(flags))
    }

    /// Reports that USB Host polling is unavailable on a host build.
    ///
    /// # Errors
    ///
    /// Always returns the unsupported sentinel.
    #[cfg(not(target_os = "espidf"))]
    pub fn poll(&mut self, _timeout_ticks: u32) -> Result<HostEvents, EspError> {
        Err(EspError(-1))
    }
}

/// Installed USB Host and CDC-ACM class driver in guaranteed drop order.
///
/// CDC is always uninstalled before the underlying USB Host library.
pub struct TonexUsbStack {
    #[cfg(target_os = "espidf")]
    dma_reserve: Option<DmaReserve>,
    #[cfg(target_os = "espidf")]
    cdc: Option<CdcDriver>,
    #[cfg(target_os = "espidf")]
    device: esp_idf_sys::cdc_acm::cdc_acm_dev_hdl_t,
    #[cfg(target_os = "espidf")]
    _enumerator: TonexEnumerator,
    #[cfg(target_os = "espidf")]
    daemon: HostDaemon,
    #[cfg(not(target_os = "espidf"))]
    host: UsbHost,
}

impl TonexUsbStack {
    /// Installs the USB Host library followed by the CDC-ACM class driver.
    ///
    /// # Errors
    ///
    /// Returns the underlying ESP-IDF error from either installation step.
    pub fn install(config: Config) -> Result<Self, EspError> {
        #[cfg(target_os = "espidf")]
        let dma_reserve = DmaReserve::allocate()?;
        // The proven C controller reserves the CDC memory first, then waits
        // 500 FreeRTOS ticks before installing USB Host. Its configured 1000
        // Hz tick rate makes that a 500 ms cold-start settle interval. This
        // when the controller and a bus-powered TONEX ONE start together:
        // installing Host immediately can miss the pedal's unstable initial
        // attach while VBUS and its composite firmware are still starting.
        #[cfg(target_os = "espidf")]
        std::thread::sleep(std::time::Duration::from_millis(
            TONEX_USB_HOST_STARTUP_SETTLE_MS,
        ));
        let host = UsbHost::install(config)?;
        #[cfg(target_os = "espidf")]
        let enumerator = TonexEnumerator::start()?;
        #[cfg(target_os = "espidf")]
        let daemon = HostDaemon::start(host)?;
        Ok(Self {
            #[cfg(target_os = "espidf")]
            dma_reserve: Some(dma_reserve),
            #[cfg(target_os = "espidf")]
            cdc: None,
            #[cfg(target_os = "espidf")]
            device: core::ptr::null_mut(),
            #[cfg(target_os = "espidf")]
            _enumerator: enumerator,
            #[cfg(target_os = "espidf")]
            daemon,
            #[cfg(not(target_os = "espidf"))]
            host,
        })
    }

    /// Handles pending USB Host events.
    ///
    /// # Errors
    ///
    /// Returns the underlying ESP-IDF event-processing error.
    pub fn poll(&mut self, timeout_ticks: u32) -> Result<HostEvents, EspError> {
        #[cfg(target_os = "espidf")]
        {
            let _ = timeout_ticks;
            Ok(self.daemon.take_events())
        }
        #[cfg(not(target_os = "espidf"))]
        {
            self.host.poll(timeout_ticks)
        }
    }

    #[must_use]
    pub fn is_device_open(&self) -> bool {
        #[cfg(target_os = "espidf")]
        {
            !self.device.is_null()
        }
        #[cfg(not(target_os = "espidf"))]
        {
            false
        }
    }

    /// Opens only the IK Multimedia TONEX ONE CDC interface.
    ///
    /// # Errors
    ///
    /// Returns an ESP-IDF error if the exact VID/PID/interface cannot be
    /// opened or its required 115200 8N1 line state cannot be configured.
    #[cfg(target_os = "espidf")]
    pub fn open_tonex_one(&mut self) -> Result<(), EspError> {
        if self.is_device_open() {
            return Ok(());
        }
        // Keep the DMA reservation intact while no matching pedal exists.
        // The CDC driver's descriptor callback establishes exact VID/PID
        // identity first; only then may the open call consume the reserved
        // contiguous buffers. Blind timed opens would release the reservation
        // for one second at a time and let radio tasks fragment it again.
        if !self.has_seen_tonex_one() {
            return Err(EspError(esp_idf_sys::ESP_ERR_NOT_FOUND));
        }

        RX_RING.clear();
        DEVICE_DISCONNECTED.store(false, core::sync::atomic::Ordering::Release);
        if self.cdc.is_none() {
            self.cdc = Some(CdcDriver::install()?);
        }
        // The retained C controller reserves exactly these DMA-capable bytes
        // before Wi-Fi/Bluetooth can fragment internal memory, then releases
        // them immediately before CDC creates the TONEX buffers. Keep the
        // same invariant under RAII so optional radios cannot make later USB
        // enumeration depend on boot timing.
        self.dma_reserve.take();
        let config = esp_idf_sys::cdc_acm::cdc_acm_host_device_config_t {
            connection_timeout_ms: TONEX_OPEN_TIMEOUT_MS,
            out_buffer_size: TONEX_TX_CHUNK_BYTES,
            in_buffer_size: TONEX_RX_BUFFER_BYTES,
            // The dedicated raw enumerator owns disconnect detection, matching
            // the reference controller's client lifecycle.
            event_cb: None,
            data_cb: Some(rx_callback),
            user_arg: core::ptr::null_mut(),
        };
        let mut handle = core::ptr::null_mut();
        // SAFETY: the configuration and output pointer remain valid for the
        // call. Callbacks use only process-lifetime atomics and the static
        // bounded ring. The exact TONEX ONE VID/PID and interface are passed.
        if let Err(error) = esp_result(unsafe {
            esp_idf_sys::cdc_acm::cdc_acm_host_open_v1_dispatch(
                IK_MULTIMEDIA_VENDOR_ID,
                TONEX_ONE_PRODUCT_ID,
                TONEX_ONE_CDC_INTERFACE,
                &config,
                &mut handle,
            )
        }) {
            LAST_USB_ERROR.store(error.0, core::sync::atomic::Ordering::Release);
            // Rebuilding the contiguous legacy-sized guard after optional
            // radios have started is best effort. CDC has just released its
            // own exact buffers, so reservation failure must not mask the
            // actual open result or prevent a subsequent direct reopen.
            let _ = self.restore_dma_reserve();
            return Err(error);
        }

        // The reference controller allows the composite device and CDC
        // interface to settle before issuing class requests. TONEX ONE can
        // enumerate before it is ready to accept line-coding control traffic.
        std::thread::sleep(std::time::Duration::from_millis(TONEX_POST_OPEN_SETTLE_MS));

        // The reference sequence performs a GET before replacing the coding.
        // Besides parity with the known-good implementation, this confirms
        // that endpoint zero accepts CDC class requests before we continue.
        let mut current_line_coding = esp_idf_sys::cdc_acm::cdc_acm_line_coding_t {
            dwDTERate: 0,
            bCharFormat: 0,
            bParityType: 0,
            bDataBits: 0,
        };
        // SAFETY: `handle` was returned by a successful open and the output
        // structure is writable for the complete synchronous request.
        if let Err(error) = esp_result(unsafe {
            esp_idf_sys::cdc_acm::cdc_acm_host_line_coding_get(handle, &mut current_line_coding)
        }) {
            LAST_USB_ERROR.store(error.0, core::sync::atomic::Ordering::Release);
            // SAFETY: close the exclusively owned handle before retrying.
            let _ = unsafe { esp_idf_sys::cdc_acm::cdc_acm_host_close(handle) };
            let _ = self.restore_dma_reserve();
            return Err(error);
        }

        let line_coding = esp_idf_sys::cdc_acm::cdc_acm_line_coding_t {
            dwDTERate: 115_200,
            bCharFormat: 0,
            bParityType: 0,
            bDataBits: 8,
        };
        // SAFETY: `handle` was returned by a successful open and remains
        // exclusively owned here.
        if let Err(error) = esp_result(unsafe {
            esp_idf_sys::cdc_acm::cdc_acm_host_line_coding_set(handle, &line_coding)
        }) {
            LAST_USB_ERROR.store(error.0, core::sync::atomic::Ordering::Release);
            // SAFETY: closing the just-opened exclusive handle is required on
            // configuration failure.
            let _ = unsafe { esp_idf_sys::cdc_acm::cdc_acm_host_close(handle) };
            let _ = self.restore_dma_reserve();
            return Err(error);
        }
        // SAFETY: same live exclusive device handle; DTR/RTS mirrors the
        // verified legacy TONEX ONE initialization.
        if let Err(error) = esp_result(unsafe {
            esp_idf_sys::cdc_acm::cdc_acm_host_set_control_line_state(handle, true, true)
        }) {
            LAST_USB_ERROR.store(error.0, core::sync::atomic::Ordering::Release);
            // SAFETY: closing the just-opened exclusive handle is required on
            // configuration failure.
            let _ = unsafe { esp_idf_sys::cdc_acm::cdc_acm_host_close(handle) };
            let _ = self.restore_dma_reserve();
            return Err(error);
        }

        // Match the proven interval after DTR/RTS setup. `self.device` is not
        // published until this expires, so the application cannot begin its
        // protocol handshake too early.
        std::thread::sleep(std::time::Duration::from_millis(
            TONEX_POST_LINE_STATE_SETTLE_MS,
        ));
        self.device = handle;
        CDC_OPEN_COUNT.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
        LAST_USB_ERROR.store(0, core::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Reports that device opening is unavailable in host tests.
    ///
    /// # Errors
    ///
    /// Always returns the unsupported sentinel.
    #[cfg(not(target_os = "espidf"))]
    pub fn open_tonex_one(&mut self) -> Result<(), EspError> {
        Err(EspError(-1))
    }

    /// Copies received callback bytes into application-owned memory.
    pub fn receive(&mut self, output: &mut [u8]) -> usize {
        #[cfg(target_os = "espidf")]
        {
            RX_RING.drain(output)
        }
        #[cfg(not(target_os = "espidf"))]
        {
            let _ = output;
            0
        }
    }

    #[must_use]
    pub fn dropped_rx_bytes(&self) -> usize {
        #[cfg(target_os = "espidf")]
        {
            RX_RING.dropped()
        }
        #[cfg(not(target_os = "espidf"))]
        {
            0
        }
    }

    /// Returns and clears the number of bytes dropped since the previous
    /// observation. This lets the application recover once without turning a
    /// transient receive burst into a permanent fault condition.
    #[must_use]
    pub fn take_dropped_rx_bytes(&self) -> usize {
        #[cfg(target_os = "espidf")]
        {
            RX_RING.take_dropped()
        }
        #[cfg(not(target_os = "espidf"))]
        {
            0
        }
    }

    /// Discards unread callback data after a receive overflow. The stream
    /// decoder can then resynchronize at the next complete frame boundary.
    pub fn discard_received(&mut self) {
        #[cfg(target_os = "espidf")]
        RX_RING.clear();
    }

    /// Returns true once for each observed device-disconnection event.
    pub fn take_device_disconnected(&mut self) -> bool {
        #[cfg(target_os = "espidf")]
        {
            DEVICE_DISCONNECTED.swap(false, core::sync::atomic::Ordering::AcqRel)
        }
        #[cfg(not(target_os = "espidf"))]
        {
            false
        }
    }

    /// Reports whether the ESP-IDF host completed enumeration far enough to
    /// expose the exact TONEX ONE identity to the CDC client.
    #[must_use]
    pub fn has_seen_tonex_one(&self) -> bool {
        #[cfg(target_os = "espidf")]
        {
            TONEX_DEVICE_SEEN.load(core::sync::atomic::Ordering::Acquire)
        }
        #[cfg(not(target_os = "espidf"))]
        {
            false
        }
    }

    /// Returns monotonic transport counters for the temporary physical
    /// diagnostic display. Product behavior does not depend on these values.
    #[must_use]
    pub fn diagnostic_snapshot(&self) -> UsbDiagnosticSnapshot {
        #[cfg(target_os = "espidf")]
        {
            UsbDiagnosticSnapshot {
                enumerated: ENUMERATOR_OWNED_COUNT.load(core::sync::atomic::Ordering::Acquire),
                opened: CDC_OPEN_COUNT.load(core::sync::atomic::Ordering::Acquire),
                gone: ENUMERATOR_GONE_COUNT.load(core::sync::atomic::Ordering::Acquire),
                last_error: LAST_USB_ERROR.load(core::sync::atomic::Ordering::Acquire),
            }
        }
        #[cfg(not(target_os = "espidf"))]
        {
            UsbDiagnosticSnapshot::default()
        }
    }

    /// Sends one complete framed TONEX message.
    ///
    /// # Errors
    ///
    /// Returns unsupported when no TONEX ONE is open, otherwise returns the
    /// ESP-IDF blocking-transfer result.
    #[cfg(target_os = "espidf")]
    pub fn transmit(&mut self, bytes: &[u8], timeout_ms: u32) -> Result<(), EspError> {
        if self.device.is_null() || bytes.is_empty() {
            return Err(EspError(esp_idf_sys::ESP_ERR_INVALID_STATE));
        }
        // Match the reference controller's 512-byte CDC transmit buffer.
        // Larger future messages remain valid even though ESP-IDF's class
        // client owns only a 512-byte output staging buffer.
        for chunk in bytes.chunks(TONEX_TX_CHUNK_BYTES) {
            // SAFETY: the exclusive handle is live and each immutable chunk
            // remains valid for the duration of this blocking call.
            esp_result(unsafe {
                esp_idf_sys::cdc_acm::cdc_acm_host_data_tx_blocking(
                    self.device,
                    chunk.as_ptr(),
                    chunk.len(),
                    timeout_ms,
                )
            })?;
        }
        Ok(())
    }

    /// Reports that transmission is unavailable in host tests.
    ///
    /// # Errors
    ///
    /// Always returns the unsupported sentinel.
    #[cfg(not(target_os = "espidf"))]
    pub fn transmit(&mut self, _bytes: &[u8], _timeout_ms: u32) -> Result<(), EspError> {
        Err(EspError(-1))
    }

    /// Closes the currently open TONEX ONE device.
    ///
    /// # Errors
    ///
    /// Returns the ESP-IDF close error. Calling this while already closed is
    /// successful.
    #[cfg(target_os = "espidf")]
    pub fn close_tonex_one(&mut self) -> Result<(), EspError> {
        if self.device.is_null() {
            let _ = self.restore_dma_reserve();
            return Ok(());
        }
        let handle = core::mem::replace(&mut self.device, core::ptr::null_mut());
        RX_RING.clear();
        // SAFETY: `handle` was exclusively owned by this stack and is removed
        // before the close call, preventing reuse after close.
        let close_result = esp_result(unsafe { esp_idf_sys::cdc_acm::cdc_acm_host_close(handle) });
        // Preserve the real close result. A monolithic guard can legitimately
        // be unavailable after radio startup, while the CDC driver's freshly
        // freed allocations remain sufficient for the next direct reopen.
        let _ = self.restore_dma_reserve();
        close_result
    }

    /// Closes CDC and releases its client after a physical disconnect so the
    /// raw enumerator can close the gone device in the same order as legacy C.
    ///
    /// # Errors
    ///
    /// Returns the CDC close result. Driver teardown remains owned by RAII.
    #[cfg(target_os = "espidf")]
    pub fn close_disconnected_tonex_one(&mut self) -> Result<(), EspError> {
        let close_result = self.close_tonex_one();
        self.cdc.take();
        close_result
    }

    /// Host-test physical disconnect close is unavailable without ESP-IDF.
    ///
    /// # Errors
    ///
    /// Always returns the unsupported sentinel.
    #[cfg(not(target_os = "espidf"))]
    pub fn close_disconnected_tonex_one(&mut self) -> Result<(), EspError> {
        Err(EspError(-1))
    }

    /// Host-test close operation.
    ///
    /// # Errors
    ///
    /// This host implementation is infallible.
    #[cfg(not(target_os = "espidf"))]
    pub fn close_tonex_one(&mut self) -> Result<(), EspError> {
        Ok(())
    }

    #[cfg(target_os = "espidf")]
    fn restore_dma_reserve(&mut self) -> Result<(), EspError> {
        if self.dma_reserve.is_none() {
            self.dma_reserve = Some(DmaReserve::allocate()?);
        }
        Ok(())
    }
}

#[cfg(target_os = "espidf")]
struct DmaReserve {
    pointer: core::ptr::NonNull<core::ffi::c_void>,
}

#[cfg(target_os = "espidf")]
impl DmaReserve {
    fn allocate() -> Result<Self, EspError> {
        // SAFETY: ESP-IDF returns an exclusively owned allocation or null.
        // The pointer is retained by this owner and freed exactly once.
        let pointer = unsafe {
            esp_idf_sys::heap_caps_malloc(TONEX_DMA_RESERVE_BYTES, esp_idf_sys::MALLOC_CAP_DMA)
        };
        core::ptr::NonNull::new(pointer)
            .map(|pointer| Self { pointer })
            .ok_or(EspError(esp_idf_sys::ESP_ERR_NO_MEM))
    }
}

#[cfg(target_os = "espidf")]
impl Drop for DmaReserve {
    fn drop(&mut self) {
        // SAFETY: `pointer` came from `heap_caps_malloc` and this owner drops
        // it exactly once before CDC requests the reserved contiguous region.
        unsafe { esp_idf_sys::heap_caps_free(self.pointer.as_ptr()) };
    }
}

#[cfg(target_os = "espidf")]
struct PthreadConfigGuard {
    previous: esp_idf_sys::esp_pthread_cfg_t,
}

#[cfg(target_os = "espidf")]
impl PthreadConfigGuard {
    fn usb_worker() -> Result<Self, EspError> {
        // ESP-IDF stores this configuration per creating thread and copies it
        // into the next pthread. Match the legacy USB tasks on core 0 at
        // priority 4 while retaining their small internal stacks.
        // SAFETY: the returned value owns no borrowed data.
        let mut previous = unsafe { esp_idf_sys::esp_pthread_get_default_config() };
        // SAFETY: `previous` is writable for the complete synchronous call.
        let _ = unsafe { esp_idf_sys::esp_pthread_get_cfg(&mut previous) };
        let mut next = previous;
        next.prio = 4;
        next.inherit_cfg = false;
        next.pin_to_core = 0;
        next.stack_alloc_caps = esp_idf_sys::MALLOC_CAP_INTERNAL | esp_idf_sys::MALLOC_CAP_8BIT;
        // SAFETY: `next` is complete and requests byte-addressable stack RAM.
        esp_result(unsafe { esp_idf_sys::esp_pthread_set_cfg(&next) })?;
        Ok(Self { previous })
    }
}

#[cfg(target_os = "espidf")]
impl Drop for PthreadConfigGuard {
    fn drop(&mut self) {
        // SAFETY: restore the configuration copied from this creating thread.
        let _ = unsafe { esp_idf_sys::esp_pthread_set_cfg(&self.previous) };
    }
}

#[cfg(target_os = "espidf")]
struct HostDaemon {
    stop: std::sync::Arc<core::sync::atomic::AtomicBool>,
    events: std::sync::Arc<core::sync::atomic::AtomicU32>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "espidf")]
impl HostDaemon {
    fn start(mut host: UsbHost) -> Result<Self, EspError> {
        let stop = std::sync::Arc::new(core::sync::atomic::AtomicBool::new(false));
        let events = std::sync::Arc::new(core::sync::atomic::AtomicU32::new(0));
        let thread_stop = std::sync::Arc::clone(&stop);
        let thread_events = std::sync::Arc::clone(&events);
        let _pthread_config = PthreadConfigGuard::usb_worker()?;
        let thread = std::thread::Builder::new()
            .name("usb-host".into())
            .stack_size(4 * 1_024)
            .spawn(move || {
                while !thread_stop.load(core::sync::atomic::Ordering::Acquire) {
                    if let Ok(events) = host.poll(u32::MAX) {
                        let mut flags = 0;
                        if events.no_clients {
                            flags |= esp_idf_sys::USB_HOST_LIB_EVENT_FLAGS_NO_CLIENTS;
                        }
                        if events.all_devices_free {
                            flags |= esp_idf_sys::USB_HOST_LIB_EVENT_FLAGS_ALL_FREE;
                        }
                        thread_events.fetch_or(flags, core::sync::atomic::Ordering::AcqRel);
                    }
                }
            })
            .map_err(|_| EspError(-2))?;
        Ok(Self {
            stop,
            events,
            thread: Some(thread),
        })
    }

    fn take_events(&self) -> HostEvents {
        HostEvents::from_flags(self.events.swap(0, core::sync::atomic::Ordering::AcqRel))
    }
}

#[cfg(target_os = "espidf")]
impl Drop for HostDaemon {
    fn drop(&mut self) {
        self.stop.store(true, core::sync::atomic::Ordering::Release);
        // SAFETY: the daemon exclusively owns an installed USB Host library.
        // Unblock only wakes its event call so the thread can observe `stop`.
        let _ = unsafe { esp_idf_sys::usb_host_lib_unblock() };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl hal_contracts::UsbTransport for TonexUsbStack {
    type Error = EspError;

    fn transmit(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.transmit(bytes, TONEX_TX_TIMEOUT_MS)
    }
}

#[cfg(any(target_os = "espidf", test))]
// ByteRing reserves one slot to distinguish full from empty. Keep enough room
// for four 8 KiB callbacks while the display or Web task briefly owns the
// application loop.
const RX_CAPACITY: usize = 32_768;
#[cfg(target_os = "espidf")]
static RX_RING: ring::ByteRing<RX_CAPACITY> = ring::ByteRing::new();
#[cfg(target_os = "espidf")]
static DEVICE_DISCONNECTED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "espidf")]
static TONEX_DEVICE_SEEN: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "espidf")]
static ENUMERATOR_NEW_ADDRESS: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
#[cfg(target_os = "espidf")]
static ENUMERATOR_DEVICE_GONE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "espidf")]
static CDC_DRIVER_ACTIVE: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
#[cfg(target_os = "espidf")]
static ENUMERATOR_OWNED_COUNT: core::sync::atomic::AtomicU16 =
    core::sync::atomic::AtomicU16::new(0);
#[cfg(target_os = "espidf")]
static CDC_OPEN_COUNT: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(0);
#[cfg(target_os = "espidf")]
static ENUMERATOR_GONE_COUNT: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(0);
#[cfg(target_os = "espidf")]
static LAST_USB_ERROR: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

#[cfg(target_os = "espidf")]
unsafe extern "C" fn rx_callback(
    data: *const u8,
    data_len: usize,
    _user_arg: *mut core::ffi::c_void,
) -> bool {
    if data.is_null() || data_len == 0 {
        return true;
    }
    // SAFETY: ESP-IDF guarantees the callback input is readable for
    // `data_len` bytes until this callback returns. The bytes are copied
    // immediately into the bounded ring.
    RX_RING.push(unsafe { core::slice::from_raw_parts(data, data_len) });
    true
}

#[cfg(target_os = "espidf")]
impl Drop for TonexUsbStack {
    fn drop(&mut self) {
        let _ = self.close_tonex_one();
    }
}

#[cfg(target_os = "espidf")]
struct TonexEnumerator {
    stop: std::sync::Arc<core::sync::atomic::AtomicBool>,
    client_address: usize,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "espidf")]
impl TonexEnumerator {
    fn start() -> Result<Self, EspError> {
        TONEX_DEVICE_SEEN.store(false, core::sync::atomic::Ordering::Release);
        DEVICE_DISCONNECTED.store(false, core::sync::atomic::Ordering::Release);
        ENUMERATOR_NEW_ADDRESS.store(0, core::sync::atomic::Ordering::Release);
        ENUMERATOR_DEVICE_GONE.store(false, core::sync::atomic::Ordering::Release);
        ENUMERATOR_OWNED_COUNT.store(0, core::sync::atomic::Ordering::Release);
        CDC_OPEN_COUNT.store(0, core::sync::atomic::Ordering::Release);
        ENUMERATOR_GONE_COUNT.store(0, core::sync::atomic::Ordering::Release);
        LAST_USB_ERROR.store(0, core::sync::atomic::Ordering::Release);

        let async_config =
            esp_idf_sys::cdc_acm::usb_host_client_config_t__bindgen_ty_1__bindgen_ty_1 {
                client_event_callback: Some(enumerator_event_callback),
                callback_arg: core::ptr::null_mut(),
            };
        let config = esp_idf_sys::cdc_acm::usb_host_client_config_t {
            is_synchronous: false,
            max_num_event_msg: 5,
            __bindgen_anon_1: esp_idf_sys::cdc_acm::usb_host_client_config_t__bindgen_ty_1 {
                async_: async_config,
            },
        };
        let mut client = core::ptr::null_mut();
        // SAFETY: Host is installed, the configuration remains valid for the
        // synchronous registration call, and `client` is a writable output.
        esp_result(unsafe {
            esp_idf_sys::cdc_acm::usb_host_client_register(&config, &mut client)
        })?;
        let client_address = client as usize;
        let stop = std::sync::Arc::new(core::sync::atomic::AtomicBool::new(false));
        let thread_stop = std::sync::Arc::clone(&stop);
        let _pthread_config = PthreadConfigGuard::usb_worker()?;
        let thread = std::thread::Builder::new()
            .name("tonex-enumerator".into())
            .stack_size(4 * 1_024)
            .spawn(move || enumerator_task(client_address, &thread_stop));
        match thread {
            Ok(thread) => Ok(Self {
                stop,
                client_address,
                thread: Some(thread),
            }),
            Err(_) => {
                // SAFETY: registration succeeded and no device has been
                // opened because the event task never started.
                let _ = unsafe { esp_idf_sys::cdc_acm::usb_host_client_deregister(client) };
                Err(EspError(esp_idf_sys::ESP_ERR_NO_MEM))
            }
        }
    }
}

#[cfg(target_os = "espidf")]
impl Drop for TonexEnumerator {
    fn drop(&mut self) {
        self.stop.store(true, core::sync::atomic::Ordering::Release);
        let client = self.client_address as esp_idf_sys::cdc_acm::usb_host_client_handle_t;
        // SAFETY: the registered client remains owned until the thread exits;
        // unblocking only wakes its event loop to observe `stop`.
        let _ = unsafe { esp_idf_sys::cdc_acm::usb_host_client_unblock(client) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(target_os = "espidf")]
unsafe extern "C" fn enumerator_event_callback(
    event: *const esp_idf_sys::cdc_acm::usb_host_client_event_msg_t,
    _argument: *mut core::ffi::c_void,
) {
    if event.is_null() {
        return;
    }
    // SAFETY: ESP-IDF guarantees the event and its active union member are
    // valid for this callback invocation.
    let event_type = unsafe { (*event).event };
    if event_type == esp_idf_sys::cdc_acm::usb_host_client_event_t_USB_HOST_CLIENT_EVENT_NEW_DEV {
        // SAFETY: NEW_DEV selects the `new_dev` union member.
        let address = unsafe { (*event).__bindgen_anon_1.new_dev.address };
        let _ = ENUMERATOR_NEW_ADDRESS.compare_exchange(
            0,
            address,
            core::sync::atomic::Ordering::AcqRel,
            core::sync::atomic::Ordering::Acquire,
        );
    } else if event_type
        == esp_idf_sys::cdc_acm::usb_host_client_event_t_USB_HOST_CLIENT_EVENT_DEV_GONE
    {
        ENUMERATOR_GONE_COUNT.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
        TONEX_DEVICE_SEEN.store(false, core::sync::atomic::Ordering::Release);
        ENUMERATOR_DEVICE_GONE.store(true, core::sync::atomic::Ordering::Release);
        DEVICE_DISCONNECTED.store(true, core::sync::atomic::Ordering::Release);
    }
}

#[cfg(target_os = "espidf")]
fn enumerator_task(client_address: usize, stop: &core::sync::atomic::AtomicBool) {
    let client = client_address as esp_idf_sys::cdc_acm::usb_host_client_handle_t;
    let mut raw_device: esp_idf_sys::cdc_acm::usb_device_handle_t = core::ptr::null_mut();
    while !stop.load(core::sync::atomic::Ordering::Acquire) {
        // SAFETY: this thread exclusively services the registered client.
        let _ = unsafe { esp_idf_sys::cdc_acm::usb_host_client_handle_events(client, 1) };

        if raw_device.is_null() {
            let address = ENUMERATOR_NEW_ADDRESS.swap(0, core::sync::atomic::Ordering::AcqRel);
            if address != 0 {
                let mut candidate = core::ptr::null_mut();
                // SAFETY: the address came from this client's NEW_DEV event
                // and the output belongs exclusively to this thread.
                if unsafe {
                    esp_idf_sys::cdc_acm::usb_host_device_open(client, address, &mut candidate)
                } == esp_idf_sys::ESP_OK
                    && !candidate.is_null()
                {
                    if prepare_raw_tonex_device(candidate).is_ok() {
                        raw_device = candidate;
                        ENUMERATOR_OWNED_COUNT.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
                        TONEX_DEVICE_SEEN.store(true, core::sync::atomic::Ordering::Release);
                    } else {
                        // SAFETY: this thread exclusively owns the rejected
                        // device handle and claimed no interfaces.
                        let _ = unsafe {
                            esp_idf_sys::cdc_acm::usb_host_device_close(client, candidate)
                        };
                    }
                }
            }
        }

        if ENUMERATOR_DEVICE_GONE.load(core::sync::atomic::Ordering::Acquire)
            && !CDC_DRIVER_ACTIVE.load(core::sync::atomic::Ordering::Acquire)
        {
            if !raw_device.is_null() {
                // SAFETY: CDC has been closed/uninstalled and this thread is
                // the remaining exclusive owner of the gone raw handle.
                let _ = unsafe { esp_idf_sys::cdc_acm::usb_host_device_close(client, raw_device) };
                raw_device = core::ptr::null_mut();
            }
            ENUMERATOR_DEVICE_GONE.store(false, core::sync::atomic::Ordering::Release);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    if !raw_device.is_null() {
        // SAFETY: shutdown occurs after the enclosing CDC owner is dropped.
        let _ = unsafe { esp_idf_sys::cdc_acm::usb_host_device_close(client, raw_device) };
    }
    // SAFETY: this thread owns the client and has closed its raw device.
    let _ = unsafe { esp_idf_sys::cdc_acm::usb_host_client_deregister(client) };
}

#[cfg(target_os = "espidf")]
fn prepare_raw_tonex_device(
    device: esp_idf_sys::cdc_acm::usb_device_handle_t,
) -> Result<(), EspError> {
    let mut descriptor = core::ptr::null();
    // SAFETY: the enumerator owns a live raw device handle and the output is
    // valid for this synchronous descriptor lookup.
    esp_result(unsafe {
        esp_idf_sys::cdc_acm::usb_host_get_device_descriptor(device, &mut descriptor)
    })?;
    if descriptor.is_null() {
        return Err(EspError(esp_idf_sys::ESP_ERR_INVALID_RESPONSE));
    }
    // SAFETY: the descriptor pointer was validated above and remains cached
    // for the complete lifetime of the owned raw handle.
    let identity = unsafe {
        DeviceIdentity {
            vendor_id: (*descriptor).__bindgen_anon_1.idVendor,
            product_id: (*descriptor).__bindgen_anon_1.idProduct,
        }
    };
    if !identity.is_tonex_one() {
        return Err(EspError(esp_idf_sys::ESP_ERR_NOT_FOUND));
    }

    let mut config = core::ptr::null();
    // SAFETY: same live owned handle and writable output pointer.
    esp_result(unsafe {
        esp_idf_sys::cdc_acm::usb_host_get_active_config_descriptor(device, &mut config)
    })?;
    if config.is_null() {
        return Err(EspError(esp_idf_sys::ESP_ERR_INVALID_RESPONSE));
    }
    // SAFETY: the cached descriptor remains live because the enumerator keeps
    // `device` open through CDC parsing, exactly as the retained C controller.
    unsafe { clamp_tonex_full_speed_endpoints(config) };
    Ok(())
}

#[cfg(target_os = "espidf")]
struct CdcDriver;

#[cfg(target_os = "espidf")]
impl CdcDriver {
    fn install() -> Result<Self, EspError> {
        // SAFETY: the owned `UsbHost` was installed immediately before this
        // call. A null configuration selects the component's documented
        // defaults, and successful creation is represented by one owner.
        esp_result(unsafe { esp_idf_sys::cdc_acm::cdc_acm_host_install(core::ptr::null()) })?;
        CDC_DRIVER_ACTIVE.store(true, core::sync::atomic::Ordering::Release);
        Ok(Self)
    }
}

#[cfg(target_os = "espidf")]
unsafe fn clamp_tonex_full_speed_endpoints(config: *const esp_idf_sys::cdc_acm::usb_config_desc_t) {
    // SAFETY: caller validated the complete cached descriptor pointer.
    let total_length = unsafe { (*config).__bindgen_anon_1.wTotalLength };
    let mut offset = 0;
    let mut current = config.cast::<esp_idf_sys::cdc_acm::usb_standard_desc_t>();

    while !current.is_null() {
        // SAFETY: `current` is either the configuration descriptor or a
        // descriptor returned by ESP-IDF's bounds-aware parser.
        if unsafe { (*current).__bindgen_anon_1.bDescriptorType }
            == esp_idf_sys::cdc_acm::USB_B_DESCRIPTOR_TYPE_ENDPOINT as u8
        {
            let endpoint = current
                .cast_mut()
                .cast::<esp_idf_sys::cdc_acm::usb_ep_desc_t>();
            // SAFETY: the descriptor discriminator proves the endpoint layout.
            // The cached descriptor storage is owned by ESP-IDF and writable,
            // matching the verified legacy workaround.
            if unsafe { (*endpoint).__bindgen_anon_1.wMaxPacketSize } > 64 {
                // SAFETY: the same validated endpoint descriptor remains live
                // and uniquely mutated only for this compatibility clamp.
                unsafe {
                    (*endpoint).__bindgen_anon_1.wMaxPacketSize = 64;
                }
            }
        }
        // SAFETY: ESP-IDF validates the offset against `total_length` and
        // returns null at the end of the descriptor buffer.
        current = unsafe {
            esp_idf_sys::cdc_acm::usb_parse_next_descriptor(current, total_length, &mut offset)
        };
    }
}

#[cfg(target_os = "espidf")]
impl Drop for CdcDriver {
    fn drop(&mut self) {
        // SAFETY: this owner only exists after successful installation. All
        // device owners must be dropped before the enclosing stack.
        let _ = unsafe { esp_idf_sys::cdc_acm::cdc_acm_host_uninstall() };
        CDC_DRIVER_ACTIVE.store(false, core::sync::atomic::Ordering::Release);
    }
}

#[cfg(target_os = "espidf")]
impl Drop for UsbHost {
    fn drop(&mut self) {
        // SAFETY: this type is only constructed after a successful install
        // and uniquely owns the library lifecycle. Shutdown errors cannot be
        // reported from `Drop`; explicit client cleanup must happen first.
        let _ = unsafe { esp_idf_sys::usb_host_uninstall() };
    }
}

#[cfg(target_os = "espidf")]
fn esp_result(code: i32) -> Result<(), EspError> {
    if code == esp_idf_sys::ESP_OK {
        Ok(())
    } else {
        Err(EspError(code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_legacy_host_phy_and_interrupt_configuration() {
        let config = Config::default();
        assert!(!config.skip_phy_setup);
        assert!(!config.root_port_unpowered);
        assert_eq!(config.interrupt_flags, 4);
    }

    #[test]
    fn dma_reserve_preserves_the_large_rx_tail_after_driver_overhead() {
        assert_eq!(TONEX_RX_BUFFER_BYTES, 8_192);
        assert_eq!(TONEX_TX_CHUNK_BYTES, 512);
        assert_eq!(TONEX_DMA_DRIVER_OVERHEAD_BYTES, 256);
        assert_eq!(TONEX_DMA_RESERVE_BYTES, 8_960);
        assert_eq!(RX_CAPACITY / TONEX_RX_BUFFER_BYTES, 4);
    }

    #[test]
    fn connection_settle_intervals_match_the_reference_controller() {
        assert_eq!(TONEX_USB_HOST_STARTUP_SETTLE_MS, 500);
        assert_eq!(TONEX_POST_OPEN_SETTLE_MS, 100);
        assert_eq!(TONEX_POST_LINE_STATE_SETTLE_MS, 250);
        assert_eq!(TONEX_OPEN_TIMEOUT_MS, 1_000);
    }

    #[test]
    fn host_build_refuses_hardware_installation() {
        assert_eq!(
            UsbHost::install(Config::default()).err(),
            Some(EspError(-1))
        );
    }

    #[test]
    fn device_filter_accepts_only_tonex_one() {
        assert!(
            DeviceIdentity {
                vendor_id: IK_MULTIMEDIA_VENDOR_ID,
                product_id: TONEX_ONE_PRODUCT_ID,
            }
            .is_tonex_one()
        );
        assert!(
            !DeviceIdentity {
                vendor_id: IK_MULTIMEDIA_VENDOR_ID,
                product_id: 0x00d0,
            }
            .is_tonex_one()
        );
        assert!(
            !DeviceIdentity {
                vendor_id: 0x0483,
                product_id: TONEX_ONE_PRODUCT_ID,
            }
            .is_tonex_one()
        );
    }
}
