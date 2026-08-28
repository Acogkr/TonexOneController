#![no_std]
//! Narrow ESP-IDF GPIO ownership boundary for board controls.

use tonex_boards::{ControlHardware, SwitchInput};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardIoError {
    Esp(i32),
    IoExpanderRequired { channel: u8 },
    MissingIoExpanderMetadata,
    MissingI2cBusRegistry,
    MixedSwitchBackends,
    I2c(esp_idf_i2c::I2cError),
    LedIndexOutOfRange { index: usize, count: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectGpioPlan {
    pins: [i16; 4],
}

impl DirectGpioPlan {
    /// Resolves a direct-GPIO plan.
    ///
    /// # Errors
    ///
    /// Returns [`BoardIoError::IoExpanderRequired`] when the hardware must be
    /// served by its dedicated expander driver.
    pub fn new(hardware: ControlHardware) -> Result<Self, BoardIoError> {
        let mut pins = [-1; 4];
        let mut index = 0;
        while index < hardware.footswitches.len() {
            pins[index] = match hardware.footswitches[index] {
                SwitchInput::Unavailable => -1,
                SwitchInput::Gpio(pin) => i16::from(pin),
                SwitchInput::IoExpander(channel) => {
                    return Err(BoardIoError::IoExpanderRequired { channel });
                }
            };
            index += 1;
        }
        Ok(Self { pins })
    }

    #[must_use]
    pub const fn pins(self) -> [i16; 4] {
        self.pins
    }
}

#[cfg(target_os = "espidf")]
mod target {
    use core::ptr;

    use esp_idf_i2c::{EspI2cBuses, EspI2cDevice};
    use esp_idf_sys::led_strip::{
        led_color_component_format_t, led_color_component_format_t_format_layout,
        led_model_t_LED_MODEL_WS2812, led_strip_clear, led_strip_config_t, led_strip_del,
        led_strip_handle_t, led_strip_new_rmt_device, led_strip_refresh, led_strip_rmt_config_t,
        led_strip_set_pixel,
    };
    use esp_idf_sys::{
        esp_rom_delay_us, gpio_get_level, gpio_mode_t_GPIO_MODE_INPUT, gpio_num_t,
        gpio_pull_mode_t_GPIO_PULLUP_ONLY, gpio_reset_pin, gpio_set_direction, gpio_set_pull_mode,
    };
    use hal_contracts::{LedDevice, SwitchDevice};
    use tonex_boards::{
        BacklightHardware, ControlHardware, IoExpanderController, LedColourOrder, LedSpec,
        SwitchInput,
    };

    use super::{BoardIoError, DirectGpioPlan};

    #[derive(Debug)]
    pub struct EspDirectSwitches {
        plan: DirectGpioPlan,
    }

    #[derive(Debug)]
    pub struct EspCh422Switches {
        mode: EspI2cDevice,
        input: EspI2cDevice,
        channels: [i8; 4],
    }

    impl EspCh422Switches {
        fn new(hardware: ControlHardware, buses: &EspI2cBuses) -> Result<Self, BoardIoError> {
            let metadata = hardware
                .io_expander
                .ok_or(BoardIoError::MissingIoExpanderMetadata)?;
            if metadata.controller != IoExpanderController::Ch422g {
                return Err(BoardIoError::MissingIoExpanderMetadata);
            }
            let mut channels = [-1_i8; 4];
            for (index, input) in hardware.footswitches.into_iter().enumerate() {
                channels[index] = match input {
                    SwitchInput::Unavailable => -1,
                    SwitchInput::IoExpander(channel) => {
                        i8::try_from(channel).map_err(|_| BoardIoError::MixedSwitchBackends)?
                    }
                    SwitchInput::Gpio(_) => return Err(BoardIoError::MixedSwitchBackends),
                };
            }

            let bus = buses.get(metadata.i2c_port).map_err(BoardIoError::I2c)?;
            let mode = bus
                .add_device(0x24, metadata.frequency_hz)
                .map_err(BoardIoError::I2c)?;
            let input = bus
                .add_device(0x26, metadata.frequency_hz)
                .map_err(BoardIoError::I2c)?;
            Ok(Self {
                mode,
                input,
                channels,
            })
        }

        fn read_all(&mut self) -> Result<u8, BoardIoError> {
            let input_mode = [0_u8];
            // SAFETY: handle is live and the borrowed byte covers the complete
            // synchronous transfer.
            self.mode
                .transmit(&input_mode, 10)
                .map_err(BoardIoError::I2c)?;
            // SAFETY: Mirrors the reference device's required mode-set delay.
            unsafe { esp_rom_delay_us(1_000) };

            let mut ignored = 0_u8;
            // SAFETY: output pointer references one writable byte.
            self.mode
                .receive(core::slice::from_mut(&mut ignored), 10)
                .map_err(BoardIoError::I2c)?;
            let mut values = 0_u8;
            let read_result = self
                .input
                .receive(core::slice::from_mut(&mut values), 10)
                .map_err(BoardIoError::I2c);
            let output_mode = [1_u8];
            let restore_result = self
                .mode
                .transmit(&output_mode, 10)
                .map_err(BoardIoError::I2c);
            read_result?;
            restore_result?;
            Ok(values)
        }
    }

    impl SwitchDevice for EspCh422Switches {
        type Error = BoardIoError;

        fn pressed_mask(&mut self) -> Result<u32, Self::Error> {
            let values = self.read_all()?;
            let mut pressed = 0_u32;
            for (index, channel) in self.channels.into_iter().enumerate() {
                if channel >= 0 && values & (1_u8 << channel) == 0 {
                    pressed |= 1_u32 << index;
                }
            }
            Ok(pressed)
        }
    }

    #[derive(Debug)]
    pub enum EspBoardSwitches {
        Direct(EspDirectSwitches),
        Ch422(EspCh422Switches),
    }

    impl EspBoardSwitches {
        /// Constructs the backend declared by the board descriptor.
        ///
        /// # Errors
        ///
        /// Returns descriptor inconsistencies and ESP-IDF setup failures.
        pub fn new(
            hardware: ControlHardware,
            buses: Option<&EspI2cBuses>,
        ) -> Result<Self, BoardIoError> {
            if hardware.io_expander.is_some() {
                let buses = buses.ok_or(BoardIoError::MissingI2cBusRegistry)?;
                EspCh422Switches::new(hardware, buses).map(Self::Ch422)
            } else {
                DirectGpioPlan::new(hardware)
                    .and_then(EspDirectSwitches::new)
                    .map(Self::Direct)
            }
        }
    }

    impl SwitchDevice for EspBoardSwitches {
        type Error = BoardIoError;

        fn pressed_mask(&mut self) -> Result<u32, Self::Error> {
            match self {
                Self::Direct(switches) => switches.pressed_mask(),
                Self::Ch422(switches) => switches.pressed_mask(),
            }
        }
    }

    #[derive(Debug)]
    pub struct EspLedStrip {
        handle: led_strip_handle_t,
        count: u8,
    }

    #[derive(Debug)]
    pub struct EspLp5562Backlight {
        device: EspI2cDevice,
        maximum_white: u8,
    }

    impl EspLp5562Backlight {
        /// Powers the LP5562 and enables its white backlight channel.
        ///
        /// # Errors
        ///
        /// Returns I2C bus, device, or register-transfer errors.
        pub fn new(hardware: BacklightHardware, buses: &EspI2cBuses) -> Result<Self, BoardIoError> {
            let BacklightHardware::Lp5562 {
                i2c_port,
                address,
                frequency_hz,
                white,
            } = hardware;
            let bus = buses.get(i2c_port).map_err(BoardIoError::I2c)?;
            let device = bus
                .add_device(address, frequency_hz)
                .map_err(BoardIoError::I2c)?;
            let mut backlight = Self {
                device,
                maximum_white: white,
            };
            backlight.write(0x00, 0x40)?;
            // SAFETY: the LP5562 datasheet requires a 1 ms startup delay.
            unsafe { esp_rom_delay_us(1_000) };
            backlight.write(0x08, 0x41)?;
            backlight.write(0x70, 0)?;
            backlight.write(0x0e, white)?;
            Ok(backlight)
        }

        /// Sets the white channel relative to the board descriptor's safe maximum.
        ///
        /// # Errors
        ///
        /// Returns an I²C error when the LP5562 register write fails.
        pub fn set_percent(&mut self, percent: u8) -> Result<(), BoardIoError> {
            let percent = percent.min(100);
            let value = (u16::from(self.maximum_white) * u16::from(percent) + 50) / 100;
            self.write(
                0x0e,
                u8::try_from(value).expect("scaled LP5562 brightness fits one byte"),
            )
        }

        fn write(&mut self, register: u8, value: u8) -> Result<(), BoardIoError> {
            self.device
                .transmit(&[register, value], 10)
                .map_err(BoardIoError::I2c)
        }
    }

    impl EspLedStrip {
        /// Creates the RMT-backed WS2812 strip declared by a board variant.
        ///
        /// # Errors
        ///
        /// Returns an ESP-IDF driver construction or initial clear failure.
        pub fn new(spec: LedSpec) -> Result<Self, BoardIoError> {
            let mut layout = led_color_component_format_t_format_layout::default();
            let (red, green, blue) = match spec.colour_order {
                LedColourOrder::Rgb => (0, 1, 2),
                LedColourOrder::Grb => (1, 0, 2),
                LedColourOrder::Gbr => (2, 0, 1),
            };
            layout.set_r_pos(red);
            layout.set_g_pos(green);
            layout.set_b_pos(blue);
            layout.set_w_pos(3);
            layout.set_bytes_per_color(1);
            layout.set_num_components(3);
            let config = led_strip_config_t {
                strip_gpio_num: i32::from(spec.gpio),
                max_leds: u32::from(spec.count),
                led_model: led_model_t_LED_MODEL_WS2812,
                color_component_format: led_color_component_format_t { format: layout },
                ..Default::default()
            };
            let rmt = led_strip_rmt_config_t {
                resolution_hz: 10_000_000,
                mem_block_symbols: 64,
                ..Default::default()
            };
            let mut handle = ptr::null_mut();
            // SAFETY: configurations and output storage stay valid throughout
            // the synchronous constructor; the returned handle is unique.
            check(unsafe {
                led_strip_new_rmt_device(&raw const config, &raw const rmt, &raw mut handle)
            })?;
            let strip = Self {
                handle,
                count: spec.count,
            };
            // SAFETY: handle is live and uniquely owned by strip.
            if let Err(error) = check(unsafe { led_strip_clear(strip.handle) }) {
                // SAFETY: constructor rollback consumes the unique handle.
                let _ = unsafe { led_strip_del(strip.handle) };
                return Err(error);
            }
            Ok(strip)
        }
    }

    impl LedDevice for EspLedStrip {
        type Error = BoardIoError;

        fn set_rgb(
            &mut self,
            index: usize,
            red: u8,
            green: u8,
            blue: u8,
        ) -> Result<(), Self::Error> {
            if index >= usize::from(self.count) {
                return Err(BoardIoError::LedIndexOutOfRange {
                    index,
                    count: self.count,
                });
            }
            let index = u32::try_from(index).map_err(|_| BoardIoError::LedIndexOutOfRange {
                index,
                count: self.count,
            })?;
            // SAFETY: handle is live and index is within the configured strip.
            check(unsafe {
                led_strip_set_pixel(
                    self.handle,
                    index,
                    u32::from(red),
                    u32::from(green),
                    u32::from(blue),
                )
            })
        }

        fn present(&mut self) -> Result<(), Self::Error> {
            // SAFETY: refresh is synchronous and uses the same live handle.
            check(unsafe { led_strip_refresh(self.handle) })
        }
    }

    impl Drop for EspLedStrip {
        fn drop(&mut self) {
            // SAFETY: handle remains unique and live until this drop.
            let _ = unsafe { led_strip_clear(self.handle) };
            // SAFETY: this is the final use of the unique handle.
            let _ = unsafe { led_strip_del(self.handle) };
        }
    }

    impl EspDirectSwitches {
        /// Configures every available input as active-low with an internal
        /// pull-up.
        ///
        /// # Errors
        ///
        /// Returns the first ESP-IDF GPIO configuration failure.
        pub fn new(plan: DirectGpioPlan) -> Result<Self, BoardIoError> {
            for pin in plan.pins().into_iter().filter(|pin| *pin >= 0) {
                let pin = gpio_num_t::from(pin);
                // SAFETY: DirectGpioPlan contains only board-owned GPIO
                // numbers. This crate is the sole raw GPIO ownership boundary.
                let result = unsafe { gpio_reset_pin(pin) };
                check(result)?;
                // SAFETY: The pin was reset above and remains owned here.
                let result = unsafe { gpio_set_direction(pin, gpio_mode_t_GPIO_MODE_INPUT) };
                check(result)?;
                // SAFETY: Applying a pull mode does not outlive or alias the
                // owned GPIO.
                let result = unsafe { gpio_set_pull_mode(pin, gpio_pull_mode_t_GPIO_PULLUP_ONLY) };
                check(result)?;
            }
            Ok(Self { plan })
        }
    }

    impl SwitchDevice for EspDirectSwitches {
        type Error = BoardIoError;

        fn pressed_mask(&mut self) -> Result<u32, Self::Error> {
            let mut pressed = 0_u32;
            for (index, pin) in self.plan.pins().into_iter().enumerate() {
                if pin >= 0 {
                    // SAFETY: All configured pins remain exclusively owned by
                    // this driver for its lifetime.
                    if unsafe { gpio_get_level(gpio_num_t::from(pin)) } == 0 {
                        pressed |= 1_u32 << index;
                    }
                }
            }
            Ok(pressed)
        }
    }

    fn check(result: i32) -> Result<(), BoardIoError> {
        if result == 0 {
            Ok(())
        } else {
            Err(BoardIoError::Esp(result))
        }
    }

    pub use EspBoardSwitches as Switches;
}

#[cfg(target_os = "espidf")]
pub use target::EspLedStrip;
#[cfg(target_os = "espidf")]
pub use target::EspLp5562Backlight;
#[cfg(target_os = "espidf")]
pub use target::Switches as EspBoardSwitches;

#[cfg(test)]
mod tests {
    use super::*;
    use tonex_boards::{DEVKITC, WAVESHARE_43B};

    #[test]
    fn direct_plan_preserves_gpio_order() {
        assert_eq!(
            DirectGpioPlan::new(DEVKITC.controls)
                .expect("devkit uses direct GPIO")
                .pins(),
            [4, 6, 2, 1]
        );
    }

    #[test]
    fn expander_profiles_cannot_silently_use_gpio() {
        assert_eq!(
            DirectGpioPlan::new(WAVESHARE_43B.controls),
            Err(BoardIoError::IoExpanderRequired { channel: 1 })
        );
    }
}
