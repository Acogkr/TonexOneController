#![cfg_attr(target_os = "espidf", allow(unsafe_code))]

#[cfg(target_os = "espidf")]
mod esp {
    use core::{
        ffi::c_void,
        ptr::{self, NonNull},
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };
    use std::{boxed::Box, vec::Vec};

    use esp_idf_sys::{
        ESP_OK, MALLOC_CAP_DMA, MALLOC_CAP_INTERNAL, SPICOMMON_BUSFLAG_QUAD, esp_err_t,
        esp_lcd_del_i80_bus, esp_lcd_i80_bus_config_t, esp_lcd_i80_bus_handle_t,
        esp_lcd_new_i80_bus, esp_lcd_new_panel_io_i80, esp_lcd_new_panel_io_spi,
        esp_lcd_new_panel_st7789, esp_lcd_new_rgb_panel, esp_lcd_panel_del,
        esp_lcd_panel_dev_config_t, esp_lcd_panel_dev_config_t__bindgen_ty_1,
        esp_lcd_panel_disp_on_off, esp_lcd_panel_draw_bitmap, esp_lcd_panel_handle_t,
        esp_lcd_panel_init, esp_lcd_panel_invert_color, esp_lcd_panel_io_del,
        esp_lcd_panel_io_event_data_t, esp_lcd_panel_io_handle_t, esp_lcd_panel_io_i80_config_t,
        esp_lcd_panel_io_i80_config_t__bindgen_ty_1, esp_lcd_panel_io_spi_config_t,
        esp_lcd_panel_io_spi_config_t__bindgen_ty_1, esp_lcd_panel_io_tx_param,
        esp_lcd_panel_mirror, esp_lcd_panel_reset, esp_lcd_panel_set_gap, esp_lcd_panel_swap_xy,
        esp_lcd_rgb_panel_config_t, esp_lcd_rgb_panel_config_t__bindgen_ty_2,
        esp_lcd_rgb_panel_get_frame_buffer, esp_lcd_rgb_timing_t,
        esp_lcd_rgb_timing_t__bindgen_ty_1, gpio_mode_t_GPIO_MODE_OUTPUT, gpio_set_direction,
        gpio_set_level, heap_caps_aligned_alloc, heap_caps_free,
        lcd_rgb_data_endian_t_LCD_RGB_DATA_ENDIAN_LITTLE,
        lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_BGR,
        lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB, spi_bus_config_t, spi_bus_free,
        spi_bus_initialize, spi_common_dma_t_SPI_DMA_CH_AUTO, spi_host_device_t,
        spi_host_device_t_SPI2_HOST, spi_host_device_t_SPI3_HOST,
    };
    use hal_contracts::DisplayDevice;
    use tonex_boards::{BoardDescriptor, DisplayController, DisplayElectrical, SpiHost};

    const ROWS_PER_TRANSFER: usize = 32;
    const AXS15231B_COLUMNS_PER_TRANSFER: usize = 48;
    const TRANSFER_TIMEOUT: Duration = Duration::from_millis(500);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum DisplayError {
        UnsupportedBoard,
        InvalidDimensions,
        OutOfMemory,
        Esp(i32),
        TransferTimeout,
    }

    pub struct EspSt7789 {
        host: spi_host_device_t,
        i80_bus: esp_lcd_i80_bus_handle_t,
        io: esp_lcd_panel_io_handle_t,
        panel: esp_lcd_panel_handle_t,
        width: u16,
        height: u16,
        frame: Vec<u16>,
        transfer: DmaBuffer,
        pending: Box<AtomicBool>,
        bus_initialized: bool,
        software_rotate_90: bool,
    }

    impl core::fmt::Debug for EspSt7789 {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter
                .debug_struct("EspSt7789")
                .field("width", &self.width)
                .field("height", &self.height)
                .finish_non_exhaustive()
        }
    }

    impl EspSt7789 {
        /// Creates either the SPI or I80 ST7789 path declared by the board.
        ///
        /// # Errors
        ///
        /// Returns descriptor, allocation, ESP-IDF, or transfer setup errors.
        pub fn new(board: &BoardDescriptor) -> Result<Self, DisplayError> {
            let hardware = board
                .hardware
                .display
                .ok_or(DisplayError::UnsupportedBoard)?;
            match hardware.electrical {
                DisplayElectrical::Spi { .. } => Self::new_spi(board),
                DisplayElectrical::I80 { .. } => Self::new_i80(board),
                _ => Err(DisplayError::UnsupportedBoard),
            }
        }

        fn new_spi(board: &BoardDescriptor) -> Result<Self, DisplayError> {
            let logical = board.display.ok_or(DisplayError::UnsupportedBoard)?;
            let hardware = board
                .hardware
                .display
                .ok_or(DisplayError::UnsupportedBoard)?;
            if !matches!(
                hardware.controller,
                DisplayController::St7789 | DisplayController::Sh8601 | DisplayController::St7796
            ) {
                return Err(DisplayError::UnsupportedBoard);
            }
            let DisplayElectrical::Spi {
                host,
                clock_gpio,
                data_gpio,
                dc_gpio,
                cs_gpio,
                reset_gpio,
                backlight_gpio,
                clock_hz,
            } = hardware.electrical
            else {
                return Err(DisplayError::UnsupportedBoard);
            };
            let transfer_pixels = usize::from(logical.width)
                .checked_mul(ROWS_PER_TRANSFER)
                .ok_or(DisplayError::InvalidDimensions)?;
            let (frame, transfer) = allocate_buffers(logical.width, logical.height)?;
            let pending = Box::new(AtomicBool::new(false));
            let host = host_id(host);
            let mut display = Self {
                host,
                i80_bus: ptr::null_mut(),
                io: ptr::null_mut(),
                panel: ptr::null_mut(),
                width: logical.width,
                height: logical.height,
                frame,
                transfer,
                pending,
                bus_initialized: false,
                software_rotate_90: false,
            };

            let mut bus = spi_bus_config_t {
                sclk_io_num: i32::from(clock_gpio),
                max_transfer_sz: i32::try_from(
                    transfer_pixels
                        .checked_mul(core::mem::size_of::<u16>())
                        .ok_or(DisplayError::InvalidDimensions)?,
                )
                .map_err(|_| DisplayError::InvalidDimensions)?,
                ..Default::default()
            };
            bus.__bindgen_anon_1.mosi_io_num = i32::from(data_gpio);
            bus.__bindgen_anon_2.miso_io_num = -1;
            bus.__bindgen_anon_3.quadwp_io_num = -1;
            bus.__bindgen_anon_4.quadhd_io_num = -1;
            // SAFETY: all GPIOs and transfer bounds come from the validated board descriptor.
            check(unsafe {
                spi_bus_initialize(host, &raw const bus, spi_common_dma_t_SPI_DMA_CH_AUTO)
            })?;
            display.bus_initialized = true;

            let io_config = esp_lcd_panel_io_spi_config_t {
                cs_gpio_num: i32::from(cs_gpio),
                dc_gpio_num: i32::from(dc_gpio),
                spi_mode: 0,
                pclk_hz: clock_hz,
                trans_queue_depth: 1,
                on_color_trans_done: Some(transfer_complete),
                user_ctx: (&raw const *display.pending).cast_mut().cast(),
                lcd_cmd_bits: 8,
                lcd_param_bits: 8,
                ..Default::default()
            };
            let lcd_host = i32::try_from(host).map_err(|_| DisplayError::UnsupportedBoard)?;
            // SAFETY: the bus is initialized and callback context is a stable Box allocation.
            check(unsafe {
                esp_lcd_new_panel_io_spi(lcd_host, &raw const io_config, &raw mut display.io)
            })?;

            let swap =
                logical.width == hardware.native_height && logical.height == hardware.native_width;
            match hardware.controller {
                DisplayController::St7789 => {
                    display.initialize_spi_panel(reset_gpio, backlight_gpio, swap)?;
                }
                DisplayController::Sh8601 => {
                    display.initialize_sh8601(reset_gpio, backlight_gpio)?;
                }
                DisplayController::St7796 => {
                    display.initialize_st7796(reset_gpio, backlight_gpio)?;
                }
                _ => return Err(DisplayError::UnsupportedBoard),
            }
            Ok(display)
        }

        fn new_axs15231b(board: &BoardDescriptor) -> Result<Self, DisplayError> {
            let logical = board.display.ok_or(DisplayError::UnsupportedBoard)?;
            let hardware = board
                .hardware
                .display
                .ok_or(DisplayError::UnsupportedBoard)?;
            if hardware.controller != DisplayController::Axs15231b {
                return Err(DisplayError::UnsupportedBoard);
            }
            let DisplayElectrical::Qspi {
                host,
                clock_gpio,
                data_gpios,
                cs_gpio,
                reset_gpio,
                backlight_gpio,
                clock_hz,
            } = hardware.electrical
            else {
                return Err(DisplayError::UnsupportedBoard);
            };
            let transfer_pixels = usize::from(logical.width)
                .checked_mul(ROWS_PER_TRANSFER)
                .ok_or(DisplayError::InvalidDimensions)?;
            let (frame, transfer) = allocate_buffers(logical.width, logical.height)?;
            let host = host_id(host);
            let mut display = Self {
                host,
                i80_bus: ptr::null_mut(),
                io: ptr::null_mut(),
                panel: ptr::null_mut(),
                width: logical.width,
                height: logical.height,
                frame,
                transfer,
                pending: Box::new(AtomicBool::new(false)),
                bus_initialized: false,
                software_rotate_90: true,
            };
            let mut bus = spi_bus_config_t {
                sclk_io_num: i32::from(clock_gpio),
                flags: SPICOMMON_BUSFLAG_QUAD,
                max_transfer_sz: i32::try_from(
                    transfer_pixels
                        .checked_mul(core::mem::size_of::<u16>())
                        .ok_or(DisplayError::InvalidDimensions)?,
                )
                .map_err(|_| DisplayError::InvalidDimensions)?,
                ..Default::default()
            };
            bus.__bindgen_anon_1.mosi_io_num = i32::from(data_gpios[0]);
            bus.__bindgen_anon_2.miso_io_num = i32::from(data_gpios[1]);
            bus.__bindgen_anon_3.quadwp_io_num = i32::from(data_gpios[2]);
            bus.__bindgen_anon_4.quadhd_io_num = i32::from(data_gpios[3]);
            // SAFETY: the descriptor supplies four distinct QSPI data pins and checked bounds.
            check(unsafe {
                spi_bus_initialize(host, &raw const bus, spi_common_dma_t_SPI_DMA_CH_AUTO)
            })?;
            display.bus_initialized = true;

            let mut io_flags = esp_lcd_panel_io_spi_config_t__bindgen_ty_1::default();
            io_flags.set_quad_mode(1);
            let io_config = esp_lcd_panel_io_spi_config_t {
                cs_gpio_num: i32::from(cs_gpio),
                dc_gpio_num: -1,
                spi_mode: 3,
                pclk_hz: clock_hz,
                trans_queue_depth: 10,
                on_color_trans_done: Some(transfer_complete),
                user_ctx: (&raw const *display.pending).cast_mut().cast(),
                lcd_cmd_bits: 32,
                lcd_param_bits: 8,
                flags: io_flags,
                ..Default::default()
            };
            let lcd_host = i32::try_from(host).map_err(|_| DisplayError::UnsupportedBoard)?;
            // SAFETY: the QSPI bus is live and the callback context has stable allocation.
            check(unsafe {
                esp_lcd_new_panel_io_spi(lcd_host, &raw const io_config, &raw mut display.io)
            })?;

            display.install_axs15231b_panel(reset_gpio, backlight_gpio)?;
            Ok(display)
        }

        fn install_axs15231b_panel(
            &mut self,
            reset_gpio: Option<u8>,
            backlight_gpio: Option<u8>,
        ) -> Result<(), DisplayError> {
            let vendor_commands: Vec<_> = AXS15231B_INIT
                .iter()
                .map(|&(command, parameters, delay_ms)| {
                    esp_idf_sys::display_drivers::axs15231b_lcd_init_cmd_t {
                        cmd: command,
                        data: parameters.as_ptr().cast(),
                        data_bytes: parameters.len(),
                        delay_ms: u32::try_from(delay_ms).unwrap_or(u32::MAX),
                    }
                })
                .collect();
            let mut vendor = esp_idf_sys::display_drivers::axs15231b_vendor_config_t::default();
            vendor.flags.set_use_qspi_interface(1);
            vendor.init_cmds = vendor_commands.as_ptr();
            vendor.init_cmds_size = u16::try_from(vendor_commands.len())
                .map_err(|_| DisplayError::InvalidDimensions)?;
            let panel_config = esp_lcd_panel_dev_config_t {
                reset_gpio_num: reset_gpio.map_or(-1, i32::from),
                __bindgen_anon_1: esp_lcd_panel_dev_config_t__bindgen_ty_1 {
                    rgb_ele_order: lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB,
                },
                bits_per_pixel: 16,
                vendor_config: (&raw mut vendor).cast(),
                ..Default::default()
            };
            // SAFETY: the module types mirror the ESP-IDF ABI and vendor lives through init.
            check(unsafe {
                esp_idf_sys::display_drivers::esp_lcd_new_panel_axs15231b(
                    self.io.cast(),
                    (&raw const panel_config).cast(),
                    (&raw mut self.panel).cast(),
                )
            })?;
            panel_reset(self.panel)?;
            panel_init(self.panel)?;
            // esp_lcd_axs15231b registers its legacy `disp_off` callback in
            // the `disp_on_off` slot, so this driver interprets false as ON.
            panel_display(self.panel, false)?;
            set_output_high(backlight_gpio)
        }

        fn new_gc9107(board: &BoardDescriptor) -> Result<Self, DisplayError> {
            let logical = board.display.ok_or(DisplayError::UnsupportedBoard)?;
            let hardware = board
                .hardware
                .display
                .ok_or(DisplayError::UnsupportedBoard)?;
            if hardware.controller != DisplayController::Gc9107 {
                return Err(DisplayError::UnsupportedBoard);
            }
            let DisplayElectrical::Spi {
                host,
                clock_gpio,
                data_gpio,
                dc_gpio,
                cs_gpio,
                reset_gpio,
                clock_hz,
                ..
            } = hardware.electrical
            else {
                return Err(DisplayError::UnsupportedBoard);
            };
            let transfer_pixels = usize::from(logical.width)
                .checked_mul(ROWS_PER_TRANSFER)
                .ok_or(DisplayError::InvalidDimensions)?;
            let (frame, transfer) = allocate_buffers(logical.width, logical.height)?;
            let host = host_id(host);
            let mut display = Self {
                host,
                i80_bus: ptr::null_mut(),
                io: ptr::null_mut(),
                panel: ptr::null_mut(),
                width: logical.width,
                height: logical.height,
                frame,
                transfer,
                pending: Box::new(AtomicBool::new(false)),
                bus_initialized: false,
                software_rotate_90: false,
            };
            let mut bus = spi_bus_config_t {
                sclk_io_num: i32::from(clock_gpio),
                max_transfer_sz: i32::try_from(
                    transfer_pixels
                        .checked_mul(core::mem::size_of::<u16>())
                        .ok_or(DisplayError::InvalidDimensions)?,
                )
                .map_err(|_| DisplayError::InvalidDimensions)?,
                ..Default::default()
            };
            bus.__bindgen_anon_1.mosi_io_num = i32::from(data_gpio);
            bus.__bindgen_anon_2.miso_io_num = -1;
            bus.__bindgen_anon_3.quadwp_io_num = -1;
            bus.__bindgen_anon_4.quadhd_io_num = -1;
            // SAFETY: descriptor GPIOs and checked transfer bounds remain valid.
            check(unsafe {
                spi_bus_initialize(host, &raw const bus, spi_common_dma_t_SPI_DMA_CH_AUTO)
            })?;
            display.bus_initialized = true;
            let io_config = esp_lcd_panel_io_spi_config_t {
                cs_gpio_num: i32::from(cs_gpio),
                dc_gpio_num: i32::from(dc_gpio),
                spi_mode: 0,
                pclk_hz: clock_hz,
                trans_queue_depth: 1,
                on_color_trans_done: Some(transfer_complete),
                user_ctx: (&raw const *display.pending).cast_mut().cast(),
                lcd_cmd_bits: 8,
                lcd_param_bits: 8,
                ..Default::default()
            };
            let lcd_host = i32::try_from(host).map_err(|_| DisplayError::UnsupportedBoard)?;
            // SAFETY: bus is live and callback context is stable.
            check(unsafe {
                esp_lcd_new_panel_io_spi(lcd_host, &raw const io_config, &raw mut display.io)
            })?;
            let panel_config = esp_lcd_panel_dev_config_t {
                reset_gpio_num: reset_gpio.map_or(-1, i32::from),
                __bindgen_anon_1: esp_lcd_panel_dev_config_t__bindgen_ty_1 {
                    rgb_ele_order: lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB,
                },
                bits_per_pixel: 16,
                ..Default::default()
            };
            // SAFETY: IO is live and output handle belongs to display.
            check(unsafe {
                esp_idf_sys::gc9107::esp_lcd_new_panel_gc9107(
                    display.io.cast(),
                    (&raw const panel_config).cast(),
                    (&raw mut display.panel).cast(),
                )
            })?;
            panel_reset(display.panel)?;
            panel_init(display.panel)?;
            panel_mirror(display.panel, true, true)?;
            panel_set_gap(display.panel, 2, 1)?;
            panel_invert(display.panel, true)?;
            panel_display(display.panel, true)?;
            Ok(display)
        }

        fn new_i80(board: &BoardDescriptor) -> Result<Self, DisplayError> {
            let logical = board.display.ok_or(DisplayError::UnsupportedBoard)?;
            let hardware = board
                .hardware
                .display
                .ok_or(DisplayError::UnsupportedBoard)?;
            if hardware.controller != DisplayController::St7789 {
                return Err(DisplayError::UnsupportedBoard);
            }
            let DisplayElectrical::I80 {
                data_gpios,
                dc_gpio,
                write_gpio,
                read_gpio,
                cs_gpio,
                reset_gpio,
                backlight_gpio,
                power_gpio,
                clock_hz,
            } = hardware.electrical
            else {
                return Err(DisplayError::UnsupportedBoard);
            };
            let transfer_pixels = usize::from(logical.width)
                .checked_mul(ROWS_PER_TRANSFER)
                .ok_or(DisplayError::InvalidDimensions)?;
            let (frame, transfer) = allocate_buffers(logical.width, logical.height)?;
            let mut display = Self {
                host: spi_host_device_t_SPI2_HOST,
                i80_bus: ptr::null_mut(),
                io: ptr::null_mut(),
                panel: ptr::null_mut(),
                width: logical.width,
                height: logical.height,
                frame,
                transfer,
                pending: Box::new(AtomicBool::new(false)),
                bus_initialized: false,
                software_rotate_90: false,
            };

            for gpio in [read_gpio, power_gpio].into_iter().flatten() {
                // SAFETY: descriptor assigns this GPIO to the display interface.
                check(unsafe {
                    gpio_set_direction(i32::from(gpio), gpio_mode_t_GPIO_MODE_OUTPUT)
                })?;
                // SAFETY: the GPIO is now an owned output and both signals idle high.
                check(unsafe { gpio_set_level(i32::from(gpio), 1) })?;
            }

            let mut bus_config = esp_lcd_i80_bus_config_t {
                dc_gpio_num: i32::from(dc_gpio),
                wr_gpio_num: i32::from(write_gpio),
                data_gpio_nums: [-1; 16],
                bus_width: 8,
                max_transfer_bytes: transfer_pixels
                    .checked_mul(core::mem::size_of::<u16>())
                    .ok_or(DisplayError::InvalidDimensions)?,
                sram_trans_align: 4,
                ..Default::default()
            };
            for (destination, source) in bus_config.data_gpio_nums[..8].iter_mut().zip(data_gpios) {
                *destination = i32::from(source);
            }
            bus_config.__bindgen_anon_1.psram_trans_align = 64;
            // SAFETY: configuration and output storage live for the synchronous call.
            check(unsafe { esp_lcd_new_i80_bus(&raw const bus_config, &raw mut display.i80_bus) })?;

            let mut dc_levels = esp_lcd_panel_io_i80_config_t__bindgen_ty_1::default();
            dc_levels.set_dc_idle_level(0);
            dc_levels.set_dc_cmd_level(0);
            dc_levels.set_dc_dummy_level(0);
            dc_levels.set_dc_data_level(1);
            let io_config = esp_lcd_panel_io_i80_config_t {
                cs_gpio_num: i32::from(cs_gpio),
                pclk_hz: clock_hz,
                trans_queue_depth: 1,
                on_color_trans_done: Some(transfer_complete),
                user_ctx: (&raw const *display.pending).cast_mut().cast(),
                lcd_cmd_bits: 8,
                lcd_param_bits: 8,
                dc_levels,
                ..Default::default()
            };
            // SAFETY: the I80 bus is live and callback context is a stable Box.
            check(unsafe {
                esp_lcd_new_panel_io_i80(display.i80_bus, &raw const io_config, &raw mut display.io)
            })?;
            display.initialize_i80_panel(reset_gpio, backlight_gpio)?;
            Ok(display)
        }

        fn initialize_spi_panel(
            &mut self,
            reset_gpio: Option<u8>,
            backlight_gpio: Option<u8>,
            swap_xy: bool,
        ) -> Result<(), DisplayError> {
            self.create_st7789(reset_gpio)?;
            if swap_xy {
                panel_swap_xy(self.panel, true)?;
            }
            panel_mirror(self.panel, true, true)?;
            panel_set_gap(self.panel, 0, 20)?;
            panel_invert(self.panel, true)?;
            panel_display(self.panel, true)?;
            set_output_high(backlight_gpio)
        }

        fn initialize_i80_panel(
            &mut self,
            reset_gpio: Option<u8>,
            backlight_gpio: Option<u8>,
        ) -> Result<(), DisplayError> {
            self.create_st7789(reset_gpio)?;
            panel_invert(self.panel, true)?;
            panel_swap_xy(self.panel, true)?;
            panel_mirror(self.panel, false, true)?;
            panel_set_gap(self.panel, 0, 35)?;
            send_lilygo_init(self.io)?;
            panel_display(self.panel, true)?;
            set_output_high(backlight_gpio)
        }

        fn initialize_sh8601(
            &mut self,
            reset_gpio: Option<u8>,
            backlight_gpio: Option<u8>,
        ) -> Result<(), DisplayError> {
            self.create_vendor_panel(
                reset_gpio,
                lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB,
                VendorPanel::Sh8601,
            )?;
            send_commands(self.io, SH8601_INIT)?;
            panel_set_gap(self.panel, 0, 35)?;
            set_output_high(backlight_gpio)
        }

        fn initialize_st7796(
            &mut self,
            reset_gpio: Option<u8>,
            backlight_gpio: Option<u8>,
        ) -> Result<(), DisplayError> {
            self.create_vendor_panel(
                reset_gpio,
                lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_BGR,
                VendorPanel::St7796,
            )?;
            send_commands(self.io, ST7796_INIT)?;
            panel_invert(self.panel, true)?;
            panel_swap_xy(self.panel, true)?;
            panel_mirror(self.panel, false, true)?;
            panel_display(self.panel, true)?;
            set_output_high(backlight_gpio)
        }

        fn create_vendor_panel(
            &mut self,
            reset_gpio: Option<u8>,
            color_order: u32,
            controller: VendorPanel,
        ) -> Result<(), DisplayError> {
            let panel_config = esp_lcd_panel_dev_config_t {
                reset_gpio_num: reset_gpio.map_or(-1, i32::from),
                __bindgen_anon_1: esp_lcd_panel_dev_config_t__bindgen_ty_1 {
                    rgb_ele_order: color_order,
                },
                bits_per_pixel: 16,
                ..Default::default()
            };
            // SAFETY: panel IO is live, the configuration is borrowed only for
            // the constructor, and `self.panel` is the unique output slot.
            let result = unsafe {
                match controller {
                    VendorPanel::Sh8601 => esp_idf_sys::display_drivers::esp_lcd_new_panel_sh8601(
                        self.io.cast(),
                        (&raw const panel_config).cast(),
                        (&raw mut self.panel).cast(),
                    ),
                    VendorPanel::St7796 => esp_idf_sys::display_drivers::esp_lcd_new_panel_st7796(
                        self.io.cast(),
                        (&raw const panel_config).cast(),
                        (&raw mut self.panel).cast(),
                    ),
                }
            };
            check(result)?;
            panel_reset(self.panel)?;
            panel_init(self.panel)
        }

        fn create_st7789(&mut self, reset_gpio: Option<u8>) -> Result<(), DisplayError> {
            let panel_config = esp_lcd_panel_dev_config_t {
                reset_gpio_num: reset_gpio.map_or(-1, i32::from),
                __bindgen_anon_1: esp_lcd_panel_dev_config_t__bindgen_ty_1 {
                    rgb_ele_order: lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB,
                },
                data_endian: lcd_rgb_data_endian_t_LCD_RGB_DATA_ENDIAN_LITTLE,
                bits_per_pixel: 16,
                ..Default::default()
            };
            // SAFETY: panel IO is live and the output handle belongs to self.
            check(unsafe {
                esp_lcd_new_panel_st7789(self.io, &raw const panel_config, &raw mut self.panel)
            })?;
            // SAFETY: panel is live after the successful constructor.
            panel_reset(self.panel)?;
            panel_init(self.panel)
        }

        fn wait(&self) -> Result<(), DisplayError> {
            let start = std::time::Instant::now();
            while self.pending.load(Ordering::Acquire) {
                if start.elapsed() >= TRANSFER_TIMEOUT {
                    return Err(DisplayError::TransferTimeout);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        }
    }

    impl DisplayDevice for EspSt7789 {
        type Error = DisplayError;

        fn dimensions(&self) -> (u16, u16) {
            (self.width, self.height)
        }

        fn frame_buffer(&mut self) -> &mut [u16] {
            &mut self.frame
        }

        fn present(&mut self) -> Result<(), Self::Error> {
            self.wait()?;
            let width = usize::from(self.width);
            let height = usize::from(self.height);
            if self.software_rotate_90 {
                for start_x in (0..width).step_by(AXS15231B_COLUMNS_PER_TRANSFER) {
                    let columns = (width - start_x).min(AXS15231B_COLUMNS_PER_TRANSFER);
                    let length = columns * height;
                    let transfer = &mut self.transfer.as_mut_slice()[..length];
                    for y in 0..height {
                        for x in 0..columns {
                            transfer[x * height + (height - y - 1)] =
                                self.frame[y * width + start_x + x].swap_bytes();
                        }
                    }
                    self.pending.store(true, Ordering::Release);
                    // SAFETY: panel is live and the rotated DMA allocation remains valid.
                    let result = check(unsafe {
                        esp_lcd_panel_draw_bitmap(
                            self.panel,
                            0,
                            i32::try_from(start_x).map_err(|_| DisplayError::InvalidDimensions)?,
                            i32::try_from(height).map_err(|_| DisplayError::InvalidDimensions)?,
                            i32::try_from(start_x + columns)
                                .map_err(|_| DisplayError::InvalidDimensions)?,
                            self.transfer.pointer().cast(),
                        )
                    });
                    if let Err(error) = result {
                        self.pending.store(false, Ordering::Release);
                        return Err(error);
                    }
                    self.wait()?;
                }
                return Ok(());
            }
            for start_y in (0..height).step_by(ROWS_PER_TRANSFER) {
                let rows = (height - start_y).min(ROWS_PER_TRANSFER);
                let length = width * rows;
                self.transfer.as_mut_slice()[..length]
                    .copy_from_slice(&self.frame[start_y * width..start_y * width + length]);
                self.pending.store(true, Ordering::Release);
                // SAFETY: panel is live and the DMA allocation remains valid until wait completes.
                let result = check(unsafe {
                    esp_lcd_panel_draw_bitmap(
                        self.panel,
                        0,
                        i32::try_from(start_y).map_err(|_| DisplayError::InvalidDimensions)?,
                        i32::from(self.width),
                        i32::try_from(start_y + rows)
                            .map_err(|_| DisplayError::InvalidDimensions)?,
                        self.transfer.pointer().cast(),
                    )
                });
                if let Err(error) = result {
                    self.pending.store(false, Ordering::Release);
                    return Err(error);
                }
                self.wait()?;
            }
            Ok(())
        }
    }

    impl Drop for EspSt7789 {
        fn drop(&mut self) {
            let _ = self.wait();
            if !self.panel.is_null() {
                // SAFETY: this owner created the live panel handle and deletes it exactly once.
                let _ = unsafe { esp_lcd_panel_del(self.panel) };
            }
            if !self.io.is_null() {
                // SAFETY: this owner created the live IO handle and deletes it after the panel.
                let _ = unsafe { esp_lcd_panel_io_del(self.io) };
            }
            if self.bus_initialized {
                // SAFETY: the bus was initialized by this owner and all child handles are gone.
                let _ = unsafe { spi_bus_free(self.host) };
            }
            if !self.i80_bus.is_null() {
                // SAFETY: all I80 child handles were deleted above.
                let _ = unsafe { esp_lcd_del_i80_bus(self.i80_bus) };
            }
        }
    }

    #[derive(Debug)]
    pub struct EspRgbDisplay {
        panel: esp_lcd_panel_handle_t,
        width: u16,
        height: u16,
        frame: NonNull<u16>,
        frame_len: usize,
    }

    impl EspRgbDisplay {
        /// Creates the RGB panel and its PSRAM-backed ESP-IDF scanout buffer.
        ///
        /// # Errors
        ///
        /// Returns descriptor, allocation, or ESP-IDF setup errors.
        pub fn new(board: &BoardDescriptor) -> Result<Self, DisplayError> {
            let logical = board.display.ok_or(DisplayError::UnsupportedBoard)?;
            let hardware = board
                .hardware
                .display
                .ok_or(DisplayError::UnsupportedBoard)?;
            if hardware.controller != DisplayController::RgbPanel {
                return Err(DisplayError::UnsupportedBoard);
            }
            let DisplayElectrical::Rgb {
                data_gpios,
                pclk_gpio,
                vsync_gpio,
                hsync_gpio,
                de_gpio,
                backlight_gpio,
                clock_hz,
                hsync_pulse_width,
                hsync_back_porch,
                hsync_front_porch,
                vsync_pulse_width,
                vsync_back_porch,
                vsync_front_porch,
            } = hardware.electrical
            else {
                return Err(DisplayError::UnsupportedBoard);
            };
            let mut timing_flags = esp_lcd_rgb_timing_t__bindgen_ty_1::default();
            timing_flags.set_pclk_active_neg(1);
            let timings = esp_lcd_rgb_timing_t {
                pclk_hz: clock_hz,
                h_res: u32::from(logical.width),
                v_res: u32::from(logical.height),
                hsync_pulse_width: u32::from(hsync_pulse_width),
                hsync_back_porch: u32::from(hsync_back_porch),
                hsync_front_porch: u32::from(hsync_front_porch),
                vsync_pulse_width: u32::from(vsync_pulse_width),
                vsync_back_porch: u32::from(vsync_back_porch),
                vsync_front_porch: u32::from(vsync_front_porch),
                flags: timing_flags,
            };
            let mut flags = esp_lcd_rgb_panel_config_t__bindgen_ty_2::default();
            flags.set_fb_in_psram(1);
            let mut config = esp_lcd_rgb_panel_config_t {
                timings,
                data_width: 16,
                bits_per_pixel: 16,
                num_fbs: 1,
                bounce_buffer_size_px: usize::from(logical.width) * 10,
                hsync_gpio_num: i32::from(hsync_gpio),
                vsync_gpio_num: i32::from(vsync_gpio),
                de_gpio_num: i32::from(de_gpio),
                pclk_gpio_num: i32::from(pclk_gpio),
                disp_gpio_num: -1,
                data_gpio_nums: data_gpios.map(i32::from),
                flags,
                ..Default::default()
            };
            config.__bindgen_anon_1.psram_trans_align = 64;
            let mut panel = ptr::null_mut();
            // SAFETY: configuration and output storage live for the synchronous call.
            check(unsafe { esp_lcd_new_rgb_panel(&raw const config, &raw mut panel) })?;
            let pixels = usize::from(logical.width)
                .checked_mul(usize::from(logical.height))
                .ok_or(DisplayError::InvalidDimensions)?;
            let mut raw_frame: *mut c_void = ptr::null_mut();
            // SAFETY: panel owns one configured framebuffer and output storage is valid.
            if let Err(error) =
                check(unsafe { esp_lcd_rgb_panel_get_frame_buffer(panel, 1, &raw mut raw_frame) })
            {
                // SAFETY: constructor rollback owns the newly created panel.
                let _ = unsafe { esp_lcd_panel_del(panel) };
                return Err(error);
            }
            let frame = NonNull::new(raw_frame.cast()).ok_or(DisplayError::OutOfMemory)?;
            // SAFETY: the ESP-IDF driver allocated exactly width*height RGB565 pixels.
            unsafe { ptr::write_bytes(frame.as_ptr(), 0, pixels) };
            let display = Self {
                panel,
                width: logical.width,
                height: logical.height,
                frame,
                frame_len: pixels,
            };
            // SAFETY: panel is live and uniquely owned by display.
            panel_reset(display.panel)?;
            panel_init(display.panel)?;
            set_output_high(backlight_gpio)?;
            Ok(display)
        }
    }

    impl DisplayDevice for EspRgbDisplay {
        type Error = DisplayError;

        fn dimensions(&self) -> (u16, u16) {
            (self.width, self.height)
        }

        fn frame_buffer(&mut self) -> &mut [u16] {
            // SAFETY: the panel owns frame_len pixels and &mut self prevents Rust aliases.
            unsafe { core::slice::from_raw_parts_mut(self.frame.as_ptr(), self.frame_len) }
        }

        fn present(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    impl Drop for EspRgbDisplay {
        fn drop(&mut self) {
            // SAFETY: this is the final use of the uniquely owned panel.
            let _ = unsafe { esp_lcd_panel_del(self.panel) };
        }
    }

    #[derive(Debug)]
    pub enum EspDisplay {
        CommandPanel(EspSt7789),
        Rgb(EspRgbDisplay),
    }

    impl EspDisplay {
        /// Creates the display implementation selected by the board descriptor.
        ///
        /// # Errors
        ///
        /// Returns unsupported-controller or concrete driver setup errors.
        pub fn new(board: &BoardDescriptor) -> Result<Self, DisplayError> {
            match board.hardware.display.map(|display| display.controller) {
                Some(
                    DisplayController::St7789
                    | DisplayController::Sh8601
                    | DisplayController::St7796,
                ) => EspSt7789::new(board).map(Self::CommandPanel),
                Some(DisplayController::RgbPanel) => EspRgbDisplay::new(board).map(Self::Rgb),
                Some(DisplayController::Gc9107) => {
                    EspSt7789::new_gc9107(board).map(Self::CommandPanel)
                }
                Some(DisplayController::Axs15231b) => {
                    EspSt7789::new_axs15231b(board).map(Self::CommandPanel)
                }
                _ => Err(DisplayError::UnsupportedBoard),
            }
        }
    }

    /// Forces the descriptor's panel backlight into its known active state.
    ///
    /// This is used by the isolated hardware diagnostic before panel setup so
    /// initialization failures remain visually distinguishable.
    pub fn enable_diagnostic_backlight(board: &BoardDescriptor) -> Result<(), DisplayError> {
        set_diagnostic_backlight(board, true)
    }

    /// Drives the QSPI panel backlight pin to an explicit diagnostic level.
    pub fn set_diagnostic_backlight(
        board: &BoardDescriptor,
        high: bool,
    ) -> Result<(), DisplayError> {
        let Some(DisplayElectrical::Qspi { backlight_gpio, .. }) =
            board.hardware.display.map(|display| display.electrical)
        else {
            return Err(DisplayError::UnsupportedBoard);
        };
        set_output_level(backlight_gpio, high)
    }

    impl DisplayDevice for EspDisplay {
        type Error = DisplayError;

        fn dimensions(&self) -> (u16, u16) {
            match self {
                Self::CommandPanel(display) => display.dimensions(),
                Self::Rgb(display) => display.dimensions(),
            }
        }

        fn frame_buffer(&mut self) -> &mut [u16] {
            match self {
                Self::CommandPanel(display) => display.frame_buffer(),
                Self::Rgb(display) => display.frame_buffer(),
            }
        }

        fn present(&mut self) -> Result<(), Self::Error> {
            match self {
                Self::CommandPanel(display) => display.present(),
                Self::Rgb(display) => display.present(),
            }
        }
    }

    fn allocate_buffers(width: u16, height: u16) -> Result<(Vec<u16>, DmaBuffer), DisplayError> {
        let pixels = usize::from(width)
            .checked_mul(usize::from(height))
            .ok_or(DisplayError::InvalidDimensions)?;
        let transfer_pixels = usize::from(width)
            .checked_mul(ROWS_PER_TRANSFER)
            .ok_or(DisplayError::InvalidDimensions)?;
        let mut frame = Vec::new();
        frame
            .try_reserve_exact(pixels)
            .map_err(|_| DisplayError::OutOfMemory)?;
        frame.resize(pixels, 0);
        Ok((frame, DmaBuffer::new(transfer_pixels)?))
    }

    fn set_output_high(gpio: Option<u8>) -> Result<(), DisplayError> {
        set_output_level(gpio, true)
    }

    fn set_output_level(gpio: Option<u8>, high: bool) -> Result<(), DisplayError> {
        if let Some(gpio) = gpio {
            // SAFETY: the board descriptor assigns this GPIO to the display.
            check(unsafe { gpio_set_direction(i32::from(gpio), gpio_mode_t_GPIO_MODE_OUTPUT) })?;
            // SAFETY: the owned GPIO is configured as an output.
            check(unsafe { gpio_set_level(i32::from(gpio), u32::from(high)) })?;
        }
        Ok(())
    }

    struct DmaBuffer {
        pointer: NonNull<u16>,
        len: usize,
    }

    impl DmaBuffer {
        fn new(len: usize) -> Result<Self, DisplayError> {
            let bytes = len
                .checked_mul(core::mem::size_of::<u16>())
                .ok_or(DisplayError::InvalidDimensions)?;
            // The JC3248W535 reference driver requires a 32-byte-aligned
            // internal DMA buffer. QSPI transfers can otherwise show
            // intermittent edge corruption even when allocation succeeds.
            let raw =
                // SAFETY: bytes is checked and the returned allocation is validated below.
                unsafe { heap_caps_aligned_alloc(32, bytes, MALLOC_CAP_DMA | MALLOC_CAP_INTERNAL) };
            let pointer = NonNull::new(raw.cast()).ok_or(DisplayError::OutOfMemory)?;
            Ok(Self { pointer, len })
        }

        fn pointer(&self) -> *mut u16 {
            self.pointer.as_ptr()
        }

        fn as_mut_slice(&mut self) -> &mut [u16] {
            // SAFETY: pointer owns len u16 elements and &mut self prevents aliasing.
            unsafe { core::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.len) }
        }
    }

    impl Drop for DmaBuffer {
        fn drop(&mut self) {
            // SAFETY: pointer came from heap_caps_malloc and is freed exactly once here.
            unsafe { heap_caps_free(self.pointer.as_ptr().cast()) };
        }
    }

    unsafe extern "C" fn transfer_complete(
        _io: esp_lcd_panel_io_handle_t,
        _event: *mut esp_lcd_panel_io_event_data_t,
        context: *mut c_void,
    ) -> bool {
        // SAFETY: panel IO stores the stable boxed AtomicBool pointer until callback teardown.
        if let Some(pending) = unsafe { context.cast::<AtomicBool>().as_ref() } {
            pending.store(false, Ordering::Release);
        }
        false
    }

    #[derive(Clone, Copy)]
    enum VendorPanel {
        Sh8601,
        St7796,
    }

    type InitCommand = (i32, &'static [u8], u64);

    const AXS15231B_INIT: &[InitCommand] = &[
        (0xbb, &[0, 0, 0, 0, 0, 0, 0x5a, 0xa5], 0),
        (
            0xa0,
            &[
                0xc0, 0x10, 0, 2, 0, 0, 4, 0x3f, 0x20, 5, 0x3f, 0x3f, 0, 0, 0, 0, 0,
            ],
            0,
        ),
        (
            0xa2,
            &[
                0x30, 0x3c, 0x24, 0x14, 0xd0, 0x20, 0xff, 0xe0, 0x40, 0x19, 0x80, 0x80, 0x80, 0x20,
                0xf9, 0x10, 2, 0xff, 0xff, 0xf0, 0x90, 1, 0x32, 0xa0, 0x91, 0xe0, 0x20, 0x7f, 0xff,
                0, 0x5a,
            ],
            0,
        ),
        (
            0xd0,
            &[
                0xe0, 0x40, 0x51, 0x24, 8, 5, 0x10, 1, 0x20, 0x15, 0x42, 0xc2, 0x22, 0x22, 0xaa, 3,
                0x10, 0x12, 0x60, 0x14, 0x1e, 0x51, 0x15, 0, 0x8a, 0x20, 0, 3, 0x3a, 0x12,
            ],
            0,
        ),
        (
            0xa3,
            &[
                0xa0, 6, 0xaa, 0, 8, 2, 0x0a, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 0, 0x55, 0x55,
            ],
            0,
        ),
        (
            0xc1,
            &[
                0x31, 4, 2, 2, 0x71, 5, 0x24, 0x55, 2, 0, 0x41, 0, 0x53, 0xff, 0xff, 0xff, 0x4f,
                0x52, 0, 0x4f, 0x52, 0, 0x45, 0x3b, 0x0b, 2, 0x0d, 0, 0xff, 0x40,
            ],
            0,
        ),
        (0xc3, &[0, 0, 0, 0x50, 3, 0, 0, 0, 1, 0x80, 1], 0),
        (
            0xc4,
            &[
                0, 0x24, 0x33, 0x80, 0, 0xea, 0x64, 0x32, 0xc8, 0x64, 0xc8, 0x32, 0x90, 0x90, 0x11,
                6, 0xdc, 0xfa, 0, 0, 0x80, 0xfe, 0x10, 0x10, 0, 0x0a, 0x0a, 0x44, 0x50,
            ],
            0,
        ),
        (
            0xc5,
            &[
                0x18, 0, 0, 3, 0xfe, 0x3a, 0x4a, 0x20, 0x30, 0x10, 0x88, 0xde, 0x0d, 8, 0x0f, 0x0f,
                1, 0x3a, 0x4a, 0x20, 0x10, 0x10, 0,
            ],
            0,
        ),
        (
            0xc6,
            &[
                5, 0x0a, 5, 0x0a, 0, 0xe0, 0x2e, 0x0b, 0x12, 0x22, 0x12, 0x22, 1, 3, 0, 0x3f, 0x6a,
                0x18, 0xc8, 0x22,
            ],
            0,
        ),
        (
            0xc7,
            &[
                0x50, 0x32, 0x28, 0, 0xa2, 0x80, 0x8f, 0, 0x80, 0xff, 7, 0x11, 0x9c, 0x67, 0xff,
                0x24, 0x0c, 0x0d, 0x0e, 0x0f,
            ],
            0,
        ),
        (0xc9, &[0x33, 0x44, 0x44, 1], 0),
        (
            0xcf,
            &[
                0x2c, 0x1e, 0x88, 0x58, 0x13, 0x18, 0x56, 0x18, 0x1e, 0x68, 0x88, 0, 0x65, 9, 0x22,
                0xc4, 0x0c, 0x77, 0x22, 0x44, 0xaa, 0x55, 8, 8, 0x12, 0xa0, 8,
            ],
            0,
        ),
        (
            0xd5,
            &[
                0x40, 0x8e, 0x8d, 1, 0x35, 4, 0x92, 0x74, 4, 0x92, 0x74, 4, 8, 0x6a, 4, 0x46, 3, 3,
                3, 3, 0x82, 1, 3, 0, 0xe0, 0x51, 0xa1, 0, 0, 0,
            ],
            0,
        ),
        (
            0xd6,
            &[
                0x10, 0x32, 0x54, 0x76, 0x98, 0xba, 0xdc, 0xfe, 0x93, 0, 1, 0x83, 7, 7, 0, 7, 7, 0,
                3, 3, 3, 3, 3, 3, 0, 0x84, 0, 0x20, 1, 0,
            ],
            0,
        ),
        (
            0xd7,
            &[
                3, 1, 0x0b, 9, 0x0f, 0x0d, 0x1e, 0x1f, 0x18, 0x1d, 0x1f, 0x19, 0x40, 0x8e, 4, 0,
                0x20, 0xa0, 0x1f,
            ],
            0,
        ),
        (
            0xd8,
            &[
                2, 0, 0x0a, 8, 0x0e, 0x0c, 0x1e, 0x1f, 0x18, 0x1d, 0x1f, 0x19,
            ],
            0,
        ),
        (0xd9, &[0x1f; 12], 0),
        (0xdd, &[0x1f; 12], 0),
        (0xdf, &[0x44, 0x73, 0x4b, 0x69, 0, 0x0a, 2, 0x90], 0),
        (
            0xe0,
            &[
                0x3b, 0x28, 0x10, 0x16, 0x0c, 6, 0x11, 0x28, 0x5c, 0x21, 0x0d, 0x35, 0x13, 0x2c,
                0x33, 0x28, 0x0d,
            ],
            0,
        ),
        (
            0xe1,
            &[
                0x37, 0x28, 0x10, 0x16, 0x0b, 6, 0x11, 0x28, 0x5c, 0x21, 0x0d, 0x35, 0x14, 0x2c,
                0x33, 0x28, 0x0f,
            ],
            0,
        ),
        (
            0xe2,
            &[
                0x3b, 7, 0x12, 0x18, 0x0e, 0x0d, 0x17, 0x35, 0x44, 0x32, 0x0c, 0x14, 0x14, 0x36,
                0x3a, 0x2f, 0x0d,
            ],
            0,
        ),
        (
            0xe3,
            &[
                0x37, 7, 0x12, 0x18, 0x0e, 0x0d, 0x17, 0x35, 0x44, 0x32, 0x0c, 0x14, 0x14, 0x36,
                0x32, 0x2f, 0x0f,
            ],
            0,
        ),
        (
            0xe4,
            &[
                0x3b, 7, 0x12, 0x18, 0x0e, 0x0d, 0x17, 0x39, 0x44, 0x2e, 0x0c, 0x14, 0x14, 0x36,
                0x3a, 0x2f, 0x0d,
            ],
            0,
        ),
        (
            0xe5,
            &[
                0x37, 7, 0x12, 0x18, 0x0e, 0x0d, 0x17, 0x39, 0x44, 0x2e, 0x0c, 0x14, 0x14, 0x36,
                0x3a, 0x2f, 0x0f,
            ],
            0,
        ),
        (
            0xa4,
            &[
                0x85, 0x85, 0x95, 0x82, 0xaf, 0xaa, 0xaa, 0x80, 0x10, 0x30, 0x40, 0x40, 0x20, 0xff,
                0x60, 0x30,
            ],
            0,
        ),
        (0xa4, &[0x85, 0x85, 0x95, 0x85], 0),
        (0xbb, &[0; 8], 0),
        (0x13, &[], 0),
        (0x11, &[], 120),
        (0x2c, &[0, 0, 0, 0], 0),
    ];

    const SH8601_INIT: &[InitCommand] = &[
        (0x36, &[0x70], 0),
        (0xb2, &[0x0c, 0x0c, 0x00, 0x33, 0x33], 0),
        (0xb7, &[0x35], 0),
        (0xbb, &[0x13], 0),
        (0xc0, &[0x2c], 0),
        (0xc2, &[0x01], 0),
        (0xc3, &[0x0b], 0),
        (0xc4, &[0x20], 0),
        (0xc6, &[0x0f], 0),
        (0xd0, &[0xa4, 0xa1], 0),
        (0xd6, &[0xa1], 0),
        (
            0xe0,
            &[
                0x00, 0x03, 0x07, 0x08, 0x07, 0x15, 0x2a, 0x44, 0x42, 0x0a, 0x17, 0x18, 0x25, 0x27,
            ],
            0,
        ),
        (
            0xe1,
            &[
                0x00, 0x03, 0x08, 0x07, 0x07, 0x23, 0x2a, 0x43, 0x42, 0x09, 0x18, 0x17, 0x25, 0x27,
            ],
            0,
        ),
        (0x21, &[], 0),
        (0x11, &[], 120),
        (0x29, &[], 0),
    ];

    const ST7796_INIT: &[InitCommand] = &[
        (0x3a, &[0x55], 0),
        (0xf0, &[0xc3], 0),
        (0xf0, &[0x96], 0),
        (0xb4, &[0x01], 0),
        (0xb6, &[0x80, 0x22, 0x3b], 0),
        (0xe8, &[0x40, 0x8a, 0x00, 0x00, 0x29, 0x19, 0xa5, 0x33], 0),
        (0xc1, &[0x06], 0),
        (0xc2, &[0xa7], 0),
        (0xc5, &[0x18], 0),
        (
            0xe0,
            &[
                0xf0, 0x09, 0x0b, 0x06, 0x04, 0x15, 0x2f, 0x54, 0x42, 0x3c, 0x17, 0x14, 0x18, 0x1b,
            ],
            0,
        ),
        (
            0xe1,
            &[
                0xe0, 0x09, 0x0b, 0x06, 0x04, 0x03, 0x2b, 0x43, 0x42, 0x3b, 0x16, 0x14, 0x17, 0x1b,
            ],
            0,
        ),
        (0xf0, &[0x3c], 0),
        (0xf0, &[0x69], 0),
        (0x11, &[], 120),
        (0x38, &[], 0),
        (0x29, &[], 120),
    ];

    fn send_commands(
        io: esp_lcd_panel_io_handle_t,
        commands: &[InitCommand],
    ) -> Result<(), DisplayError> {
        for &(command, parameters, delay_ms) in commands {
            let pointer = if parameters.is_empty() {
                ptr::null()
            } else {
                parameters.as_ptr()
            };
            // SAFETY: IO is live and each static parameter slice covers the synchronous call.
            check(unsafe {
                esp_lcd_panel_io_tx_param(io, command, pointer.cast(), parameters.len())
            })?;
            if delay_ms != 0 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
        }
        Ok(())
    }

    fn send_lilygo_init(io: esp_lcd_panel_io_handle_t) -> Result<(), DisplayError> {
        const POSITIVE_GAMMA: [u8; 14] = [
            0xf0, 0x05, 0x0a, 0x06, 0x06, 0x03, 0x2b, 0x32, 0x43, 0x36, 0x11, 0x10, 0x2b, 0x32,
        ];
        const NEGATIVE_GAMMA: [u8; 14] = [
            0xf0, 0x08, 0x0c, 0x0b, 0x09, 0x24, 0x2b, 0x22, 0x43, 0x38, 0x15, 0x16, 0x2f, 0x37,
        ];
        const COMMANDS: &[(i32, &[u8])] = &[
            (0x11, &[]),
            (0x3a, &[0x05]),
            (0xb2, &[0x0b, 0x0b, 0x00, 0x33, 0x33]),
            (0xb7, &[0x75]),
            (0xbb, &[0x28]),
            (0xc0, &[0x2c]),
            (0xc2, &[0x01]),
            (0xc3, &[0x1f]),
            (0xc6, &[0x13]),
            (0xd0, &[0xa7]),
            (0xd0, &[0xa4, 0xa1]),
            (0xd6, &[0xa1]),
            (0xe0, &POSITIVE_GAMMA),
            (0xe1, &NEGATIVE_GAMMA),
        ];
        for (index, (command, parameters)) in COMMANDS.iter().enumerate() {
            let pointer: *const u8 = if parameters.is_empty() {
                ptr::null()
            } else {
                parameters.as_ptr()
            };
            // SAFETY: IO is live and the static parameter slice covers the full call.
            check(unsafe {
                esp_lcd_panel_io_tx_param(io, *command, pointer.cast(), parameters.len())
            })?;
            if index == 0 {
                std::thread::sleep(Duration::from_millis(120));
            }
        }
        Ok(())
    }

    const fn host_id(host: SpiHost) -> spi_host_device_t {
        match host {
            SpiHost::Spi2 => spi_host_device_t_SPI2_HOST,
            SpiHost::Spi3 => spi_host_device_t_SPI3_HOST,
        }
    }

    fn panel_reset(panel: esp_lcd_panel_handle_t) -> Result<(), DisplayError> {
        // SAFETY: callers pass a live panel handle uniquely owned by their
        // display object; ESP-IDF does not retain any Rust reference.
        check(unsafe { esp_lcd_panel_reset(panel) })
    }

    fn panel_init(panel: esp_lcd_panel_handle_t) -> Result<(), DisplayError> {
        // SAFETY: callers invoke initialization only after successful panel
        // construction and before the handle can be deleted.
        check(unsafe { esp_lcd_panel_init(panel) })
    }

    fn panel_swap_xy(panel: esp_lcd_panel_handle_t, enabled: bool) -> Result<(), DisplayError> {
        // SAFETY: the handle remains live for this synchronous configuration call.
        check(unsafe { esp_lcd_panel_swap_xy(panel, enabled) })
    }

    fn panel_mirror(
        panel: esp_lcd_panel_handle_t,
        mirror_x: bool,
        mirror_y: bool,
    ) -> Result<(), DisplayError> {
        // SAFETY: the handle remains live for this synchronous configuration call.
        check(unsafe { esp_lcd_panel_mirror(panel, mirror_x, mirror_y) })
    }

    fn panel_set_gap(panel: esp_lcd_panel_handle_t, x: i32, y: i32) -> Result<(), DisplayError> {
        // SAFETY: the handle remains live and scalar offsets are copied by ESP-IDF.
        check(unsafe { esp_lcd_panel_set_gap(panel, x, y) })
    }

    fn panel_invert(panel: esp_lcd_panel_handle_t, enabled: bool) -> Result<(), DisplayError> {
        // SAFETY: the handle remains live for this synchronous configuration call.
        check(unsafe { esp_lcd_panel_invert_color(panel, enabled) })
    }

    fn panel_display(panel: esp_lcd_panel_handle_t, enabled: bool) -> Result<(), DisplayError> {
        // SAFETY: the handle remains live for this synchronous configuration call.
        check(unsafe { esp_lcd_panel_disp_on_off(panel, enabled) })
    }

    fn check(result: esp_err_t) -> Result<(), DisplayError> {
        if result == ESP_OK {
            Ok(())
        } else {
            Err(DisplayError::Esp(result))
        }
    }
}

#[cfg(target_os = "espidf")]
pub use esp::{
    DisplayError, EspDisplay, EspRgbDisplay, EspSt7789, enable_diagnostic_backlight,
    set_diagnostic_backlight,
};
