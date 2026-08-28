#![no_std]

pub use tonex_ui_model::UiClass;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValidationTier {
    Tier1,
    Tier2,
    Tier3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PsramMode {
    Quad,
    Octal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayBus {
    Spi,
    Qspi,
    I80,
    Rgb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayController {
    Gc9107,
    St7789,
    Sh8601,
    Axs15231b,
    St7796,
    RgbPanel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayElectrical {
    Spi {
        host: SpiHost,
        clock_gpio: u8,
        data_gpio: u8,
        dc_gpio: u8,
        cs_gpio: u8,
        reset_gpio: Option<u8>,
        backlight_gpio: Option<u8>,
        clock_hz: u32,
    },
    Qspi {
        host: SpiHost,
        clock_gpio: u8,
        data_gpios: [u8; 4],
        cs_gpio: u8,
        reset_gpio: Option<u8>,
        backlight_gpio: Option<u8>,
        clock_hz: u32,
    },
    I80 {
        data_gpios: [u8; 8],
        dc_gpio: u8,
        write_gpio: u8,
        read_gpio: Option<u8>,
        cs_gpio: u8,
        reset_gpio: Option<u8>,
        backlight_gpio: Option<u8>,
        power_gpio: Option<u8>,
        clock_hz: u32,
    },
    Rgb {
        data_gpios: [u8; 16],
        pclk_gpio: u8,
        vsync_gpio: u8,
        hsync_gpio: u8,
        de_gpio: u8,
        backlight_gpio: Option<u8>,
        clock_hz: u32,
        hsync_pulse_width: u16,
        hsync_back_porch: u16,
        hsync_front_porch: u16,
        vsync_pulse_width: u16,
        vsync_back_porch: u16,
        vsync_front_porch: u16,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpiHost {
    Spi2,
    Spi3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiDisplayPins {
    pub clock: u8,
    pub data: u8,
    pub dc: u8,
    pub cs: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TouchController {
    None,
    Cst816,
    Cst816OrCst328,
    Axs15231b,
    Gt911,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capabilities(u16);

impl Capabilities {
    pub const DISPLAY: Self = Self(1 << 0);
    pub const TOUCH: Self = Self(1 << 1);
    pub const INTERNAL_SWITCHES: Self = Self(1 << 2);
    pub const EXTERNAL_SWITCHES: Self = Self(1 << 3);
    pub const RGB_LEDS: Self = Self(1 << 4);
    pub const PSRAM: Self = Self(1 << 5);
    pub const WIFI: Self = Self(1 << 6);
    pub const BLE_MIDI: Self = Self(1 << 7);
    pub const SERIAL_MIDI: Self = Self(1 << 8);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayHardware {
    pub native_width: u16,
    pub native_height: u16,
    pub bus: DisplayBus,
    pub controller: DisplayController,
    pub touch: TouchController,
    pub touch_electrical: Option<TouchElectrical>,
    pub electrical: DisplayElectrical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeripheralPin {
    Gpio(u8),
    IoExpander(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TouchElectrical {
    pub i2c_port: u8,
    pub addresses: [u16; 2],
    pub address_count: u8,
    pub coordinate_width: u16,
    pub coordinate_height: u16,
    pub frequency_hz: u32,
    pub reset: Option<PeripheralPin>,
    pub interrupt: Option<PeripheralPin>,
    pub swap_xy: bool,
    pub mirror_x: bool,
    pub mirror_y: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TouchTransform {
    swap_xy: bool,
    mirror_x: bool,
    mirror_y: bool,
}

impl DisplayHardware {
    const fn with_touch(mut self, electrical: TouchElectrical) -> Self {
        self.touch_electrical = Some(electrical);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwitchInput {
    Unavailable,
    Gpio(u8),
    IoExpander(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoExpanderController {
    Ch422g,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IoExpanderHardware {
    pub controller: IoExpanderController,
    pub i2c_port: u8,
    pub scl_gpio: u8,
    pub sda_gpio: u8,
    pub frequency_hz: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlHardware {
    pub footswitches: [SwitchInput; 4],
    pub rgb_led_gpio: Option<u8>,
    pub io_expander: Option<IoExpanderHardware>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I2cBusHardware {
    pub port: u8,
    pub scl_gpio: u8,
    pub sda_gpio: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SerialMidiHardware {
    pub uart_port: u8,
    pub rx_gpio: u8,
    pub tx_gpio: Option<u8>,
    pub baud_rate: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HardwareProfile {
    /// Stable identity for a wiring and component combination.
    pub id: &'static str,
    /// Existing C file that is the pin/initialization evidence.
    pub legacy_platform_module: &'static str,
    pub i2c_bus_count: u8,
    pub i2c_buses: [Option<I2cBusHardware>; 2],
    pub display: Option<DisplayHardware>,
    pub backlight: Option<BacklightHardware>,
    pub controls: ControlHardware,
    pub serial_midi: Option<SerialMidiHardware>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BacklightHardware {
    Lp5562 {
        i2c_port: u8,
        address: u16,
        frequency_hz: u32,
        white: u8,
    },
}

impl HardwareProfile {
    const fn with_backlight(mut self, backlight: BacklightHardware) -> Self {
        self.backlight = Some(backlight);
        self
    }

    const fn with_serial_midi(mut self, serial_midi: SerialMidiHardware) -> Self {
        self.serial_midi = Some(serial_midi);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplaySpec {
    pub width: u16,
    pub height: u16,
    pub ui_class: UiClass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LedColourOrder {
    Rgb,
    Grb,
    Gbr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LedSpec {
    pub gpio: u8,
    pub count: u8,
    pub colour_order: LedColourOrder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDescriptor {
    /// Stable release/feature ID for this build variant.
    pub id: &'static str,
    /// Key in `legacy/source/esp_idf_project_configuration.json`.
    pub legacy_build: &'static str,
    /// Shared wiring/driver composition.
    pub hardware: &'static HardwareProfile,
    /// Flash size selected by the legacy sdkconfig, not an unverified claim
    /// about the maximum physical chip capacity.
    pub configured_flash_mb: u8,
    pub psram_mode: PsramMode,
    pub capabilities: Capabilities,
    pub display: Option<DisplaySpec>,
    pub leds: Option<LedSpec>,
    pub tier: ValidationTier,
}

impl BoardDescriptor {
    const fn with_leds(mut self, leds: LedSpec) -> Self {
        self.leds = Some(leds);
        self
    }
}

pub const WAVESHARE_169: HardwareProfile = profile(
    "waveshare-169",
    "platform_ws169.c",
    one_i2c(0, 10, 11),
    Some(display(
        240,
        280,
        DisplayBus::Spi,
        DisplayController::St7789,
        TouchController::None,
        spi_display(
            SpiHost::Spi3,
            SpiDisplayPins {
                clock: 6,
                data: 7,
                dc: 4,
                cs: 5,
            },
            Some(8),
            Some(15),
            40_000_000,
        ),
    )),
    controls([16, 3, 2, 44], Some(9)),
)
.with_serial_midi(serial_midi(18, Some(17)));
pub const WAVESHARE_169_TOUCH: HardwareProfile = profile(
    "waveshare-169-touch",
    "platform_ws169.c",
    one_i2c(0, 10, 11),
    Some(
        display(
            240,
            280,
            DisplayBus::Spi,
            DisplayController::St7789,
            TouchController::Cst816,
            spi_display(
                SpiHost::Spi3,
                SpiDisplayPins {
                    clock: 6,
                    data: 7,
                    dc: 4,
                    cs: 5,
                },
                Some(8),
                Some(15),
                40_000_000,
            ),
        )
        .with_touch(touch(
            0,
            [0x15, 0],
            1,
            (240, 280),
            Some(PeripheralPin::Gpio(13)),
            Some(PeripheralPin::Gpio(14)),
            transform(false, true, false),
        )),
    ),
    controls_optional(
        [
            SwitchInput::Gpio(3),
            SwitchInput::Gpio(2),
            SwitchInput::Gpio(44),
            SwitchInput::Unavailable,
        ],
        None,
    ),
)
.with_serial_midi(serial_midi(18, Some(17)));
pub const WAVESHARE_43B: HardwareProfile = profile(
    "waveshare-43b",
    "platform_ws43.c",
    one_i2c(0, 9, 8),
    Some(
        display(
            800,
            480,
            DisplayBus::Rgb,
            DisplayController::RgbPanel,
            TouchController::Gt911,
            rgb_display(14_500_000, 45),
        )
        .with_touch(touch(
            0,
            [0x5d, 0x14],
            2,
            (480, 800),
            Some(PeripheralPin::IoExpander(2)),
            Some(PeripheralPin::Gpio(4)),
            transform(true, false, false),
        )),
    ),
    controls_expander(
        [
            SwitchInput::IoExpander(1),
            SwitchInput::IoExpander(6),
            SwitchInput::Unavailable,
            SwitchInput::Unavailable,
        ],
        IoExpanderHardware {
            controller: IoExpanderController::Ch422g,
            i2c_port: 0,
            scl_gpio: 9,
            sda_gpio: 8,
            frequency_hz: 400_000,
        },
    ),
)
.with_serial_midi(serial_midi(43, Some(44)));
pub const WAVESHARE_35B: HardwareProfile = profile(
    "waveshare-35b",
    "platform_ws35b.c",
    one_i2c(0, 7, 8),
    Some(
        display(
            480,
            320,
            DisplayBus::Qspi,
            DisplayController::Axs15231b,
            TouchController::Axs15231b,
            qspi_display(5, [1, 2, 3, 4], 12, 6),
        )
        .with_touch(touch(
            0,
            [0x3b, 0],
            1,
            (480, 320),
            None,
            Some(PeripheralPin::IoExpander(2)),
            transform(false, false, false),
        )),
    ),
    controls([38, 39, 40, 41], None),
)
.with_serial_midi(serial_midi(17, Some(18)));
pub const JC3248W535: HardwareProfile = profile(
    "jc3248w535",
    "platform_jc3248w.c",
    two_i2c(0, 8, 4, 1, 17, 18),
    Some(
        display(
            480,
            320,
            DisplayBus::Qspi,
            DisplayController::Axs15231b,
            TouchController::Axs15231b,
            qspi_display(47, [21, 48, 40, 39], 45, 1),
        )
        .with_touch(touch(
            0,
            [0x3b, 0],
            1,
            (320, 480),
            None,
            None,
            transform(true, true, false),
        )),
    ),
    controls([5, 6, 7, 15], None),
)
.with_serial_midi(serial_midi(16, Some(46)));
pub const WAVESHARE_ZERO: HardwareProfile = profile(
    "waveshare-zero",
    "platform_wszero.c",
    one_i2c(0, 10, 11),
    None,
    controls([4, 6, 2, 1], Some(21)),
)
.with_serial_midi(serial_midi(5, Some(7)));
pub const DEVKITC: HardwareProfile = profile(
    "devkitc",
    "platform_devkitc.c",
    one_i2c(0, 10, 11),
    None,
    controls([4, 6, 2, 1], Some(48)),
)
.with_serial_midi(serial_midi(5, Some(7)));
pub const M5_ATOMS3R: HardwareProfile = profile(
    "m5-atoms3r",
    "platform_m5atoms3r.c",
    two_i2c(0, 0, 45, 1, 1, 2),
    Some(display(
        128,
        128,
        DisplayBus::Spi,
        DisplayController::Gc9107,
        TouchController::None,
        spi_display(
            SpiHost::Spi3,
            SpiDisplayPins {
                clock: 15,
                data: 21,
                dc: 42,
                cs: 14,
            },
            Some(48),
            None,
            40_000_000,
        ),
    )),
    controls([5, 6, 7, 8], None),
)
.with_backlight(BacklightHardware::Lp5562 {
    i2c_port: 0,
    address: 0x30,
    frequency_hz: 400_000,
    white: 180,
})
.with_serial_midi(serial_midi(38, Some(39)));
pub const LILYGO_TDISPLAY_S3: HardwareProfile = profile(
    "lilygo-tdisplay-s3",
    "platform_lgtdisps3.c",
    one_i2c(0, 17, 18),
    Some(
        display(
            320,
            170,
            DisplayBus::I80,
            DisplayController::St7789,
            TouchController::Cst816OrCst328,
            DisplayElectrical::I80 {
                data_gpios: [39, 40, 41, 42, 45, 46, 47, 48],
                dc_gpio: 7,
                write_gpio: 8,
                read_gpio: Some(9),
                cs_gpio: 6,
                reset_gpio: Some(5),
                backlight_gpio: Some(38),
                power_gpio: Some(15),
                clock_hz: 10_000_000,
            },
        )
        .with_touch(touch(
            0,
            [0x15, 0x1a],
            2,
            (320, 170),
            Some(PeripheralPin::Gpio(21)),
            Some(PeripheralPin::Gpio(16)),
            transform(true, false, false),
        )),
    ),
    controls([1, 2, 3, 10], None),
)
.with_serial_midi(serial_midi(11, Some(12)));
pub const WAVESHARE_19_TOUCH: HardwareProfile = profile(
    "waveshare-19-touch",
    "platform_ws19t.c",
    one_i2c(0, 48, 47),
    Some(
        display(
            320,
            170,
            DisplayBus::Spi,
            DisplayController::Sh8601,
            TouchController::Cst816,
            spi_display(
                SpiHost::Spi3,
                SpiDisplayPins {
                    clock: 10,
                    data: 13,
                    dc: 11,
                    cs: 12,
                },
                Some(9),
                Some(14),
                20_000_000,
            ),
        )
        .with_touch(touch(
            0,
            [0x15, 0],
            1,
            (320, 170),
            Some(PeripheralPin::Gpio(17)),
            Some(PeripheralPin::Gpio(21)),
            transform(false, false, false),
        )),
    ),
    controls([1, 2, 3, 4], None),
)
.with_serial_midi(serial_midi(5, Some(6)));
pub const WAVESHARE_7_43: HardwareProfile = profile(
    "waveshare-7-43",
    "platform_ws7.c",
    one_i2c(0, 9, 8),
    Some(
        display(
            800,
            480,
            DisplayBus::Rgb,
            DisplayController::RgbPanel,
            TouchController::Gt911,
            rgb_display(14_000_000, 36),
        )
        .with_touch(touch(
            0,
            [0x5d, 0x14],
            2,
            (480, 800),
            Some(PeripheralPin::IoExpander(1)),
            Some(PeripheralPin::Gpio(4)),
            transform(true, false, false),
        )),
    ),
    controls_optional([SwitchInput::Unavailable; 4], None),
)
.with_serial_midi(serial_midi(6, None));
pub const PIRATE_POLAR_PRO: HardwareProfile = profile(
    "pirate-polar-pro",
    "platform_pirate_pro_169.c",
    one_i2c(0, 12, 13),
    Some(display(
        240,
        280,
        DisplayBus::Spi,
        DisplayController::St7789,
        TouchController::None,
        spi_display(
            SpiHost::Spi3,
            SpiDisplayPins {
                clock: 6,
                data: 7,
                dc: 4,
                cs: 5,
            },
            Some(8),
            Some(15),
            40_000_000,
        ),
    )),
    controls([16, 3, 2, 44], Some(1)),
)
.with_serial_midi(serial_midi(18, Some(17)));
pub const PIRATE_POLAR_MAX_V2: HardwareProfile = profile(
    "pirate-polar-max-v2",
    "platform_pirate_max35.c",
    one_i2c(0, 41, 40),
    Some(
        display(
            480,
            320,
            DisplayBus::Spi,
            DisplayController::St7796,
            TouchController::Gt911,
            spi_display(
                SpiHost::Spi2,
                SpiDisplayPins {
                    clock: 47,
                    data: 21,
                    dc: 14,
                    cs: 48,
                },
                Some(13),
                Some(39),
                48_000_000,
            ),
        )
        .with_touch(touch(
            0,
            [0x5d, 0x14],
            2,
            (320, 480),
            None,
            Some(PeripheralPin::Gpio(42)),
            transform(true, false, true),
        )),
    ),
    controls([2, 4, 5, 6], Some(1)),
)
.with_serial_midi(serial_midi(10, Some(9)));

const BASE: Capabilities = Capabilities::INTERNAL_SWITCHES
    .union(Capabilities::EXTERNAL_SWITCHES)
    .union(Capabilities::PSRAM)
    .union(Capabilities::WIFI)
    .union(Capabilities::BLE_MIDI)
    .union(Capabilities::SERIAL_MIDI);
const WITH_DISPLAY: Capabilities = BASE.union(Capabilities::DISPLAY);
const WITH_TOUCH: Capabilities = WITH_DISPLAY.union(Capabilities::TOUCH);

// Phase 0/4 registry. All variants remain Tier 3 until both an ESP-IDF target
// build and physical hardware test provide stronger evidence.
pub const REGISTRY: &[BoardDescriptor] = &[
    variant(
        "waveshare-169",
        "WS169",
        &WAVESHARE_169,
        8,
        PsramMode::Octal,
        WITH_DISPLAY,
        portrait(),
    ),
    variant(
        "waveshare-169-landscape",
        "WS169Land",
        &WAVESHARE_169,
        8,
        PsramMode::Octal,
        WITH_DISPLAY,
        landscape(),
    ),
    variant(
        "waveshare-169-touch",
        "WS169TOUCH",
        &WAVESHARE_169_TOUCH,
        8,
        PsramMode::Octal,
        WITH_TOUCH,
        portrait(),
    ),
    variant(
        "waveshare-169-touch-landscape",
        "WS169TOUCHLand",
        &WAVESHARE_169_TOUCH,
        8,
        PsramMode::Octal,
        WITH_TOUCH,
        landscape(),
    ),
    variant(
        "waveshare-43b",
        "WS43B",
        &WAVESHARE_43B,
        16,
        PsramMode::Octal,
        WITH_TOUCH,
        large(),
    ),
    variant(
        "waveshare-35b",
        "WS35B",
        &WAVESHARE_35B,
        16,
        PsramMode::Octal,
        WITH_TOUCH,
        medium(),
    ),
    variant(
        "jc3248w535",
        "JC3248W",
        &JC3248W535,
        16,
        PsramMode::Octal,
        WITH_TOUCH,
        medium(),
    ),
    headless_led(
        "waveshare-zero",
        "WSZERO",
        &WAVESHARE_ZERO,
        4,
        PsramMode::Quad,
        BASE.union(Capabilities::RGB_LEDS),
        LedSpec {
            gpio: 21,
            count: 1,
            colour_order: LedColourOrder::Rgb,
        },
    ),
    headless_led(
        "devkitc-n8r2",
        "DEVKITC_N8R2",
        &DEVKITC,
        8,
        PsramMode::Quad,
        BASE.union(Capabilities::RGB_LEDS),
        LedSpec {
            gpio: 48,
            count: 1,
            colour_order: LedColourOrder::Gbr,
        },
    ),
    headless_led(
        "devkitc-n16r8",
        "DEVKITC_N16R8",
        &DEVKITC,
        8,
        PsramMode::Octal,
        BASE.union(Capabilities::RGB_LEDS),
        LedSpec {
            gpio: 48,
            count: 1,
            colour_order: LedColourOrder::Gbr,
        },
    ),
    variant(
        "m5-atoms3r",
        "M5AtomS3R",
        &M5_ATOMS3R,
        8,
        PsramMode::Octal,
        WITH_DISPLAY,
        tiny(),
    ),
    variant(
        "lilygo-tdisplay-s3",
        "LGTDisplayS3",
        &LILYGO_TDISPLAY_S3,
        16,
        PsramMode::Octal,
        WITH_TOUCH,
        compact(),
    ),
    variant(
        "waveshare-19-touch",
        "WS19Touch",
        &WAVESHARE_19_TOUCH,
        8,
        PsramMode::Octal,
        WITH_TOUCH,
        compact(),
    ),
    variant(
        "waveshare-7-43",
        "WS7_43",
        &WAVESHARE_7_43,
        8,
        PsramMode::Octal,
        WITH_TOUCH,
        large(),
    ),
    variant(
        "pirate-polar-mini",
        "PirateMini",
        &WAVESHARE_169,
        8,
        PsramMode::Octal,
        WITH_DISPLAY,
        portrait(),
    ),
    variant(
        "pirate-polar-plus",
        "PiratePlus",
        &WAVESHARE_169,
        8,
        PsramMode::Octal,
        WITH_DISPLAY,
        landscape(),
    ),
    headless_led(
        "pirate-polar-zero",
        "PirateZERO",
        &WAVESHARE_ZERO,
        4,
        PsramMode::Quad,
        BASE.union(Capabilities::RGB_LEDS),
        LedSpec {
            gpio: 21,
            count: 1,
            colour_order: LedColourOrder::Rgb,
        },
    ),
    variant(
        "pirate-polar-43b",
        "Pirate43B",
        &WAVESHARE_43B,
        16,
        PsramMode::Octal,
        WITH_TOUCH,
        large(),
    ),
    variant(
        "pirate-polar-mini-v2",
        "PirateMiniV2",
        &WAVESHARE_169,
        8,
        PsramMode::Quad,
        WITH_DISPLAY,
        portrait(),
    ),
    variant(
        "pirate-polar-plus-v2",
        "PiratePlusV2",
        &WAVESHARE_169,
        16,
        PsramMode::Quad,
        WITH_DISPLAY.union(Capabilities::RGB_LEDS),
        landscape(),
    )
    .with_leds(LedSpec {
        gpio: 9,
        count: 2,
        colour_order: LedColourOrder::Grb,
    }),
    variant(
        "pirate-polar-pro",
        "PiratePro",
        &PIRATE_POLAR_PRO,
        16,
        PsramMode::Quad,
        WITH_DISPLAY.union(Capabilities::RGB_LEDS),
        portrait(),
    )
    .with_leds(LedSpec {
        gpio: 1,
        count: 4,
        colour_order: LedColourOrder::Grb,
    }),
    variant(
        "pirate-polar-max-v2",
        "PirateMaxV2",
        &PIRATE_POLAR_MAX_V2,
        16,
        PsramMode::Octal,
        WITH_TOUCH.union(Capabilities::RGB_LEDS),
        medium(),
    )
    .with_leds(LedSpec {
        gpio: 1,
        count: 4,
        colour_order: LedColourOrder::Gbr,
    }),
];

const fn profile(
    id: &'static str,
    legacy_platform_module: &'static str,
    i2c_buses: [Option<I2cBusHardware>; 2],
    display: Option<DisplayHardware>,
    controls: ControlHardware,
) -> HardwareProfile {
    HardwareProfile {
        id,
        legacy_platform_module,
        i2c_bus_count: if i2c_buses[1].is_some() { 2 } else { 1 },
        i2c_buses,
        display,
        backlight: None,
        controls,
        serial_midi: None,
    }
}

const fn serial_midi(rx_gpio: u8, tx_gpio: Option<u8>) -> SerialMidiHardware {
    SerialMidiHardware {
        uart_port: 1,
        rx_gpio,
        tx_gpio,
        baud_rate: 31_250,
    }
}

const fn one_i2c(port: u8, scl_gpio: u8, sda_gpio: u8) -> [Option<I2cBusHardware>; 2] {
    [
        Some(I2cBusHardware {
            port,
            scl_gpio,
            sda_gpio,
        }),
        None,
    ]
}

const fn two_i2c(
    port_1: u8,
    scl_1: u8,
    sda_1: u8,
    port_2: u8,
    scl_2: u8,
    sda_2: u8,
) -> [Option<I2cBusHardware>; 2] {
    [
        Some(I2cBusHardware {
            port: port_1,
            scl_gpio: scl_1,
            sda_gpio: sda_1,
        }),
        Some(I2cBusHardware {
            port: port_2,
            scl_gpio: scl_2,
            sda_gpio: sda_2,
        }),
    ]
}

const fn controls(pins: [u8; 4], rgb_led_gpio: Option<u8>) -> ControlHardware {
    ControlHardware {
        footswitches: [
            SwitchInput::Gpio(pins[0]),
            SwitchInput::Gpio(pins[1]),
            SwitchInput::Gpio(pins[2]),
            SwitchInput::Gpio(pins[3]),
        ],
        rgb_led_gpio,
        io_expander: None,
    }
}

const fn controls_optional(
    footswitches: [SwitchInput; 4],
    rgb_led_gpio: Option<u8>,
) -> ControlHardware {
    ControlHardware {
        footswitches,
        rgb_led_gpio,
        io_expander: None,
    }
}

const fn controls_expander(
    footswitches: [SwitchInput; 4],
    io_expander: IoExpanderHardware,
) -> ControlHardware {
    ControlHardware {
        footswitches,
        rgb_led_gpio: None,
        io_expander: Some(io_expander),
    }
}

const fn display(
    native_width: u16,
    native_height: u16,
    bus: DisplayBus,
    controller: DisplayController,
    touch: TouchController,
    electrical: DisplayElectrical,
) -> DisplayHardware {
    DisplayHardware {
        native_width,
        native_height,
        bus,
        controller,
        touch,
        touch_electrical: None,
        electrical,
    }
}

const fn touch(
    i2c_port: u8,
    addresses: [u16; 2],
    address_count: u8,
    coordinates: (u16, u16),
    reset: Option<PeripheralPin>,
    interrupt: Option<PeripheralPin>,
    transform: TouchTransform,
) -> TouchElectrical {
    TouchElectrical {
        i2c_port,
        addresses,
        address_count,
        coordinate_width: coordinates.0,
        coordinate_height: coordinates.1,
        frequency_hz: 400_000,
        reset,
        interrupt,
        swap_xy: transform.swap_xy,
        mirror_x: transform.mirror_x,
        mirror_y: transform.mirror_y,
    }
}

const fn transform(swap_xy: bool, mirror_x: bool, mirror_y: bool) -> TouchTransform {
    TouchTransform {
        swap_xy,
        mirror_x,
        mirror_y,
    }
}

const fn spi_display(
    host: SpiHost,
    pins: SpiDisplayPins,
    reset_gpio: Option<u8>,
    backlight_gpio: Option<u8>,
    clock_hz: u32,
) -> DisplayElectrical {
    DisplayElectrical::Spi {
        host,
        clock_gpio: pins.clock,
        data_gpio: pins.data,
        dc_gpio: pins.dc,
        cs_gpio: pins.cs,
        reset_gpio,
        backlight_gpio,
        clock_hz,
    }
}

const fn qspi_display(
    clock_gpio: u8,
    data_gpios: [u8; 4],
    cs_gpio: u8,
    backlight_gpio: u8,
) -> DisplayElectrical {
    DisplayElectrical::Qspi {
        host: SpiHost::Spi2,
        clock_gpio,
        data_gpios,
        cs_gpio,
        reset_gpio: None,
        backlight_gpio: Some(backlight_gpio),
        clock_hz: 40_000_000,
    }
}

const fn rgb_display(clock_hz: u32, vertical_porch: u16) -> DisplayElectrical {
    DisplayElectrical::Rgb {
        data_gpios: [14, 38, 18, 17, 10, 39, 0, 45, 48, 47, 21, 1, 2, 42, 41, 40],
        pclk_gpio: 7,
        vsync_gpio: 3,
        hsync_gpio: 46,
        de_gpio: 5,
        backlight_gpio: None,
        clock_hz,
        hsync_pulse_width: 4,
        hsync_back_porch: 8,
        hsync_front_porch: 8,
        vsync_pulse_width: 4,
        vsync_back_porch: vertical_porch,
        vsync_front_porch: vertical_porch,
    }
}

const fn variant(
    id: &'static str,
    legacy_build: &'static str,
    hardware: &'static HardwareProfile,
    configured_flash_mb: u8,
    psram_mode: PsramMode,
    capabilities: Capabilities,
    display: DisplaySpec,
) -> BoardDescriptor {
    BoardDescriptor {
        id,
        legacy_build,
        hardware,
        configured_flash_mb,
        psram_mode,
        capabilities,
        display: Some(display),
        leds: None,
        tier: ValidationTier::Tier3,
    }
}

const fn headless(
    id: &'static str,
    legacy_build: &'static str,
    hardware: &'static HardwareProfile,
    configured_flash_mb: u8,
    psram_mode: PsramMode,
    capabilities: Capabilities,
) -> BoardDescriptor {
    BoardDescriptor {
        id,
        legacy_build,
        hardware,
        configured_flash_mb,
        psram_mode,
        capabilities,
        display: None,
        leds: None,
        tier: ValidationTier::Tier3,
    }
}

const fn headless_led(
    id: &'static str,
    legacy_build: &'static str,
    hardware: &'static HardwareProfile,
    configured_flash_mb: u8,
    psram_mode: PsramMode,
    capabilities: Capabilities,
    leds: LedSpec,
) -> BoardDescriptor {
    let mut descriptor = headless(
        id,
        legacy_build,
        hardware,
        configured_flash_mb,
        psram_mode,
        capabilities,
    );
    descriptor.leds = Some(leds);
    descriptor
}

const fn portrait() -> DisplaySpec {
    DisplaySpec {
        width: 240,
        height: 280,
        ui_class: UiClass::Portrait240x280,
    }
}

const fn landscape() -> DisplaySpec {
    DisplaySpec {
        width: 280,
        height: 240,
        ui_class: UiClass::Landscape280x240,
    }
}

const fn tiny() -> DisplaySpec {
    DisplaySpec {
        width: 128,
        height: 128,
        ui_class: UiClass::Tiny128,
    }
}

const fn compact() -> DisplaySpec {
    DisplaySpec {
        width: 320,
        height: 170,
        ui_class: UiClass::Compact320x170,
    }
}

const fn medium() -> DisplaySpec {
    DisplaySpec {
        width: 480,
        height: 320,
        ui_class: UiClass::Medium480x320,
    }
}

const fn large() -> DisplaySpec {
    DisplaySpec {
        width: 800,
        height: 480,
        ui_class: UiClass::Large800x480,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_ids_and_legacy_builds_are_unique() {
        for (index, board) in REGISTRY.iter().enumerate() {
            for other in &REGISTRY[index + 1..] {
                assert_ne!(board.id, other.id);
                assert_ne!(board.legacy_build, other.legacy_build);
            }
        }
    }

    #[test]
    fn every_legacy_matrix_entry_is_registered() {
        const EXPECTED: &[&str] = &[
            "WS169",
            "WS169Land",
            "WS169TOUCH",
            "WS169TOUCHLand",
            "WS43B",
            "WS35B",
            "JC3248W",
            "WSZERO",
            "DEVKITC_N8R2",
            "DEVKITC_N16R8",
            "M5AtomS3R",
            "LGTDisplayS3",
            "WS19Touch",
            "WS7_43",
            "PirateMini",
            "PiratePlus",
            "PirateZERO",
            "Pirate43B",
            "PirateMiniV2",
            "PiratePlusV2",
            "PiratePro",
            "PirateMaxV2",
        ];
        assert_eq!(REGISTRY.len(), EXPECTED.len());
        for expected in EXPECTED {
            assert!(REGISTRY.iter().any(|board| board.legacy_build == *expected));
        }
    }

    #[test]
    fn display_and_touch_capabilities_match_hardware() {
        for board in REGISTRY {
            assert_eq!(
                board.capabilities.contains(Capabilities::DISPLAY),
                board.display.is_some(),
                "{}",
                board.id
            );
            let has_touch = board
                .hardware
                .display
                .is_some_and(|display| display.touch != TouchController::None);
            assert_eq!(
                board.capabilities.contains(Capabilities::TOUCH),
                has_touch,
                "{}",
                board.id
            );
            if let Some(display) = board.hardware.display {
                assert_eq!(
                    display.touch_electrical.is_some(),
                    display.touch != TouchController::None,
                    "{}",
                    board.id
                );
                if let Some(touch) = display.touch_electrical {
                    assert!((1..=2).contains(&touch.address_count), "{}", board.id);
                    assert!(touch.frequency_hz > 0, "{}", board.id);
                    assert!(touch.coordinate_width > 0, "{}", board.id);
                    assert!(touch.coordinate_height > 0, "{}", board.id);
                    assert!(
                        board.hardware.i2c_buses[usize::from(touch.i2c_port)].is_some(),
                        "{}",
                        board.id
                    );
                }
            }
            assert_eq!(
                board.capabilities.contains(Capabilities::RGB_LEDS),
                board.leds.is_some(),
                "{}",
                board.id
            );
        }
    }

    #[test]
    fn display_bus_metadata_is_complete_and_consistent() {
        for board in REGISTRY {
            let Some(display) = board.hardware.display else {
                continue;
            };
            let (bus, clock_hz) = match display.electrical {
                DisplayElectrical::Spi { clock_hz, .. } => (DisplayBus::Spi, clock_hz),
                DisplayElectrical::Qspi { clock_hz, .. } => (DisplayBus::Qspi, clock_hz),
                DisplayElectrical::I80 { clock_hz, .. } => (DisplayBus::I80, clock_hz),
                DisplayElectrical::Rgb { clock_hz, .. } => (DisplayBus::Rgb, clock_hz),
            };
            assert_eq!(display.bus, bus, "{}", board.id);
            assert!(clock_hz > 0, "{}", board.id);
        }
    }

    #[test]
    fn i2c_bus_metadata_is_complete_and_consistent() {
        for board in REGISTRY {
            let buses = board.hardware.i2c_buses;
            assert!(buses[0].is_some(), "{}", board.id);
            let count = buses.iter().flatten().count();
            assert_eq!(
                usize::from(board.hardware.i2c_bus_count),
                count,
                "{}",
                board.id
            );
            for (index, bus) in buses.iter().flatten().enumerate() {
                assert_eq!(usize::from(bus.port), index, "{}", board.id);
                assert_ne!(bus.scl_gpio, bus.sda_gpio, "{}", board.id);
            }
        }
    }

    #[test]
    fn every_hardware_profile_has_source_derived_switch_wiring() {
        let profiles = [
            &WAVESHARE_169,
            &WAVESHARE_169_TOUCH,
            &WAVESHARE_43B,
            &WAVESHARE_35B,
            &JC3248W535,
            &WAVESHARE_ZERO,
            &DEVKITC,
            &M5_ATOMS3R,
            &LILYGO_TDISPLAY_S3,
            &WAVESHARE_19_TOUCH,
            &WAVESHARE_7_43,
            &PIRATE_POLAR_PRO,
            &PIRATE_POLAR_MAX_V2,
        ];
        assert_eq!(profiles.len(), 13);
        assert_eq!(
            WAVESHARE_43B.controls.footswitches,
            [
                SwitchInput::IoExpander(1),
                SwitchInput::IoExpander(6),
                SwitchInput::Unavailable,
                SwitchInput::Unavailable,
            ]
        );
        assert_eq!(
            WAVESHARE_43B
                .controls
                .io_expander
                .expect("CH422G metadata exists")
                .scl_gpio,
            9
        );
        assert_eq!(
            WAVESHARE_7_43.controls.footswitches,
            [SwitchInput::Unavailable; 4]
        );
        assert_eq!(PIRATE_POLAR_MAX_V2.controls.rgb_led_gpio, Some(1));
        let max = REGISTRY
            .iter()
            .find(|board| board.id == "pirate-polar-max-v2")
            .expect("max is registered");
        assert_eq!(
            max.leds,
            Some(LedSpec {
                gpio: 1,
                count: 4,
                colour_order: LedColourOrder::Gbr,
            })
        );
    }

    #[test]
    fn serial_midi_capability_always_has_source_derived_uart_wiring() {
        for board in REGISTRY {
            if board.capabilities.contains(Capabilities::SERIAL_MIDI) {
                let serial = board
                    .hardware
                    .serial_midi
                    .expect("serial MIDI capability requires UART metadata");
                assert_eq!(serial.uart_port, 1);
                assert_eq!(serial.baud_rate, 31_250);
            }
        }
        assert_eq!(WAVESHARE_43B.serial_midi, Some(serial_midi(43, Some(44))));
        assert_eq!(WAVESHARE_7_43.serial_midi, Some(serial_midi(6, None)));
        assert_eq!(
            PIRATE_POLAR_MAX_V2.serial_midi,
            Some(serial_midi(10, Some(9)))
        );
    }

    #[test]
    fn ui_geometry_matches_native_panel_in_either_orientation() {
        for board in REGISTRY {
            let (Some(ui), Some(panel)) = (board.display, board.hardware.display) else {
                continue;
            };
            let same = ui.width == panel.native_width && ui.height == panel.native_height;
            let rotated = ui.width == panel.native_height && ui.height == panel.native_width;
            assert!(same || rotated, "{}", board.id);
        }
    }

    #[test]
    fn configured_memory_modes_match_legacy_matrix() {
        let zero = REGISTRY
            .iter()
            .find(|board| board.legacy_build == "WSZERO")
            .expect("WSZERO is registered");
        assert_eq!(zero.configured_flash_mb, 4);
        assert_eq!(zero.psram_mode, PsramMode::Quad);

        let large = REGISTRY
            .iter()
            .find(|board| board.legacy_build == "WS43B")
            .expect("WS43B is registered");
        assert_eq!(large.configured_flash_mb, 16);
        assert_eq!(large.psram_mode, PsramMode::Octal);
    }

    #[test]
    fn shared_hardware_profiles_prevent_product_driver_copies() {
        let waveshare = REGISTRY
            .iter()
            .find(|board| board.legacy_build == "WS43B")
            .expect("WS43B is registered");
        let pirate = REGISTRY
            .iter()
            .find(|board| board.legacy_build == "Pirate43B")
            .expect("Pirate43B is registered");
        assert_eq!(waveshare.hardware.id, pirate.hardware.id);
        assert_eq!(
            waveshare.hardware.legacy_platform_module,
            pirate.hardware.legacy_platform_module
        );
    }
}
