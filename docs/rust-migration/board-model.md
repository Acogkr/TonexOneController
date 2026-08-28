# Board and reusable-driver model

Status: source-verified metadata and target builds; hardware validation pending

## Three levels

The Rust board registry separates:

1. **Build variant** — release identity, legacy sdkconfig key, selected Flash
   size, PSRAM mode, LED capability, and UI orientation.
2. **Hardware profile** — one wiring and component combination with a
   reusable platform composition.
3. **Component driver** — display, touch, I/O expander, LED, or bus driver
   shared by every hardware profile using that component.

This prevents product variants such as Pirate43B from copying the
Waveshare 4.3B RGB/GT911 implementation.

Display hardware records the bus host, complete signal wiring,
backlight/reset availability, and source-derived pixel clock so a driver can
be composed without reopening the legacy platform header.

## Source-verified component mapping

| Hardware profile | Display | Bus | Touch | C evidence |
| --- | --- | --- | --- | --- |
| Waveshare 1.69 | ST7789, 240×280 | SPI | none | `platform_ws169.c` |
| Waveshare 1.69 Touch | ST7789, 240×280 | SPI | CST816 | `platform_ws169.c` |
| Waveshare 4.3B | raw RGB panel, 800×480 | RGB | GT911 | `platform_ws43.c` |
| Waveshare 3.5B | AXS15231B, 480×320 | QSPI | AXS15231B | `platform_ws35b.c` |
| JC3248W535 | AXS15231B, 480×320 | QSPI | AXS15231B | `platform_jc3248w.c` |
| M5 AtomS3R | GC9107, 128×128 | SPI | none | `platform_m5atoms3r.c` |
| LilyGo T-Display-S3 | ST7789, 320×170 | i80 | CST816 or CST328 | `platform_lgtdisps3.c` |
| Waveshare 1.9 Touch | SH8601, 320×170 | SPI | CST816 | `platform_ws19t.c` |
| Waveshare 7/4.3 | raw RGB panel, 800×480 | RGB | GT911 | `platform_ws7.c` |
| Pirate Polar Pro | ST7789, 240×280 | SPI | none in release sdkconfig | `platform_pirate_pro_169.c` |
| Pirate Polar Max V2 | ST7796, 480×320 | SPI | GT911 | `platform_pirate_max35.c` |

Headless Waveshare Zero and DevKit-C profiles have no display or touch
driver.

## Memory interpretation

`configured_flash_mb` records `CONFIG_ESPTOOLPY_FLASHSIZE` in the legacy
sdkconfig. It does not claim the maximum physical Flash size. This distinction
matters for `DEVKITC_N16R8`: the legacy build key suggests 16 MB physical
Flash, while its current sdkconfig selects an 8 MB flashing size.

PSRAM mode records the legacy Quad/Octal setting. Physical PSRAM capacity is
not yet included because it has not been established consistently for every
product variant from authoritative local evidence.

## Validation status

All build variants remain Tier 3. The mappings and optimized target builds
prove source coverage and compilation, but do not prove:

- correct physical board revision;
- correct physical Flash/PSRAM capacity;
- display color/order/timing;
- touch reset, interrupt, or coordinate behavior;
- USB Host power behavior;
- real-device runtime operation.

These require target builds and physical tests before tier promotion.
