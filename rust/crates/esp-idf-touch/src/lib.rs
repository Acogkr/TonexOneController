#![cfg_attr(target_os = "espidf", allow(unsafe_code))]

use hal_contracts::TouchPoint;
use tonex_boards::TouchElectrical;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchError {
    UnsupportedBoard,
    UnsupportedResetPin,
    NoDevice,
    I2c(esp_idf_i2c::I2cError),
    Esp(i32),
}

#[cfg(any(target_os = "espidf", test))]
fn decode_axs15231b_point(data: [u8; 8]) -> Option<TouchPoint> {
    // Byte 1 is the point count in Espressif's AXS15231B record format.
    // Byte 0 is the gesture field and must not be used as touch status.
    if data[1] != 1 {
        return None;
    }
    Some(TouchPoint {
        x: (u16::from(data[2] & 0x0f) << 8) | u16::from(data[3]),
        y: (u16::from(data[4] & 0x0f) << 8) | u16::from(data[5]),
    })
}

#[must_use]
pub fn map_touch(
    mut point: TouchPoint,
    native_width: u16,
    native_height: u16,
    logical_width: u16,
    logical_height: u16,
    transform: TouchElectrical,
) -> Option<TouchPoint> {
    if point.x >= native_width || point.y >= native_height {
        return None;
    }
    if transform.mirror_x {
        point.x = native_width - 1 - point.x;
    }
    if transform.mirror_y {
        point.y = native_height - 1 - point.y;
    }
    let (mut mapped_width, mut mapped_height) = (native_width, native_height);
    if transform.swap_xy {
        core::mem::swap(&mut point.x, &mut point.y);
        core::mem::swap(&mut mapped_width, &mut mapped_height);
    }
    if logical_width == mapped_height && logical_height == mapped_width {
        core::mem::swap(&mut point.x, &mut point.y);
        core::mem::swap(&mut mapped_width, &mut mapped_height);
    }
    (mapped_width == logical_width
        && mapped_height == logical_height
        && point.x < logical_width
        && point.y < logical_height)
        .then_some(point)
}

#[cfg(target_os = "espidf")]
mod target {
    use std::time::Duration;

    use esp_idf_i2c::{EspI2cBuses, EspI2cDevice};
    use esp_idf_sys::{
        gpio_mode_t_GPIO_MODE_INPUT, gpio_mode_t_GPIO_MODE_OUTPUT, gpio_num_t, gpio_reset_pin,
        gpio_set_direction, gpio_set_level,
    };
    use hal_contracts::{TouchDevice, TouchPoint};
    use tonex_boards::{BoardDescriptor, PeripheralPin, TouchController, TouchElectrical};

    use super::{TouchError, map_touch};

    #[derive(Debug)]
    pub struct EspCst816 {
        device: EspI2cDevice,
        logical_width: u16,
        logical_height: u16,
        transform: TouchElectrical,
    }

    impl EspCst816 {
        /// Creates a CST816 device for a compatible board descriptor.
        ///
        /// # Errors
        ///
        /// Returns descriptor, GPIO, I2C bus, or device setup errors.
        pub fn new(board: &BoardDescriptor, buses: &EspI2cBuses) -> Result<Self, TouchError> {
            Self::build(board, buses, true)
        }

        fn build(
            board: &BoardDescriptor,
            buses: &EspI2cBuses,
            perform_reset: bool,
        ) -> Result<Self, TouchError> {
            let hardware = board.hardware.display.ok_or(TouchError::UnsupportedBoard)?;
            if !matches!(
                hardware.touch,
                TouchController::Cst816 | TouchController::Cst816OrCst328
            ) {
                return Err(TouchError::UnsupportedBoard);
            }
            let logical = board.display.ok_or(TouchError::UnsupportedBoard)?;
            let transform = hardware
                .touch_electrical
                .ok_or(TouchError::UnsupportedBoard)?;
            if perform_reset {
                reset(transform, buses)?;
            }
            let bus = buses.get(transform.i2c_port).map_err(TouchError::I2c)?;
            let device = bus
                .add_device(transform.addresses[0], transform.frequency_hz)
                .map_err(TouchError::I2c)?;
            Ok(Self {
                device,
                logical_width: logical.width,
                logical_height: logical.height,
                transform,
            })
        }
    }

    impl TouchDevice for EspCst816 {
        type Error = TouchError;

        fn poll(&mut self) -> Result<Option<TouchPoint>, Self::Error> {
            let mut data = [0_u8; 5];
            self.device
                .transmit_receive(&[0x02], &mut data, 10)
                .map_err(TouchError::I2c)?;
            if data[0] % 16 == 0 {
                return Ok(None);
            }
            let point = TouchPoint {
                x: (u16::from(data[1] & 0x0f) << 8) | u16::from(data[2]),
                y: (u16::from(data[3] & 0x0f) << 8) | u16::from(data[4]),
            };
            Ok(map_touch(
                point,
                self.transform.coordinate_width,
                self.transform.coordinate_height,
                self.logical_width,
                self.logical_height,
                self.transform,
            ))
        }
    }

    #[derive(Debug)]
    pub struct EspCst328 {
        device: EspI2cDevice,
        logical_width: u16,
        logical_height: u16,
        transform: TouchElectrical,
    }

    impl EspCst328 {
        fn new(board: &BoardDescriptor, buses: &EspI2cBuses) -> Result<Self, TouchError> {
            let hardware = board.hardware.display.ok_or(TouchError::UnsupportedBoard)?;
            let logical = board.display.ok_or(TouchError::UnsupportedBoard)?;
            let transform = hardware
                .touch_electrical
                .ok_or(TouchError::UnsupportedBoard)?;
            let bus = buses.get(transform.i2c_port).map_err(TouchError::I2c)?;
            let device = bus
                .add_device(0x1a, transform.frequency_hz)
                .map_err(TouchError::I2c)?;
            Ok(Self {
                device,
                logical_width: logical.width,
                logical_height: logical.height,
                transform,
            })
        }
    }

    impl TouchDevice for EspCst328 {
        type Error = TouchError;

        fn poll(&mut self) -> Result<Option<TouchPoint>, Self::Error> {
            let mut count = [0_u8; 1];
            self.device
                .transmit_receive(&[0xd0, 0x05], &mut count, 10)
                .map_err(TouchError::I2c)?;
            let points = count[0] & 0x0f;
            if points == 0 || points > 5 {
                self.device
                    .transmit(&[0xd0, 0x05, 0], 10)
                    .map_err(TouchError::I2c)?;
                return Ok(None);
            }
            let mut data = [0_u8; 27];
            self.device
                .transmit_receive(&[0xd0, 0x00], &mut data, 10)
                .map_err(TouchError::I2c)?;
            self.device
                .transmit(&[0xd0, 0x05, 0], 10)
                .map_err(TouchError::I2c)?;
            let point = TouchPoint {
                x: (u16::from(data[1]) << 4) | u16::from(data[3] >> 4),
                y: (u16::from(data[2]) << 4) | u16::from(data[3] & 0x0f),
            };
            Ok(map_point(
                point,
                self.logical_width,
                self.logical_height,
                self.transform,
            ))
        }
    }

    #[derive(Debug)]
    pub struct EspGt911 {
        device: EspI2cDevice,
        logical_width: u16,
        logical_height: u16,
        transform: TouchElectrical,
    }

    impl EspGt911 {
        fn new(board: &BoardDescriptor, buses: &EspI2cBuses) -> Result<Self, TouchError> {
            let hardware = board.hardware.display.ok_or(TouchError::UnsupportedBoard)?;
            let logical = board.display.ok_or(TouchError::UnsupportedBoard)?;
            let transform = hardware
                .touch_electrical
                .ok_or(TouchError::UnsupportedBoard)?;
            let bus = buses.get(transform.i2c_port).map_err(TouchError::I2c)?;
            let address = transform.addresses[..usize::from(transform.address_count)]
                .iter()
                .copied()
                .find(|address| bus.probe(*address, 50))
                .ok_or(TouchError::NoDevice)?;
            let device = bus
                .add_device(address, transform.frequency_hz)
                .map_err(TouchError::I2c)?;
            Ok(Self {
                device,
                logical_width: logical.width,
                logical_height: logical.height,
                transform,
            })
        }
    }

    impl TouchDevice for EspGt911 {
        type Error = TouchError;

        fn poll(&mut self) -> Result<Option<TouchPoint>, Self::Error> {
            let mut status = [0_u8; 1];
            self.device
                .transmit_receive(&[0x81, 0x4e], &mut status, 10)
                .map_err(TouchError::I2c)?;
            if status[0] & 0x80 == 0 || status[0] % 16 == 0 {
                return Ok(None);
            }
            let mut data = [0_u8; 4];
            self.device
                .transmit_receive(&[0x81, 0x50], &mut data, 10)
                .map_err(TouchError::I2c)?;
            self.device
                .transmit(&[0x81, 0x4e, 0], 10)
                .map_err(TouchError::I2c)?;
            let point = TouchPoint {
                x: u16::from_le_bytes([data[0], data[1]]),
                y: u16::from_le_bytes([data[2], data[3]]),
            };
            Ok(map_point(
                point,
                self.logical_width,
                self.logical_height,
                self.transform,
            ))
        }
    }

    #[derive(Debug)]
    pub struct EspAxs15231b {
        device: EspI2cDevice,
        logical_width: u16,
        logical_height: u16,
        transform: TouchElectrical,
        #[cfg(feature = "diagnostics")]
        last_data: [u8; 8],
    }

    impl EspAxs15231b {
        fn new(board: &BoardDescriptor, buses: &EspI2cBuses) -> Result<Self, TouchError> {
            let hardware = board.hardware.display.ok_or(TouchError::UnsupportedBoard)?;
            let logical = board.display.ok_or(TouchError::UnsupportedBoard)?;
            let transform = hardware
                .touch_electrical
                .ok_or(TouchError::UnsupportedBoard)?;
            let bus = buses.get(transform.i2c_port).map_err(TouchError::I2c)?;
            let address = transform.addresses[0];
            if !bus.probe(address, 50) {
                return Err(TouchError::NoDevice);
            }
            let device = bus
                .add_device(address, transform.frequency_hz)
                .map_err(TouchError::I2c)?;
            Ok(Self {
                device,
                logical_width: logical.width,
                logical_height: logical.height,
                transform,
                #[cfg(feature = "diagnostics")]
                last_data: [0; 8],
            })
        }
    }

    impl TouchDevice for EspAxs15231b {
        type Error = TouchError;

        fn poll(&mut self) -> Result<Option<TouchPoint>, Self::Error> {
            const TRANSFER_TIMEOUT_MS: i32 = 100;
            const READ_COMMAND: [u8; 11] =
                [0xb5, 0xab, 0xa5, 0x5a, 0x00, 0x00, 0x00, 0x08, 0, 0, 0];
            let mut data = [0_u8; 8];
            // The controller requires two complete I2C transactions, matching
            // Espressif's reference driver: command write + STOP, then read.
            // A repeated-start transmit/receive transaction is rejected by the
            // physical JC3248W535 controller.
            self.device
                .transmit(&READ_COMMAND, TRANSFER_TIMEOUT_MS)
                .map_err(TouchError::I2c)?;
            self.device
                .receive(&mut data, TRANSFER_TIMEOUT_MS)
                .map_err(TouchError::I2c)?;
            #[cfg(feature = "diagnostics")]
            if data != self.last_data {
                println!("TOUCH_RAW bytes={data:02x?}");
                self.last_data = data;
            }
            let Some(point) = super::decode_axs15231b_point(data) else {
                return Ok(None);
            };
            Ok(map_point(
                point,
                self.logical_width,
                self.logical_height,
                self.transform,
            ))
        }
    }

    #[derive(Debug)]
    pub enum EspTouch {
        Cst816(EspCst816),
        Cst328(EspCst328),
        Gt911(EspGt911),
        Axs15231b(EspAxs15231b),
    }

    impl EspTouch {
        /// Auto-selects the supported touch controller declared by the board.
        ///
        /// # Errors
        ///
        /// Returns descriptor, reset, probe, or I2C setup errors.
        pub fn new(board: &BoardDescriptor, buses: &EspI2cBuses) -> Result<Self, TouchError> {
            let hardware = board.hardware.display.ok_or(TouchError::UnsupportedBoard)?;
            match hardware.touch {
                TouchController::Cst816 => EspCst816::new(board, buses).map(Self::Cst816),
                TouchController::Cst816OrCst328 => {
                    let transform = hardware
                        .touch_electrical
                        .ok_or(TouchError::UnsupportedBoard)?;
                    reset(transform, buses)?;
                    let bus = buses.get(transform.i2c_port).map_err(TouchError::I2c)?;
                    if bus.probe(0x15, 50) {
                        EspCst816::build(board, buses, false).map(Self::Cst816)
                    } else if bus.probe(0x1a, 50) {
                        EspCst328::new(board, buses).map(Self::Cst328)
                    } else {
                        Err(TouchError::NoDevice)
                    }
                }
                TouchController::Gt911 => EspGt911::new(board, buses).map(Self::Gt911),
                TouchController::Axs15231b => EspAxs15231b::new(board, buses).map(Self::Axs15231b),
                TouchController::None => Err(TouchError::UnsupportedBoard),
            }
        }
    }

    impl TouchDevice for EspTouch {
        type Error = TouchError;

        fn poll(&mut self) -> Result<Option<TouchPoint>, Self::Error> {
            match self {
                Self::Cst816(touch) => touch.poll(),
                Self::Cst328(touch) => touch.poll(),
                Self::Gt911(touch) => touch.poll(),
                Self::Axs15231b(touch) => touch.poll(),
            }
        }
    }

    fn map_point(
        point: TouchPoint,
        logical_width: u16,
        logical_height: u16,
        transform: TouchElectrical,
    ) -> Option<TouchPoint> {
        map_touch(
            point,
            transform.coordinate_width,
            transform.coordinate_height,
            logical_width,
            logical_height,
            transform,
        )
    }

    fn reset(transform: TouchElectrical, buses: &EspI2cBuses) -> Result<(), TouchError> {
        let select_pin = if transform.address_count > 1 {
            match transform.interrupt {
                Some(PeripheralPin::Gpio(pin)) => Some(gpio_num_t::from(pin)),
                _ => None,
            }
        } else {
            None
        };
        if let Some(pin) = select_pin {
            // SAFETY: the descriptor assigns this GPIO to touch interrupt/address select.
            check(unsafe { gpio_reset_pin(pin) })?;
            check(unsafe { gpio_set_direction(pin, gpio_mode_t_GPIO_MODE_OUTPUT) })?;
            check(unsafe { gpio_set_level(pin, 0) })?;
        }
        let Some(pin) = transform.reset else {
            release_select_pin(select_pin)?;
            return Ok(());
        };
        match pin {
            PeripheralPin::Gpio(pin) => reset_gpio(pin)?,
            PeripheralPin::IoExpander(channel) => {
                let bus = buses.get(transform.i2c_port).map_err(TouchError::I2c)?;
                let mut mode = bus
                    .add_device(0x24, transform.frequency_hz)
                    .map_err(TouchError::I2c)?;
                let mut output = bus
                    .add_device(0x38, transform.frequency_hz)
                    .map_err(TouchError::I2c)?;
                mode.transmit(&[1], 10).map_err(TouchError::I2c)?;
                let bit = 1_u8
                    .checked_shl(u32::from(channel))
                    .ok_or(TouchError::UnsupportedResetPin)?;
                output.transmit(&[!bit], 10).map_err(TouchError::I2c)?;
                std::thread::sleep(Duration::from_millis(20));
                output.transmit(&[0xff], 10).map_err(TouchError::I2c)?;
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        release_select_pin(select_pin)?;
        Ok(())
    }

    fn reset_gpio(pin: u8) -> Result<(), TouchError> {
        let pin = gpio_num_t::from(pin);
        // SAFETY: descriptor assigns this GPIO to the touch reset line.
        check(unsafe { gpio_reset_pin(pin) })?;
        // SAFETY: the reset pin remains exclusively owned by this driver.
        check(unsafe { gpio_set_direction(pin, gpio_mode_t_GPIO_MODE_OUTPUT) })?;
        // SAFETY: output mode is configured and levels follow the reference sequence.
        check(unsafe { gpio_set_level(pin, 0) })?;
        std::thread::sleep(Duration::from_millis(20));
        // SAFETY: same owned output pin.
        check(unsafe { gpio_set_level(pin, 1) })?;
        std::thread::sleep(Duration::from_millis(50));
        Ok(())
    }

    fn release_select_pin(pin: Option<gpio_num_t>) -> Result<(), TouchError> {
        if let Some(pin) = pin {
            // SAFETY: return the temporarily owned address-select pin to input mode.
            check(unsafe { gpio_set_direction(pin, gpio_mode_t_GPIO_MODE_INPUT) })?;
        }
        Ok(())
    }

    fn check(result: i32) -> Result<(), TouchError> {
        if result == 0 {
            Ok(())
        } else {
            Err(TouchError::Esp(result))
        }
    }
}

#[cfg(target_os = "espidf")]
pub use target::{EspCst816, EspTouch};

#[cfg(test)]
mod tests {
    use super::*;
    use tonex_boards::{PeripheralPin, TouchElectrical};

    const fn transform(swap_xy: bool, mirror_x: bool, mirror_y: bool) -> TouchElectrical {
        TouchElectrical {
            i2c_port: 0,
            addresses: [0x15, 0],
            address_count: 1,
            coordinate_width: 240,
            coordinate_height: 280,
            frequency_hz: 400_000,
            reset: Some(PeripheralPin::Gpio(13)),
            interrupt: None,
            swap_xy,
            mirror_x,
            mirror_y,
        }
    }

    #[test]
    fn axs15231b_decodes_point_count_and_12_bit_coordinates() {
        let data = [0x7f, 1, 0x21, 0x34, 0x03, 0xab, 0, 0];
        assert_eq!(
            decode_axs15231b_point(data),
            Some(TouchPoint { x: 0x134, y: 0x3ab })
        );
    }

    #[test]
    fn axs15231b_ignores_gesture_and_rejects_missing_or_extra_points() {
        assert_eq!(decode_axs15231b_point([0xff, 0, 0, 0, 0, 0, 0, 0]), None);
        assert_eq!(decode_axs15231b_point([0, 2, 0, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn maps_portrait_mirror_and_landscape_rotation() {
        let point = TouchPoint { x: 10, y: 20 };
        assert_eq!(
            map_touch(point, 240, 280, 240, 280, transform(false, true, false)),
            Some(TouchPoint { x: 229, y: 20 })
        );
        assert_eq!(
            map_touch(point, 240, 280, 280, 240, transform(false, true, false)),
            Some(TouchPoint { x: 20, y: 229 })
        );
    }

    #[test]
    fn rejects_coordinates_outside_the_physical_panel() {
        assert_eq!(
            map_touch(
                TouchPoint { x: 240, y: 0 },
                240,
                280,
                240,
                280,
                transform(false, false, false)
            ),
            None
        );
    }
}
