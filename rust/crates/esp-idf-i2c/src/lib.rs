#![cfg_attr(target_os = "espidf", allow(unsafe_code))]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum I2cError {
    Esp(i32),
    MissingPort(u8),
}

#[cfg(target_os = "espidf")]
mod target {
    use core::ptr;
    use std::{sync::Arc, vec::Vec};

    use esp_idf_sys::{
        gpio_num_t, i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7, i2c_del_master_bus, i2c_device_config_t,
        i2c_master_bus_add_device, i2c_master_bus_config_t, i2c_master_bus_config_t__bindgen_ty_1,
        i2c_master_bus_handle_t, i2c_master_bus_rm_device, i2c_master_dev_handle_t,
        i2c_master_probe, i2c_master_receive, i2c_master_transmit, i2c_master_transmit_receive,
        i2c_new_master_bus, soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT,
    };
    use tonex_boards::{HardwareProfile, I2cBusHardware};

    use super::I2cError;

    #[derive(Clone, Debug)]
    pub struct EspI2cBus {
        inner: Arc<BusInner>,
    }

    #[derive(Debug)]
    struct BusInner {
        port: u8,
        handle: i2c_master_bus_handle_t,
    }

    // SAFETY: ESP-IDF's new I2C master bus API serializes operations internally;
    // the handle remains live through Arc and is deleted after all devices.
    unsafe impl Send for BusInner {}
    // SAFETY: the same ESP-IDF thread-safety and Arc lifetime guarantees apply.
    unsafe impl Sync for BusInner {}

    impl EspI2cBus {
        /// Creates the source-described ESP-IDF master bus.
        ///
        /// # Errors
        ///
        /// Returns the ESP-IDF construction error.
        pub fn new(spec: I2cBusHardware) -> Result<Self, I2cError> {
            let mut config = i2c_master_bus_config_t {
                i2c_port: i32::from(spec.port),
                sda_io_num: gpio_num_t::from(spec.sda_gpio),
                scl_io_num: gpio_num_t::from(spec.scl_gpio),
                __bindgen_anon_1: i2c_master_bus_config_t__bindgen_ty_1 {
                    clk_source: soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT,
                },
                glitch_ignore_cnt: 7,
                ..Default::default()
            };
            config.flags.set_enable_internal_pullup(1);
            let mut handle = ptr::null_mut();
            // SAFETY: config and output storage are valid for this synchronous call.
            check(unsafe { i2c_new_master_bus(&raw const config, &raw mut handle) })?;
            Ok(Self {
                inner: Arc::new(BusInner {
                    port: spec.port,
                    handle,
                }),
            })
        }

        #[must_use]
        pub fn port(&self) -> u8 {
            self.inner.port
        }

        /// Adds one addressed device owned by the returned handle.
        ///
        /// # Errors
        ///
        /// Returns an ESP-IDF allocation or configuration error.
        pub fn add_device(
            &self,
            address: u16,
            frequency_hz: u32,
        ) -> Result<EspI2cDevice, I2cError> {
            let config = i2c_device_config_t {
                dev_addr_length: i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7,
                device_address: address,
                scl_speed_hz: frequency_hz,
                ..Default::default()
            };
            let mut handle = ptr::null_mut();
            // SAFETY: the Arc keeps the bus live and output storage is valid.
            check(unsafe {
                i2c_master_bus_add_device(self.inner.handle, &raw const config, &raw mut handle)
            })?;
            Ok(EspI2cDevice {
                bus: Arc::clone(&self.inner),
                handle,
            })
        }

        #[must_use]
        pub fn probe(&self, address: u16, timeout_ms: i32) -> bool {
            // SAFETY: self keeps the bus handle live for this synchronous probe.
            (unsafe { i2c_master_probe(self.inner.handle, address, timeout_ms) }) == 0
        }
    }

    impl Drop for BusInner {
        fn drop(&mut self) {
            // SAFETY: Arc proves all device owners have been dropped first.
            let _ = unsafe { i2c_del_master_bus(self.handle) };
        }
    }

    #[derive(Debug)]
    pub struct EspI2cDevice {
        bus: Arc<BusInner>,
        handle: i2c_master_dev_handle_t,
    }

    // SAFETY: the device handle has unique ownership and ESP-IDF permits its
    // synchronous operations from whichever task currently owns it.
    unsafe impl Send for EspI2cDevice {}

    impl EspI2cDevice {
        /// Sends one complete transaction.
        ///
        /// # Errors
        ///
        /// Returns the ESP-IDF transfer error.
        pub fn transmit(&mut self, bytes: &[u8], timeout_ms: i32) -> Result<(), I2cError> {
            // SAFETY: handle is live and bytes remains borrowed for the synchronous call.
            check(unsafe {
                i2c_master_transmit(self.handle, bytes.as_ptr(), bytes.len(), timeout_ms)
            })
        }

        /// Receives one complete transaction.
        ///
        /// # Errors
        ///
        /// Returns the ESP-IDF transfer error.
        pub fn receive(&mut self, bytes: &mut [u8], timeout_ms: i32) -> Result<(), I2cError> {
            // SAFETY: handle is live and bytes is uniquely borrowed for the call.
            check(unsafe {
                i2c_master_receive(self.handle, bytes.as_mut_ptr(), bytes.len(), timeout_ms)
            })
        }

        /// Performs a repeated-start register write/read transaction.
        ///
        /// # Errors
        ///
        /// Returns the ESP-IDF transfer error.
        pub fn transmit_receive(
            &mut self,
            write: &[u8],
            read: &mut [u8],
            timeout_ms: i32,
        ) -> Result<(), I2cError> {
            // SAFETY: both non-overlapping slices remain valid for the synchronous call.
            check(unsafe {
                i2c_master_transmit_receive(
                    self.handle,
                    write.as_ptr(),
                    write.len(),
                    read.as_mut_ptr(),
                    read.len(),
                    timeout_ms,
                )
            })
        }
    }

    impl Drop for EspI2cDevice {
        fn drop(&mut self) {
            // SAFETY: this is the final use of the unique device handle.
            let _ = unsafe { i2c_master_bus_rm_device(self.handle) };
            let _ = &self.bus;
        }
    }

    #[derive(Debug)]
    pub struct EspI2cBuses {
        buses: Vec<EspI2cBus>,
    }

    impl EspI2cBuses {
        /// Creates every bus declared by a hardware profile.
        ///
        /// # Errors
        ///
        /// Returns the first bus construction error.
        pub fn new(profile: &HardwareProfile) -> Result<Self, I2cError> {
            let mut buses = Vec::with_capacity(usize::from(profile.i2c_bus_count));
            for spec in profile.i2c_buses.into_iter().flatten() {
                buses.push(EspI2cBus::new(spec)?);
            }
            Ok(Self { buses })
        }

        /// Clones the owner for a configured bus port.
        ///
        /// # Errors
        ///
        /// Returns [`I2cError::MissingPort`] for an undeclared port.
        pub fn get(&self, port: u8) -> Result<EspI2cBus, I2cError> {
            self.buses
                .iter()
                .find(|bus| bus.port() == port)
                .cloned()
                .ok_or(I2cError::MissingPort(port))
        }
    }

    fn check(result: i32) -> Result<(), I2cError> {
        if result == 0 {
            Ok(())
        } else {
            Err(I2cError::Esp(result))
        }
    }
}

#[cfg(target_os = "espidf")]
pub use target::{EspI2cBus, EspI2cBuses, EspI2cDevice};
