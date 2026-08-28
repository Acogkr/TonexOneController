#![no_std]

use core::{
    convert::Infallible,
    fmt::{self, Write},
};

use embedded_graphics::{
    Drawable,
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_6X10, FONT_7X13_BOLD, FONT_9X15_BOLD, FONT_9X18_BOLD, FONT_10X20},
    },
    pixelcolor::{IntoStorage, Rgb565},
    prelude::{Pixel, Primitive, RgbColor},
    primitives::{
        Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
    },
    text::{Baseline, Text},
};
use qrcode::{Color as QrColor, EcLevel, QrCode};
use tonex_domain::Slot;
use tonex_parameters::{ParameterKind, all_specs};
use tonex_ui_model::{
    BluetoothState, StageLayout, StatusText, ToneProfile, UiClass, UiPage, UiRect, UiViewModel,
    settings_layout, stage_layout,
};

const BACKGROUND: Rgb565 = Rgb565::new(1, 2, 2);
const PANEL: Rgb565 = Rgb565::new(3, 6, 5);
const PANEL_RAISED: Rgb565 = Rgb565::new(5, 10, 8);
const TEXT: Rgb565 = Rgb565::new(30, 59, 27);
const MUTED: Rgb565 = Rgb565::new(15, 29, 15);
const CONNECTED: Rgb565 = Rgb565::new(5, 52, 10);
const DISCONNECTED: Rgb565 = Rgb565::new(29, 9, 8);
const BLUETOOTH_IDLE: Rgb565 = Rgb565::new(14, 28, 14);
const BLUETOOTH_CONNECTED: Rgb565 = Rgb565::new(4, 28, 31);
const MUTE_RED: Rgb565 = Rgb565::new(31, 5, 5);
const MUTE_PANEL: Rgb565 = Rgb565::new(8, 1, 1);
pub const SKIN_SOURCE_WIDTH: usize = 240;
pub const SKIN_SOURCE_HEIGHT: usize = 80;
const SKIN_COUNT: usize = 50;
static SKIN_RLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/skins.rle"));
static SKIN_ROW_OFFSETS: [u32; SKIN_COUNT * SKIN_SOURCE_HEIGHT + 1] =
    include!(concat!(env!("OUT_DIR"), "/skin_row_offsets.rs"));
const WEB_AMP_SKIN_COUNT: usize = 28;
static WEB_AMP_SKINS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/web_amp_skins.pngs"));
static WEB_AMP_SKIN_OFFSETS: [u32; WEB_AMP_SKIN_COUNT + 1] =
    include!(concat!(env!("OUT_DIR"), "/web_amp_skin_offsets.rs"));

/// Decodes one build-time converted RGB565 row into caller-owned storage.
///
/// The lossless row-RLE representation removes repeated panel/background
/// pixels from flash without adding a heap allocation or runtime PNG decoder.
/// Returns false only for invalid row/storage input or corrupt generated data.
pub fn decode_skin_row(
    id: tonex_skins::SkinId,
    row: usize,
    destination: &mut [u16; SKIN_SOURCE_WIDTH],
) -> bool {
    if row >= SKIN_SOURCE_HEIGHT {
        return false;
    }
    let offset_index = usize::from(id.get()) * SKIN_SOURCE_HEIGHT + row;
    let Ok(start) = usize::try_from(SKIN_ROW_OFFSETS[offset_index]) else {
        return false;
    };
    let Ok(end) = usize::try_from(SKIN_ROW_OFFSETS[offset_index + 1]) else {
        return false;
    };
    let Some(encoded) = SKIN_RLE.get(start..end) else {
        return false;
    };
    let mut written = 0;
    let mut runs = encoded.chunks_exact(3);
    for run in &mut runs {
        let length = usize::from(run[0]);
        if length == 0 || written + length > destination.len() {
            return false;
        }
        let pixel = u16::from_le_bytes([run[1], run[2]]);
        destination[written..written + length].fill(pixel);
        written += length;
    }
    runs.remainder().is_empty() && written == destination.len()
}

/// Returns a browser-native PNG for amplifier skins.
#[must_use]
pub fn web_amp_skin_png(id: tonex_skins::SkinId) -> Option<&'static [u8]> {
    let index = usize::from(id.get());
    if index >= WEB_AMP_SKIN_COUNT {
        return None;
    }
    let start = usize::try_from(WEB_AMP_SKIN_OFFSETS[index]).ok()?;
    let end = usize::try_from(WEB_AMP_SKIN_OFFSETS[index + 1]).ok()?;
    WEB_AMP_SKINS.get(start..end)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderError {
    Headless,
    BufferSize { expected: usize, actual: usize },
}

/// Renders a complete RGB565 snapshot into caller-owned storage.
///
/// # Errors
///
/// Rejects headless layouts and buffers that do not exactly match the selected
/// UI class.
pub fn render(model: &UiViewModel, pixels: &mut [u16]) -> Result<(), RenderError> {
    if model.class == UiClass::Headless {
        return Err(RenderError::Headless);
    }
    let width = usize::from(model.metrics.width);
    let height = usize::from(model.metrics.height);
    let expected = width.checked_mul(height).ok_or(RenderError::BufferSize {
        expected: usize::MAX,
        actual: pixels.len(),
    })?;
    if pixels.len() != expected {
        return Err(RenderError::BufferSize {
            expected,
            actual: pixels.len(),
        });
    }

    let mut target = Framebuffer {
        pixels,
        size: Size::new(
            u32::from(model.metrics.width),
            u32::from(model.metrics.height),
        ),
    };
    let margin = i32::from(model.metrics.margin);
    let width_i32 = i32::from(model.metrics.width);
    let height_i32 = i32::from(model.metrics.height);
    if model.class == UiClass::Medium480x320 {
        // The JC3248W535 panel is already framed by a physical bezel. A second
        // black, rounded outer frame made the rendered surface look undersized.
        // Keep the content safe-area margin, but paint the complete LCD surface.
        unwrap_infallible(target.clear(PANEL));
    } else {
        unwrap_infallible(target.clear(BACKGROUND));
        unwrap_infallible(
            rounded(
                Point::new(margin, margin),
                Size::new(
                    u32::from(model.metrics.width - model.metrics.margin * 2),
                    u32::from(model.metrics.height - model.metrics.margin * 2),
                ),
                12,
            )
            .into_styled(PrimitiveStyle::with_fill(PANEL))
            .draw(&mut target),
        );
    }
    match model.snapshot.page {
        UiPage::Settings => {
            draw_settings_menu(&mut target, model);
            return Ok(());
        }
        UiPage::QuickConnect => {
            draw_quick_connect(&mut target, model, margin, width_i32, height_i32);
            return Ok(());
        }
        UiPage::Display => {
            draw_display_info(&mut target, model);
            return Ok(());
        }
        UiPage::DeviceInfo => {
            draw_device_info(&mut target, model);
            return Ok(());
        }
        UiPage::Tuner => {
            draw_tuner(&mut target, model, margin, width_i32, height_i32);
            return Ok(());
        }
        UiPage::Presets { offset } => {
            draw_presets(&mut target, model, offset);
            return Ok(());
        }
        UiPage::EditMenu { offset } => {
            draw_edit_menu(&mut target, model, offset);
            return Ok(());
        }
        UiPage::Edit { block, offset } => {
            draw_parameter_page(&mut target, model, block.label(), offset, |raw| {
                block.contains_parameter(raw)
            });
            return Ok(());
        }
        UiPage::Global { offset } => {
            draw_parameter_page(&mut target, model, "GLOBAL", offset, |raw| raw >= 110);
            return Ok(());
        }
        UiPage::Stage => {}
    }
    draw_content(&mut target, model, margin, width_i32, height_i32);
    Ok(())
}

fn draw_edit_menu(target: &mut Framebuffer<'_>, model: &UiViewModel, offset: u8) {
    let accent = theme_colour(model);
    let (content_top, content_bottom, rows) = draw_subpage_header(target, model, "EDIT", accent);
    let rows = rows.min(tonex_ui_model::EDIT_BLOCKS.len());
    let gap: u16 = if model.class == UiClass::Tiny128 {
        2
    } else {
        5
    };
    let available = content_bottom.saturating_sub(content_top);
    let row_count = u16::try_from(rows).unwrap_or(1).max(1);
    let row_height = (available.saturating_sub(gap.saturating_mul(row_count.saturating_sub(1)))
        / row_count)
        .max(12);
    let width = model.metrics.width.saturating_sub(model.metrics.margin * 2);
    for row in 0..rows {
        let index = usize::from(offset).saturating_add(row);
        let Some(block) = tonex_ui_model::EDIT_BLOCKS.get(index).copied() else {
            break;
        };
        let rect = UiRect::new(
            model.metrics.margin,
            content_top.saturating_add(
                u16::try_from(row)
                    .unwrap_or(0)
                    .saturating_mul(row_height.saturating_add(gap)),
            ),
            width,
            row_height,
        );
        draw_list_card(target, rect, false, accent);
        let font = if model.class == UiClass::Tiny128 {
            &FONT_6X10
        } else {
            &FONT_7X13_BOLD
        };
        draw_text_left_centered(target, block.label(), rect, font, TEXT, 12);
        draw_text_right_centered(target, ">", rect, font, accent, 12);
    }
}

fn draw_presets(target: &mut Framebuffer<'_>, model: &UiViewModel, offset: u8) {
    let accent = theme_colour(model);
    let (content_top, content_bottom, rows) = draw_subpage_header(target, model, "PRESETS", accent);
    let rows = rows.min(tonex_domain::PRESET_COUNT);
    let gap: u16 = if model.class == UiClass::Tiny128 {
        2
    } else {
        5
    };
    let available = content_bottom.saturating_sub(content_top);
    let row_count = u16::try_from(rows).unwrap_or(1).max(1);
    let row_height = (available.saturating_sub(gap.saturating_mul(row_count.saturating_sub(1)))
        / row_count)
        .max(12);
    let width = model.metrics.width.saturating_sub(model.metrics.margin * 2);
    for row in 0..rows {
        let index = usize::from(offset).saturating_add(row);
        if index >= tonex_domain::PRESET_COUNT {
            break;
        }
        let rect = UiRect::new(
            model.metrics.margin,
            content_top.saturating_add(
                u16::try_from(row)
                    .unwrap_or(0)
                    .saturating_mul(row_height.saturating_add(gap)),
            ),
            width,
            row_height,
        );
        let selected = index == usize::from(model.snapshot.preset.get());
        draw_list_card(target, rect, selected, accent);
        let mut number = TextBuffer::<4>::default();
        let _ = write!(number, "{:02}", index + 1);
        let font = if model.class == UiClass::Tiny128 {
            &FONT_6X10
        } else {
            &FONT_7X13_BOLD
        };
        draw_text_left_centered(target, number.as_str(), rect, font, accent, 10);
        let label = model.snapshot.preset_labels[index];
        let label = core::str::from_utf8(label.as_bytes()).unwrap_or_default();
        let label = if label.is_empty() {
            "WAITING..."
        } else {
            label
        };
        let label_rect = UiRect::new(
            rect.x.saturating_add(if model.class == UiClass::Tiny128 {
                24
            } else {
                42
            }),
            rect.y,
            rect.width
                .saturating_sub(if model.class == UiClass::Tiny128 {
                    28
                } else {
                    48
                }),
            rect.height,
        );
        draw_text_left_centered(target, label, label_rect, font, TEXT, 0);
    }
}

fn draw_parameter_page<F>(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    title: &str,
    offset: u8,
    belongs: F,
) where
    F: Fn(u16) -> bool,
{
    let accent = theme_colour(model);
    let (content_top, content_bottom, default_rows) =
        draw_subpage_header(target, model, title, accent);
    let rows = default_rows.min(5);
    let gap: u16 = if model.class == UiClass::Tiny128 {
        2
    } else {
        6
    };
    let available = content_bottom.saturating_sub(content_top);
    let row_count = u16::try_from(rows).unwrap_or(1).max(1);
    let row_height = (available.saturating_sub(gap.saturating_mul(row_count.saturating_sub(1)))
        / row_count)
        .max(12);
    let width = model.metrics.width.saturating_sub(model.metrics.margin * 2);
    let mut matched = 0_usize;
    let mut displayed = 0_usize;
    for (definition, value) in all_specs().iter().zip(model.snapshot.parameters) {
        if !belongs(definition.id.get()) {
            continue;
        }
        if matched < usize::from(offset) {
            matched += 1;
            continue;
        }
        if displayed >= rows {
            break;
        }
        let rect = UiRect::new(
            model.metrics.margin,
            content_top.saturating_add(
                u16::try_from(displayed)
                    .unwrap_or(0)
                    .saturating_mul(row_height.saturating_add(gap)),
            ),
            width,
            row_height,
        );
        draw_list_card(target, rect, false, accent);
        let font = if model.class == UiClass::Tiny128 {
            &FONT_6X10
        } else {
            &FONT_7X13_BOLD
        };
        draw_text_left_centered(target, definition.label, rect, font, TEXT, 10);
        let mut value_text = TextBuffer::<20>::default();
        match definition.kind {
            ParameterKind::Switch => {
                let _ = value_text.write_str(if value == 0.0 { "OFF" } else { "ON" });
            }
            ParameterKind::Select => {
                let _ = write!(value_text, "{value:.0}");
            }
            ParameterKind::Range => {
                let _ = write!(value_text, "{value:.1}");
            }
        }
        draw_text_right_centered(target, value_text.as_str(), rect, font, accent, 10);
        matched += 1;
        displayed += 1;
    }
}

fn draw_subpage_header(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    title: &str,
    accent: Rgb565,
) -> (u16, u16, usize) {
    let layout = stage_layout(model.class, model.has_touch);
    let header_height: u16 = match model.class {
        UiClass::Tiny128 => 24,
        UiClass::Compact320x170 => 42,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => 56,
        UiClass::Medium480x320 => 70,
        UiClass::Large800x480 => 108,
        UiClass::Headless => 0,
    };
    let title_rect = UiRect::new(
        model.metrics.margin,
        model.metrics.margin,
        model.metrics.width.saturating_sub(model.metrics.margin * 2),
        header_height.saturating_sub(model.metrics.margin),
    );
    let font = match model.class {
        UiClass::Tiny128 => &FONT_7X13_BOLD,
        UiClass::Compact320x170 | UiClass::Portrait240x280 | UiClass::Landscape280x240 => {
            &FONT_9X15_BOLD
        }
        UiClass::Medium480x320 | UiClass::Large800x480 => &FONT_10X20,
        UiClass::Headless => &FONT_6X10,
    };
    draw_text_centered(target, title, title_rect, font, TEXT);
    if let Some(back) = layout.settings {
        draw_back_button(target, back, accent);
    }
    let rows = match model.class {
        UiClass::Tiny128 | UiClass::Landscape280x240 => 4,
        UiClass::Compact320x170 => 3,
        UiClass::Portrait240x280 | UiClass::Medium480x320 => 5,
        UiClass::Large800x480 => 7,
        UiClass::Headless => 1,
    };
    (
        header_height,
        model.metrics.height.saturating_sub(model.metrics.margin),
        rows,
    )
}

fn draw_feature_header(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    title: &str,
    subtitle: &str,
    accent: Rgb565,
) -> u16 {
    let layout = stage_layout(model.class, model.has_touch);
    let (title_font, subtitle_font, gap) = match model.class {
        UiClass::Tiny128 => (&FONT_7X13_BOLD, &FONT_6X10, 6),
        UiClass::Compact320x170 | UiClass::Portrait240x280 | UiClass::Landscape280x240 => {
            (&FONT_9X15_BOLD, &FONT_6X10, 8)
        }
        UiClass::Medium480x320 => (&FONT_10X20, &FONT_7X13_BOLD, 16),
        UiClass::Large800x480 => (&FONT_10X20, &FONT_7X13_BOLD, 20),
        UiClass::Headless => return 0,
    };
    let back = layout.settings;
    if let Some(back) = back {
        draw_back_button(target, back, accent);
    }
    let left = back.map_or(model.metrics.margin, |rect| {
        rect.right().saturating_add(gap)
    });
    let title_height = u16::try_from(title_font.character_size.height).unwrap_or(0);
    let subtitle_height = u16::try_from(subtitle_font.character_size.height).unwrap_or(0);
    let text_height = title_height
        .saturating_add(4)
        .saturating_add(subtitle_height);
    let title_y = back.map_or(model.metrics.margin, |rect| {
        rect.y
            .saturating_add(rect.height.saturating_sub(text_height) / 2)
    });
    unwrap_infallible(
        Text::with_baseline(
            title,
            Point::new(i32::from(left), i32::from(title_y)),
            MonoTextStyle::new(title_font, TEXT),
            Baseline::Top,
        )
        .draw(target),
    );
    let subtitle_y = title_y.saturating_add(title_height).saturating_add(4);
    unwrap_infallible(
        Text::with_baseline(
            subtitle,
            Point::new(i32::from(left), i32::from(subtitle_y)),
            MonoTextStyle::new(subtitle_font, MUTED),
            Baseline::Top,
        )
        .draw(target),
    );
    let text_bottom = subtitle_y.saturating_add(subtitle_height);
    back.map_or(text_bottom, UiRect::bottom)
        .max(text_bottom)
        .saturating_add(gap)
}

fn draw_list_card(target: &mut Framebuffer<'_>, rect: UiRect, selected: bool, accent: Rgb565) {
    let style = PrimitiveStyleBuilder::new()
        .fill_color(if selected { PANEL_RAISED } else { BACKGROUND })
        .stroke_color(if selected { accent } else { MUTED })
        .stroke_width(if selected { 2 } else { 1 })
        .build();
    unwrap_infallible(
        rounded(
            Point::new(i32::from(rect.x), i32::from(rect.y)),
            Size::new(u32::from(rect.width), u32::from(rect.height)),
            8,
        )
        .into_styled(style)
        .draw(target),
    );
}

fn draw_text_left_centered(
    target: &mut Framebuffer<'_>,
    value: &str,
    rect: UiRect,
    font: &'static MonoFont<'static>,
    colour: Rgb565,
    inset: u16,
) {
    let maximum = usize::try_from(
        u32::from(rect.width.saturating_sub(inset * 2)) / font.character_size.width.max(1),
    )
    .unwrap_or(1);
    let visible = prefix_chars(value, maximum.max(1));
    let text_height = u16::try_from(font.character_size.height).unwrap_or(0);
    unwrap_infallible(
        Text::with_baseline(
            visible,
            Point::new(
                i32::from(rect.x.saturating_add(inset)),
                i32::from(
                    rect.y
                        .saturating_add(rect.height.saturating_sub(text_height) / 2),
                ),
            ),
            MonoTextStyle::new(font, colour),
            Baseline::Top,
        )
        .draw(target),
    );
}

fn draw_text_right_centered(
    target: &mut Framebuffer<'_>,
    value: &str,
    rect: UiRect,
    font: &'static MonoFont<'static>,
    colour: Rgb565,
    inset: u16,
) {
    let text_width = u16::try_from(value.chars().count())
        .unwrap_or(0)
        .saturating_mul(u16::try_from(font.character_size.width).unwrap_or(0));
    let text_height = u16::try_from(font.character_size.height).unwrap_or(0);
    unwrap_infallible(
        Text::with_baseline(
            value,
            Point::new(
                i32::from(
                    rect.right()
                        .saturating_sub(inset)
                        .saturating_sub(text_width),
                ),
                i32::from(
                    rect.y
                        .saturating_add(rect.height.saturating_sub(text_height) / 2),
                ),
            ),
            MonoTextStyle::new(font, colour),
            Baseline::Top,
        )
        .draw(target),
    );
}

#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]
fn draw_tuner(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    margin: i32,
    width: i32,
    height: i32,
) {
    let tuner = model.snapshot.tuner;
    let signal_present = tuner.signal == tonex_ui_model::TunerSignalState::Stable;
    let accent = if signal_present && tuner.cents.abs() <= 3.0 {
        Rgb565::new(7, 55, 18)
    } else {
        Rgb565::new(30, 42, 7)
    };
    let compact = matches!(model.class, UiClass::Tiny128 | UiClass::Compact320x170);
    let subtitle = if tuner.muted {
        "OUTPUT MUTED"
    } else {
        "OUTPUT THRU"
    };
    let content_top = draw_feature_header(target, model, "TUNER", subtitle, accent);
    if tuner.signal == tonex_ui_model::TunerSignalState::Unavailable {
        let rect = UiRect::new(
            model.metrics.margin,
            content_top,
            model.metrics.width - model.metrics.margin * 2,
            model
                .metrics
                .height
                .saturating_sub(content_top + model.metrics.margin),
        );
        draw_text_centered(
            target,
            if model.class == UiClass::Tiny128 {
                "NO TUNER INPUT"
            } else {
                "TUNER INPUT UNAVAILABLE"
            },
            rect,
            if compact {
                &FONT_7X13_BOLD
            } else if model.class == UiClass::Medium480x320 {
                &FONT_10X20
            } else {
                &FONT_9X15_BOLD
            },
            MUTED,
        );
        return;
    }

    let mut note_label = TextBuffer::<8>::default();
    if let Some(note) = tuner.note {
        let octave = i16::from(note / 12) - 1;
        let _ = write!(note_label, "{}{octave}", note_name(note));
    } else {
        let _ = note_label.write_str("--");
    }
    let note_scale = match model.class {
        UiClass::Tiny128 => 3,
        UiClass::Compact320x170 => 4,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => 5,
        UiClass::Medium480x320 => 7,
        UiClass::Large800x480 => 10,
        UiClass::Headless => 1,
    };
    draw_scaled_text_centered(
        target,
        note_label.as_str(),
        Point::new(
            margin,
            height / 2 - i32::try_from(note_scale * 10).unwrap_or(20),
        ),
        u32::try_from((width - margin * 2).max(1)).unwrap_or(1),
        note_scale,
        if signal_present { TEXT } else { MUTED },
    );

    let rail_left = margin + 14;
    let rail_right = width - margin - 14;
    let rail_y = if compact {
        height - margin - 38
    } else {
        height * 2 / 3
    };
    if signal_present {
        let cents = tuner.cents.clamp(-50.0, 50.0).round() as i32;
        let mut cents_label = TextBuffer::<12>::default();
        let _ = write!(cents_label, "{cents:+}c");
        let font = if compact {
            &FONT_7X13_BOLD
        } else {
            &FONT_9X15_BOLD
        };
        let label_width = i32::try_from(
            cents_label.as_str().len() * usize::try_from(font.character_size.width).unwrap_or(9),
        )
        .unwrap_or(0);
        unwrap_infallible(
            Text::with_baseline(
                cents_label.as_str(),
                Point::new((width - label_width) / 2, rail_y + 13),
                MonoTextStyle::new(font, accent),
                Baseline::Top,
            )
            .draw(target),
        );
    }

    if !compact
        && signal_present
        && tuner.frequency_hz.is_finite()
        && tuner.target_hz.is_finite()
        && tuner.frequency_hz > 0.0
        && tuner.target_hz > 0.0
    {
        let mut frequencies = TextBuffer::<32>::default();
        let _ = write!(
            frequencies,
            "{:.1} / {:.1} Hz",
            tuner.frequency_hz, tuner.target_hz
        );
        draw_text_centered(
            target,
            frequencies.as_str(),
            UiRect::new(
                model.metrics.margin,
                u16::try_from((rail_y + 37).min(height.saturating_sub(margin).saturating_sub(32)))
                    .unwrap_or(0),
                model.metrics.width - model.metrics.margin * 2,
                14,
            ),
            &FONT_6X10,
            MUTED,
        );
    }
    unwrap_infallible(
        Rectangle::new(
            Point::new(rail_left, rail_y),
            Size::new(
                u32::try_from((rail_right - rail_left).max(1)).unwrap_or(1),
                3,
            ),
        )
        .into_styled(PrimitiveStyle::with_fill(PANEL_RAISED))
        .draw(target),
    );
    let centre = (rail_left + rail_right) / 2;
    unwrap_infallible(
        Text::with_baseline(
            "b",
            Point::new(rail_left, rail_y - 17),
            MonoTextStyle::new(&FONT_6X10, MUTED),
            Baseline::Top,
        )
        .draw(target),
    );
    unwrap_infallible(
        Text::with_baseline(
            "#",
            Point::new(rail_right - 6, rail_y - 17),
            MonoTextStyle::new(&FONT_6X10, MUTED),
            Baseline::Top,
        )
        .draw(target),
    );
    unwrap_infallible(
        Rectangle::new(Point::new(centre - 1, rail_y - 7), Size::new(3, 17))
            .into_styled(PrimitiveStyle::with_fill(accent))
            .draw(target),
    );
    if signal_present {
        let clamped = tuner.cents.clamp(-50.0, 50.0);
        let span = (rail_right - rail_left) as f32 / 2.0;
        let marker_x = centre + (clamped / 50.0 * span) as i32;
        unwrap_infallible(
            rounded(Point::new(marker_x - 5, rail_y - 5), Size::new(11, 11), 5)
                .into_styled(PrimitiveStyle::with_fill(accent))
                .draw(target),
        );
    }

    let mut reference = TextBuffer::<16>::default();
    let _ = write!(reference, "A = {} Hz", tuner.reference_hz);
    let footer_y = height - margin - 16;
    unwrap_infallible(
        Text::with_baseline(
            reference.as_str(),
            Point::new(margin + 8, footer_y),
            MonoTextStyle::new(&FONT_6X10, MUTED),
            Baseline::Top,
        )
        .draw(target),
    );
    let signal = if signal_present {
        "IN"
    } else if tuner.signal == tonex_ui_model::TunerSignalState::Weak {
        "SIGNAL LOW"
    } else {
        "PLAY A NOTE"
    };
    let signal_width = i32::try_from(signal.len() * 6).unwrap_or(0);
    unwrap_infallible(
        Text::with_baseline(
            signal,
            Point::new(width - margin - 8 - signal_width, footer_y),
            MonoTextStyle::new(
                &FONT_6X10,
                if signal_present || tuner.signal == tonex_ui_model::TunerSignalState::Weak {
                    accent
                } else {
                    MUTED
                },
            ),
            Baseline::Top,
        )
        .draw(target),
    );
}

const fn note_name(note: u8) -> &'static str {
    match note % 12 {
        0 => "C",
        1 => "C#",
        2 => "D",
        3 => "D#",
        4 => "E",
        5 => "F",
        6 => "F#",
        7 => "G",
        8 => "G#",
        9 => "A",
        10 => "A#",
        _ => "B",
    }
}

fn draw_settings_menu(target: &mut Framebuffer<'_>, model: &UiViewModel) {
    let accent = theme_colour(model);
    let _ = draw_subpage_header(target, model, "SETTINGS", accent);
    let show_display = model.snapshot.brightness_dimmable;
    let layout = settings_layout(model.class, show_display);
    let cards = layout.items;

    let mut brightness = TextBuffer::<24>::default();
    let _ = write!(
        brightness,
        "BRIGHTNESS {}%",
        model.snapshot.display_brightness_percent
    );
    let board = core::str::from_utf8(model.snapshot.board_label.as_bytes()).unwrap_or("DEVICE");
    let tuner = if model.snapshot.tuner.signal == tonex_ui_model::TunerSignalState::Unavailable {
        "NOT AVAILABLE"
    } else {
        "READY"
    };
    let connect_title = if matches!(
        model.class,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 | UiClass::Tiny128
    ) {
        "CONNECT"
    } else {
        "QUICK CONNECT"
    };
    let display_detail = if matches!(
        model.class,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 | UiClass::Tiny128
    ) {
        brightness
            .as_str()
            .strip_prefix("BRIGHTNESS ")
            .unwrap_or(brightness.as_str())
    } else {
        brightness.as_str()
    };
    let device_detail = if board.is_empty() {
        "BOARD + SYSTEM"
    } else {
        board
    };
    if show_display {
        let content = [
            (connect_title, "WI-FI + WEB"),
            ("DISPLAY", display_detail),
            ("DEVICE", device_detail),
            ("TUNER", tuner),
        ];
        for (rect, (title, detail)) in cards.into_iter().zip(content) {
            draw_settings_card(target, model.class, rect, title, detail, accent);
        }
    } else {
        let content = [
            (connect_title, "WI-FI + WEB"),
            ("DEVICE", device_detail),
            ("TUNER", tuner),
        ];
        for (rect, (title, detail)) in cards.into_iter().take(3).zip(content) {
            draw_settings_card(target, model.class, rect, title, detail, accent);
        }
    }
}

fn draw_settings_card(
    target: &mut Framebuffer<'_>,
    class: UiClass,
    rect: UiRect,
    title: &str,
    detail: &str,
    accent: Rgb565,
) {
    let radius = match class {
        UiClass::Tiny128 | UiClass::Compact320x170 => 6,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => 8,
        UiClass::Medium480x320 => 10,
        UiClass::Large800x480 => 14,
        UiClass::Headless => 0,
    };
    let style = PrimitiveStyleBuilder::new()
        .fill_color(PANEL)
        .stroke_color(MUTED)
        .stroke_width(1)
        .build();
    unwrap_infallible(
        rounded(
            Point::new(i32::from(rect.x), i32::from(rect.y)),
            Size::new(u32::from(rect.width), u32::from(rect.height)),
            radius,
        )
        .into_styled(style)
        .draw(target),
    );

    let inset = match class {
        UiClass::Tiny128 => 7,
        UiClass::Compact320x170 => 10,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => 11,
        UiClass::Medium480x320 => 16,
        UiClass::Large800x480 => 22,
        UiClass::Headless => 0,
    };
    let rail_width = if matches!(class, UiClass::Medium480x320 | UiClass::Large800x480) {
        4
    } else {
        3
    };
    let rail_height = rect.height.saturating_sub(inset * 2).min(30);
    let rail = Rectangle::new(
        Point::new(
            i32::from(rect.x.saturating_add(inset)),
            i32::from(rect.y.saturating_add(inset)),
        ),
        Size::new(u32::from(rail_width), u32::from(rail_height)),
    );
    unwrap_infallible(
        rail.into_styled(PrimitiveStyle::with_fill(accent))
            .draw(target),
    );

    let text_x = inset.saturating_add(rail_width).saturating_add(8);
    let title_height = if matches!(class, UiClass::Tiny128 | UiClass::Compact320x170) {
        rect.height / 2
    } else {
        rect.height * 3 / 5
    };
    let title_rect = UiRect::new(rect.x, rect.y, rect.width, title_height);
    draw_text_left_centered(
        target,
        title,
        title_rect,
        if matches!(class, UiClass::Tiny128 | UiClass::Compact320x170) {
            &FONT_7X13_BOLD
        } else {
            &FONT_9X15_BOLD
        },
        TEXT,
        text_x,
    );
    let detail_rect = UiRect::new(
        rect.x,
        rect.y.saturating_add(title_height),
        rect.width,
        rect.height.saturating_sub(title_height),
    );
    draw_text_left_centered(target, detail, detail_rect, &FONT_6X10, MUTED, text_x);
    draw_text_right_centered(target, ">", rect, &FONT_9X15_BOLD, accent, inset);
}

fn draw_display_info(target: &mut Framebuffer<'_>, model: &UiViewModel) {
    let accent = theme_colour(model);
    let (top, bottom, _) = draw_subpage_header(target, model, "DISPLAY", accent);
    let width = model.metrics.width - model.metrics.margin * 2;
    let rect = UiRect::new(model.metrics.margin, top, width, (bottom - top) / 2);
    draw_list_card(target, rect, true, accent);
    draw_text_left_centered(target, "BRIGHTNESS", rect, &FONT_7X13_BOLD, TEXT, 12);
    let mut value = TextBuffer::<8>::default();
    let _ = write!(value, "{}%", model.snapshot.display_brightness_percent);
    draw_text_right_centered(target, value.as_str(), rect, &FONT_9X15_BOLD, accent, 12);
    let note = UiRect::new(
        model.metrics.margin,
        rect.y + rect.height + 6,
        width,
        bottom.saturating_sub(rect.y + rect.height + 6),
    );
    draw_text_centered(
        target,
        if model.snapshot.brightness_dimmable {
            "TAP LEFT / RIGHT  -10 / +10"
        } else {
            "FIXED BACKLIGHT - NO DIMMER"
        },
        note,
        &FONT_6X10,
        MUTED,
    );
}

fn draw_device_info(target: &mut Framebuffer<'_>, model: &UiViewModel) {
    let accent = theme_colour(model);
    let top = draw_feature_header(target, model, "DEVICE", "BOARD + CONNECTION", accent);
    let bottom = model.metrics.height.saturating_sub(model.metrics.margin);
    let labels = ["BOARD", "DISPLAY", "TOUCH", "TONEX ONE"];
    let mut board = TextBuffer::<32>::default();
    for byte in model.snapshot.board_label.as_bytes() {
        let _ = board.write_char(char::from(byte.to_ascii_uppercase()));
    }
    if board.as_str().is_empty() {
        let _ = board.write_str("UNKNOWN");
    }
    let mut dimensions = TextBuffer::<16>::default();
    let _ = write!(
        dimensions,
        "{}x{}",
        model.snapshot.display_width, model.snapshot.display_height
    );
    let values = [
        board.as_str(),
        dimensions.as_str(),
        if model.snapshot.touch_ready {
            "READY"
        } else {
            "UNAVAILABLE"
        },
        match model.snapshot.connection {
            tonex_domain::ConnectionState::Disconnected => "DISCONNECTED",
            tonex_domain::ConnectionState::Connecting => "CONNECTING",
            tonex_domain::ConnectionState::Synchronizing => "SYNCING",
            tonex_domain::ConnectionState::Ready => "READY",
            tonex_domain::ConnectionState::Faulted => "FAULT",
        },
    ];
    let gap = match model.class {
        UiClass::Tiny128 => 3,
        UiClass::Compact320x170 => 6,
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => 7,
        UiClass::Medium480x320 => 8,
        UiClass::Large800x480 => 12,
        UiClass::Headless => 0,
    };
    let available_width = model.metrics.width.saturating_sub(model.metrics.margin * 2);
    let card_width = available_width.saturating_sub(gap) / 2;
    let card_height = bottom.saturating_sub(top).saturating_sub(gap) / 2;
    let right = model.metrics.margin + card_width + gap;
    let lower = top + card_height + gap;
    let cards = [
        UiRect::new(model.metrics.margin, top, card_width, card_height),
        UiRect::new(right, top, card_width, card_height),
        UiRect::new(model.metrics.margin, lower, card_width, card_height),
        UiRect::new(right, lower, card_width, card_height),
    ];
    for (index, ((label, value), rect)) in labels.into_iter().zip(values).zip(cards).enumerate() {
        let value_colour = if index == 2 {
            if model.snapshot.touch_ready {
                CONNECTED
            } else {
                DISCONNECTED
            }
        } else if index == 3 {
            if model.snapshot.connection == tonex_domain::ConnectionState::Ready {
                CONNECTED
            } else {
                DISCONNECTED
            }
        } else {
            TEXT
        };
        draw_device_card(
            target,
            model.class,
            rect,
            label,
            value,
            value_colour,
            accent,
        );
    }
}

fn draw_device_card(
    target: &mut Framebuffer<'_>,
    class: UiClass,
    rect: UiRect,
    label: &str,
    value: &str,
    value_colour: Rgb565,
    accent: Rgb565,
) {
    let radius = if matches!(class, UiClass::Tiny128 | UiClass::Compact320x170) {
        6
    } else {
        9
    };
    let style = PrimitiveStyleBuilder::new()
        .fill_color(PANEL)
        .stroke_color(MUTED)
        .stroke_width(1)
        .build();
    unwrap_infallible(
        rounded(
            Point::new(i32::from(rect.x), i32::from(rect.y)),
            Size::new(u32::from(rect.width), u32::from(rect.height)),
            radius,
        )
        .into_styled(style)
        .draw(target),
    );
    let inset = if rect.width >= 180 { 16 } else { 10 };
    unwrap_infallible(
        Rectangle::new(
            Point::new(i32::from(rect.x + inset), i32::from(rect.y + inset)),
            Size::new(rect.width.saturating_sub(inset * 2).min(30).into(), 3),
        )
        .into_styled(PrimitiveStyle::with_fill(accent))
        .draw(target),
    );
    let label_rect = UiRect::new(rect.x, rect.y + 6, rect.width, rect.height / 2);
    let value_rect = UiRect::new(
        rect.x,
        rect.y + rect.height / 2,
        rect.width,
        rect.height - rect.height / 2,
    );
    draw_text_left_centered(
        target,
        label,
        label_rect,
        if matches!(class, UiClass::Medium480x320 | UiClass::Large800x480) {
            &FONT_7X13_BOLD
        } else {
            &FONT_6X10
        },
        MUTED,
        inset,
    );
    draw_text_left_centered(
        target,
        value,
        value_rect,
        if matches!(class, UiClass::Tiny128 | UiClass::Compact320x170) {
            &FONT_7X13_BOLD
        } else {
            &FONT_9X15_BOLD
        },
        value_colour,
        inset,
    );
}

fn draw_quick_connect(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    margin: i32,
    width: i32,
    height: i32,
) {
    let accent = theme_colour(model);
    let tiny = model.class == UiClass::Tiny128;
    let content_top = i32::from(draw_feature_header(
        target,
        model,
        if tiny { "CONNECT" } else { "QUICK CONNECT" },
        "WI-FI + WEB ACCESS",
        accent,
    ));
    let bottom = height - margin;
    let gap = if tiny { 6 } else { 16 };
    let horizontal_inset = margin;
    let maximum_by_width = ((width - horizontal_inset * 2 - gap) / 2).max(38);
    let label_height = if tiny { 10 } else { 13 };
    let details_height = if matches!(model.class, UiClass::Tiny128 | UiClass::Compact320x170) {
        0
    } else {
        15
    };
    let qr_top = content_top + label_height + 5;
    let maximum_by_height = (bottom - qr_top - details_height).max(38);
    let box_size = u32::try_from(maximum_by_width.min(maximum_by_height)).unwrap_or(38);
    let pair_width = i32::try_from(box_size.saturating_mul(2)).unwrap_or(76) + gap;
    let first_left = ((width - pair_width) / 2).max(horizontal_inset);
    let second_left = first_left + i32::try_from(box_size).unwrap_or(38) + gap;
    draw_centered_label(
        target,
        "WI-FI",
        first_left,
        content_top,
        i32::try_from(box_size).unwrap_or(38),
        accent,
    );
    draw_centered_label(
        target,
        "OPEN WEB",
        second_left,
        content_top,
        i32::try_from(box_size).unwrap_or(38),
        accent,
    );
    draw_wifi_qr(target, model, Point::new(first_left, qr_top), box_size);
    draw_qr_payload(
        target,
        "http://tonexone.test/",
        Point::new(second_left, qr_top),
        box_size,
    );
    if !tiny {
        let details_top = qr_top + i32::try_from(box_size).unwrap_or(38) + 4;
        if details_top + 10 <= bottom {
            draw_centered_label(
                target,
                model.snapshot.wifi.ssid.as_str(),
                first_left,
                details_top,
                i32::try_from(box_size).unwrap_or(38),
                MUTED,
            );
            draw_centered_label(
                target,
                "tonexone.test",
                second_left,
                details_top,
                i32::try_from(box_size).unwrap_or(38),
                MUTED,
            );
        }
    }
}

fn draw_wifi_qr(target: &mut Framebuffer<'_>, model: &UiViewModel, origin: Point, box_size: u32) {
    let mut payload = TextBuffer::<160>::default();
    let _ = payload.write_str("WIFI:T:WPA;S:");
    write_wifi_qr_field(&mut payload, model.snapshot.wifi.ssid.as_str());
    let _ = payload.write_str(";P:");
    write_wifi_qr_field(&mut payload, model.snapshot.wifi.password.as_str());
    let _ = payload.write_str(";;");
    draw_qr_payload(target, payload.as_str(), origin, box_size);
}

fn draw_qr_payload(target: &mut Framebuffer<'_>, payload: &str, origin: Point, box_size: u32) {
    unwrap_infallible(
        Rectangle::new(origin, Size::new(box_size, box_size))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(target),
    );
    let Ok(code) = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::L) else {
        return;
    };
    let modules = u32::try_from(code.width()).unwrap_or(0);
    let total_modules = modules.saturating_add(8);
    let scale = if total_modules == 0 {
        0
    } else {
        box_size / total_modules
    };
    if scale == 0 {
        return;
    }
    let rendered = total_modules * scale;
    let inset = (box_size - rendered) / 2 + 4 * scale;
    for y in 0..modules {
        for x in 0..modules {
            if code[(
                usize::try_from(x).unwrap_or(0),
                usize::try_from(y).unwrap_or(0),
            )] != QrColor::Dark
            {
                continue;
            }
            unwrap_infallible(
                Rectangle::new(
                    Point::new(
                        origin.x + i32::try_from(inset + x * scale).unwrap_or(0),
                        origin.y + i32::try_from(inset + y * scale).unwrap_or(0),
                    ),
                    Size::new(scale, scale),
                )
                .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                .draw(target),
            );
        }
    }
}

fn draw_centered_label(
    target: &mut Framebuffer<'_>,
    label: &str,
    left: i32,
    top: i32,
    width: i32,
    colour: Rgb565,
) {
    let label_width = i32::try_from(label.chars().count() * 7).unwrap_or(0);
    unwrap_infallible(
        Text::with_baseline(
            label,
            Point::new(left + (width - label_width) / 2, top),
            MonoTextStyle::new(&FONT_7X13_BOLD, colour),
            Baseline::Top,
        )
        .draw(target),
    );
}

fn write_wifi_qr_field<const CAPACITY: usize>(output: &mut TextBuffer<CAPACITY>, value: &str) {
    for character in value.chars() {
        if matches!(character, '\\' | ';' | ',' | ':' | '"') {
            let _ = output.write_char('\\');
        }
        let _ = output.write_char(character);
    }
}

fn draw_scaled_text_centered(
    target: &mut Framebuffer<'_>,
    value: &str,
    origin: Point,
    available_width: u32,
    scale: u32,
    colour: Rgb565,
) {
    let scale = scale.max(1);
    let character_width = FONT_10X20.character_size.width * scale;
    let text_width = u32::try_from(value.chars().count())
        .unwrap_or(0)
        .saturating_mul(character_width);
    let offset = available_width.saturating_sub(text_width) / 2;
    let logical_size = Size::new(available_width / scale, FONT_10X20.character_size.height);
    let mut scaled = ScaledTarget {
        target,
        origin: origin + Size::new(offset, 0),
        scale,
        logical_size,
    };
    unwrap_infallible(
        Text::with_baseline(
            value,
            Point::zero(),
            MonoTextStyle::new(&FONT_10X20, colour),
            Baseline::Top,
        )
        .draw(&mut scaled),
    );
}

fn draw_content(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    _margin: i32,
    _width: i32,
    _height: i32,
) {
    let accent = theme_colour(model);
    let layout = stage_layout(model.class, model.has_touch);
    draw_open_header(target, model, &layout, accent);
    draw_signal_chain(target, model, &layout, accent);
    draw_skin(target, model, &layout);
    draw_mute_overlay(target, model, &layout);
    draw_performance_footer(target, model, &layout, accent);
}

fn draw_mute_overlay(target: &mut Framebuffer<'_>, model: &UiViewModel, layout: &StageLayout) {
    if !model.snapshot.mute.is_active() {
        return;
    }
    let Some(rect) = layout.skin else {
        return;
    };
    let style = PrimitiveStyleBuilder::new()
        .fill_color(MUTE_PANEL)
        .stroke_color(MUTE_RED)
        .stroke_width(3)
        .build();
    unwrap_infallible(
        rounded(
            Point::new(i32::from(rect.x), i32::from(rect.y)),
            Size::new(u32::from(rect.width), u32::from(rect.height)),
            10,
        )
        .into_styled(style)
        .draw(target),
    );
    let (font, scale) = match model.class {
        UiClass::Medium480x320 => (&FONT_10X20, 2),
        UiClass::Large800x480 => (&FONT_10X20, 3),
        _ => (&FONT_9X15_BOLD, 1),
    };
    draw_scaled_font_centered(target, "MUTED", rect, font, scale, MUTE_RED);
}

fn draw_skin(target: &mut Framebuffer<'_>, model: &UiViewModel, layout: &StageLayout) {
    let Some(rect) = layout.skin else {
        return;
    };
    let framebuffer_width = target.size.width as usize;
    let mut source_row = [0_u16; SKIN_SOURCE_WIDTH];
    let mut decoded_source_y = None;
    for target_y in 0..usize::from(rect.height) {
        let source_y = target_y * SKIN_SOURCE_HEIGHT / usize::from(rect.height);
        if decoded_source_y != Some(source_y) {
            if !decode_skin_row(model.snapshot.skin.id, source_y, &mut source_row) {
                return;
            }
            decoded_source_y = Some(source_y);
        }
        let framebuffer_y = usize::from(rect.y) + target_y;
        if framebuffer_y >= target.size.height as usize {
            break;
        }
        for target_x in 0..usize::from(rect.width) {
            let source_x = target_x * SKIN_SOURCE_WIDTH / usize::from(rect.width);
            let framebuffer_x = usize::from(rect.x) + target_x;
            if framebuffer_x >= framebuffer_width {
                break;
            }
            target.pixels[framebuffer_y * framebuffer_width + framebuffer_x] = source_row[source_x];
        }
    }
}

fn draw_open_header(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    layout: &StageLayout,
    accent: Rgb565,
) {
    let mut number_buffer = [0_u8; 3];
    let number = decimal(model.snapshot.preset.get() + 1, &mut number_buffer);
    let label = core::str::from_utf8(model.snapshot.preset_label.as_bytes()).unwrap_or("Preset");
    let (header_font, name_scale, group_font, group_scale) = match model.class {
        UiClass::Tiny128 => (&FONT_6X10, 1, &FONT_6X10, 1),
        UiClass::Compact320x170 => (&FONT_7X13_BOLD, 1, &FONT_7X13_BOLD, 1),
        UiClass::Portrait240x280 | UiClass::Landscape280x240 => {
            (&FONT_7X13_BOLD, 1, &FONT_7X13_BOLD, 1)
        }
        UiClass::Medium480x320 => (&FONT_7X13_BOLD, 2, &FONT_7X13_BOLD, 2),
        UiClass::Large800x480 => (&FONT_10X20, 2, &FONT_10X20, 2),
        UiClass::Headless => return,
    };
    draw_scaled_font_centered(
        target,
        label,
        layout.preset_name,
        header_font,
        name_scale,
        TEXT,
    );
    let mut group_label = TextBuffer::<12>::default();
    let _ = write!(group_label, "{number}-{}", slot_label(model.snapshot.slot));
    let group_rect = UiRect::new(
        layout.preset_number.x,
        layout.preset_number.y,
        layout.slot.right() - layout.preset_number.x,
        layout.preset_number.height,
    );
    draw_scaled_font_centered(
        target,
        group_label.as_str(),
        group_rect,
        group_font,
        group_scale,
        accent,
    );
    if let Some(settings) = layout.settings {
        draw_gear_icon(target, settings, accent);
    }

    if !matches!(model.status, StatusText::Ready) && model.class != UiClass::Tiny128 {
        let status = match model.status {
            StatusText::Disconnected => "OFFLINE",
            StatusText::Connecting => "CONNECTING",
            StatusText::Synchronizing { .. } => "SYNCING",
            StatusText::Faulted => "FAULT",
            StatusText::Ready => "",
        };
        let status_rect = UiRect::new(
            layout.preset_name.x,
            layout.preset_name.bottom().saturating_sub(10),
            layout.preset_name.width,
            10,
        );
        draw_text_centered(target, status, status_rect, &FONT_6X10, MUTED);
    }
}

fn draw_signal_chain(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    layout: &StageLayout,
    accent: Rgb565,
) {
    let enabled = [
        model.snapshot.fx.gate,
        model.snapshot.fx.compressor,
        model.snapshot.fx.amp,
        model.snapshot.fx.cabinet,
        model.snapshot.fx.modulation,
        model.snapshot.fx.delay,
        model.snapshot.fx.reverb,
    ];
    let labels = match model.class {
        UiClass::Tiny128 => ["G", "C", "A", "C", "M", "D", "R"],
        _ => ["GATE", "COMP", "AMP", "CAB", "MOD", "DLY", "REV"],
    };
    let online = matches!(model.status, StatusText::Ready);
    let line_colour = if online { accent } else { MUTED };

    for pair in layout.signal_chain.windows(2) {
        let [first, last] = pair else {
            continue;
        };
        if first.y != last.y {
            continue;
        }
        let y = i32::from(first.y + first.height / 2);
        let start = Point::new(i32::from(first.x + first.width / 2), y);
        let width = u32::from(last.x + last.width / 2 - first.x - first.width / 2 + 1);
        unwrap_infallible(
            Rectangle::new(start, Size::new(width, 2))
                .into_styled(PrimitiveStyle::with_fill(line_colour))
                .draw(target),
        );
    }

    for (index, rect) in layout.signal_chain.iter().copied().enumerate() {
        let is_on = online && enabled[index];
        let fill = if is_on { PANEL_RAISED } else { BACKGROUND };
        let text = if is_on { TEXT } else { MUTED };
        let radius = if rect.height >= 40 { 10 } else { 5 };
        let style = PrimitiveStyleBuilder::new()
            .fill_color(fill)
            .stroke_color(if is_on { accent } else { MUTED })
            .stroke_width(if is_on { 2 } else { 1 })
            .build();
        unwrap_infallible(
            rounded(
                Point::new(i32::from(rect.x), i32::from(rect.y)),
                Size::new(u32::from(rect.width), u32::from(rect.height)),
                radius,
            )
            .into_styled(style)
            .draw(target),
        );
        if is_on {
            let inset = if rect.width >= 48 { 9 } else { 5 };
            let line_width = rect.width.saturating_sub(inset * 2);
            unwrap_infallible(
                rounded(
                    Point::new(i32::from(rect.x + inset), i32::from(rect.y + 7)),
                    Size::new(u32::from(line_width), 3),
                    2,
                )
                .into_styled(PrimitiveStyle::with_fill(accent))
                .draw(target),
            );
            if rect.width >= 48 && rect.height >= 42 {
                unwrap_infallible(
                    Circle::new(
                        Point::new(
                            i32::from(rect.right().saturating_sub(13)),
                            i32::from(rect.bottom().saturating_sub(13)),
                        ),
                        5,
                    )
                    .into_styled(PrimitiveStyle::with_fill(accent))
                    .draw(target),
                );
            }
        }
        let font = match model.class {
            UiClass::Tiny128 | UiClass::Compact320x170 => &FONT_6X10,
            UiClass::Portrait240x280 | UiClass::Landscape280x240 => &FONT_7X13_BOLD,
            UiClass::Medium480x320 | UiClass::Large800x480 => &FONT_9X18_BOLD,
            UiClass::Headless => return,
        };
        draw_text_centered(target, labels[index], rect, font, text);
    }
}

fn draw_performance_footer(
    target: &mut Framebuffer<'_>,
    model: &UiViewModel,
    layout: &StageLayout,
    accent: Rgb565,
) {
    let online = matches!(model.status, StatusText::Ready);
    let mut volume = TextBuffer::<24>::default();
    let mut bpm = TextBuffer::<16>::default();
    if online {
        let _ = write!(volume, "{:.1}", model.snapshot.preset_volume);
        let _ = write!(bpm, "{:.0}", model.snapshot.bpm);
    } else {
        let _ = volume.write_str("-- dB");
        let _ = bpm.write_str("--");
    }
    draw_metric(
        target,
        layout.master,
        volume.as_str(),
        "PRESET VOL",
        model.class,
        TEXT,
    );
    draw_metric(target, layout.bpm, bpm.as_str(), "BPM", model.class, accent);
    draw_connection_status(target, model, layout);
}

fn draw_connection_status(target: &mut Framebuffer<'_>, model: &UiViewModel, layout: &StageLayout) {
    if model.class == UiClass::Tiny128 {
        return;
    }
    let gap = if model.class == UiClass::Large800x480 {
        24
    } else {
        12
    };
    let left = layout.bpm.right().saturating_add(gap);
    let right = model.metrics.width.saturating_sub(model.metrics.margin);
    if right <= left {
        return;
    }
    let rect = UiRect::new(left, layout.bpm.y, right - left, layout.bpm.height);
    let cell_gap = if model.class == UiClass::Large800x480 {
        18
    } else {
        12
    };
    let tonex_width = connection_badge_width("TONEX ONE", model.class);
    let bluetooth_width = connection_badge_width("BLUETOOTH", model.class);
    let group_width = tonex_width
        .saturating_add(cell_gap)
        .saturating_add(bluetooth_width);
    // Connection state is secondary stage information. Keep the pair grouped
    // at the far-right edge instead of floating in the middle of the footer.
    let group_left = rect.right().saturating_sub(group_width);
    let tonex_rect = UiRect::new(group_left, rect.y, tonex_width, rect.height);
    let bluetooth_rect = UiRect::new(
        group_left
            .saturating_add(tonex_width)
            .saturating_add(cell_gap),
        rect.y,
        bluetooth_width,
        rect.height,
    );
    draw_connection_badge(
        target,
        tonex_rect,
        "TONEX ONE",
        if matches!(model.status, StatusText::Ready) {
            CONNECTED
        } else {
            DISCONNECTED
        },
        model.class,
    );
    let bluetooth_colour = match model.snapshot.bluetooth {
        BluetoothState::Connected => BLUETOOTH_CONNECTED,
        BluetoothState::Searching | BluetoothState::Off => BLUETOOTH_IDLE,
    };
    draw_connection_badge(
        target,
        bluetooth_rect,
        "BLUETOOTH",
        bluetooth_colour,
        model.class,
    );
}

fn connection_badge_width(label: &str, class: UiClass) -> u16 {
    let font = if class == UiClass::Large800x480 {
        &FONT_9X15_BOLD
    } else {
        &FONT_6X10
    };
    let dot_size: u16 = if class == UiClass::Large800x480 {
        10
    } else {
        7
    };
    dot_size.saturating_add(7).saturating_add(
        u16::try_from(label.chars().count())
            .unwrap_or(0)
            .saturating_mul(u16::try_from(font.character_size.width).unwrap_or(0)),
    )
}

fn draw_connection_badge(
    target: &mut Framebuffer<'_>,
    rect: UiRect,
    label: &str,
    colour: Rgb565,
    class: UiClass,
) {
    let font = if class == UiClass::Large800x480 {
        &FONT_9X15_BOLD
    } else {
        &FONT_6X10
    };
    let dot_size: u16 = if class == UiClass::Large800x480 {
        10
    } else {
        7
    };
    let text_width = u16::try_from(label.chars().count())
        .unwrap_or(0)
        .saturating_mul(u16::try_from(font.character_size.width).unwrap_or(0));
    let group_width = dot_size.saturating_add(7).saturating_add(text_width);
    let group_left = rect
        .x
        .saturating_add(rect.width.saturating_sub(group_width));
    let dot_y = rect
        .y
        .saturating_add(rect.height.saturating_sub(dot_size) / 2);
    unwrap_infallible(
        Circle::new(
            Point::new(i32::from(group_left), i32::from(dot_y)),
            u32::from(dot_size),
        )
        .into_styled(PrimitiveStyle::with_fill(colour))
        .draw(target),
    );
    let text_rect = UiRect::new(
        group_left.saturating_add(dot_size + 7),
        rect.y,
        text_width,
        rect.height,
    );
    draw_text_centered(target, label, text_rect, font, colour);
}

fn draw_metric(
    target: &mut Framebuffer<'_>,
    rect: UiRect,
    value: &str,
    label: &str,
    class: UiClass,
    colour: Rgb565,
) {
    if class == UiClass::Tiny128 {
        let label_height = u16::try_from(FONT_6X10.character_size.height).unwrap_or(u16::MAX);
        let value_rect = UiRect::new(
            rect.x,
            rect.y,
            rect.width,
            rect.height.saturating_sub(label_height + 3),
        );
        let label_rect = UiRect::new(
            rect.x,
            rect.bottom().saturating_sub(label_height),
            rect.width,
            label_height,
        );
        draw_scaled_font_centered(target, value, value_rect, &FONT_7X13_BOLD, 1, colour);
        draw_text_centered(target, label, label_rect, &FONT_6X10, MUTED);
        return;
    }

    let (font, scale) = match class {
        UiClass::Compact320x170 | UiClass::Portrait240x280 | UiClass::Landscape280x240 => {
            (&FONT_6X10, 1)
        }
        UiClass::Medium480x320 => (&FONT_7X13_BOLD, 1),
        UiClass::Large800x480 => (&FONT_9X15_BOLD, 1),
        UiClass::Tiny128 | UiClass::Headless => return,
    };
    let compact_master = label == "PRESET VOL";
    let compact_label = if compact_master { "VOL" } else { label };
    let display_value = if rect.width < 90 && compact_master {
        value.strip_suffix(" dB").unwrap_or(value)
    } else {
        value
    };
    let mut line = TextBuffer::<32>::default();
    let _ = write!(line, "{compact_label}  {display_value}");
    draw_scaled_font_centered(target, line.as_str(), rect, font, scale, colour);
}

fn draw_back_button(target: &mut Framebuffer<'_>, rect: UiRect, accent: Rgb565) {
    let style = PrimitiveStyleBuilder::new()
        .fill_color(BACKGROUND)
        .stroke_color(MUTED)
        .stroke_width(1)
        .build();
    unwrap_infallible(
        rounded(
            Point::new(i32::from(rect.x), i32::from(rect.y)),
            Size::new(u32::from(rect.width), u32::from(rect.height)),
            11,
        )
        .into_styled(style)
        .draw(target),
    );

    let centre_x = i32::from(rect.x) + i32::from(rect.width) / 2;
    let centre_y = i32::from(rect.y) + i32::from(rect.height) / 2;
    let arm = i32::from(rect.height.clamp(24, 52)) / 7;
    let offset = (arm / 2).max(2);
    let chevron = PrimitiveStyle::with_stroke(accent, 2);
    unwrap_infallible(
        Line::new(
            Point::new(centre_x + offset, centre_y - arm),
            Point::new(centre_x - offset, centre_y),
        )
        .into_styled(chevron)
        .draw(target),
    );
    unwrap_infallible(
        Line::new(
            Point::new(centre_x - offset, centre_y),
            Point::new(centre_x + offset, centre_y + arm),
        )
        .into_styled(chevron)
        .draw(target),
    );
}

fn draw_gear_icon(target: &mut Framebuffer<'_>, rect: UiRect, accent: Rgb565) {
    let diameter = rect
        .width
        .min(rect.height)
        .saturating_mul(52)
        .checked_div(100)
        .unwrap_or(1)
        .min(27);
    let left = rect.x + (rect.width - diameter) / 2;
    let top = rect.y + (rect.height - diameter) / 2;
    let tooth = 5_u16;
    let centre_x = left + diameter / 2;
    let centre_y = top + diameter / 2;
    let fill = PrimitiveStyle::with_fill(accent);
    for (x, y, width, height) in [
        (centre_x - tooth / 2, top, tooth, 7),
        (centre_x - tooth / 2, top + diameter - 7, tooth, 7),
        (left, centre_y - tooth / 2, 7, tooth),
        (left + diameter - 7, centre_y - tooth / 2, 7, tooth),
    ] {
        unwrap_infallible(
            Rectangle::new(
                Point::new(i32::from(x), i32::from(y)),
                Size::new(u32::from(width), u32::from(height)),
            )
            .into_styled(fill)
            .draw(target),
        );
    }
    for (x, y) in [
        (left + 3, top + 3),
        (left + diameter - 8, top + 3),
        (left + 3, top + diameter - 8),
        (left + diameter - 8, top + diameter - 8),
    ] {
        unwrap_infallible(
            Rectangle::new(Point::new(i32::from(x), i32::from(y)), Size::new(5, 5))
                .into_styled(fill)
                .draw(target),
        );
    }
    let outer_style = PrimitiveStyleBuilder::new()
        .fill_color(PANEL)
        .stroke_color(accent)
        .stroke_width(3)
        .build();
    unwrap_infallible(
        Circle::new(
            Point::new(i32::from(left + 4), i32::from(top + 4)),
            u32::from(diameter.saturating_sub(8)),
        )
        .into_styled(outer_style)
        .draw(target),
    );
    unwrap_infallible(
        Circle::new(
            Point::new(i32::from(centre_x - 4), i32::from(centre_y - 4)),
            8,
        )
        .into_styled(PrimitiveStyle::with_fill(accent))
        .draw(target),
    );
    unwrap_infallible(
        Circle::new(
            Point::new(i32::from(centre_x - 2), i32::from(centre_y - 2)),
            4,
        )
        .into_styled(PrimitiveStyle::with_fill(PANEL))
        .draw(target),
    );
}

fn draw_text_centered(
    target: &mut Framebuffer<'_>,
    value: &str,
    rect: UiRect,
    font: &'static MonoFont<'static>,
    colour: Rgb565,
) {
    let maximum = u32::from(rect.width) / font.character_size.width.max(1);
    let visible = prefix_chars(value, usize::try_from(maximum).unwrap_or(1).max(1));
    let text_width = i32::try_from(
        u32::try_from(visible.chars().count()).unwrap_or(0) * font.character_size.width,
    )
    .unwrap_or(0);
    let x = i32::from(rect.x) + (i32::from(rect.width) - text_width) / 2;
    let y = i32::from(rect.y)
        + (i32::from(rect.height) - i32::try_from(font.character_size.height).unwrap_or(10)) / 2;
    unwrap_infallible(
        Text::with_baseline(
            visible,
            Point::new(x, y),
            MonoTextStyle::new(font, colour),
            Baseline::Top,
        )
        .draw(target),
    );
}

fn draw_scaled_font_centered(
    target: &mut Framebuffer<'_>,
    value: &str,
    rect: UiRect,
    font: &'static MonoFont<'static>,
    preferred_scale: u32,
    colour: Rgb565,
) {
    let count = u32::try_from(value.chars().count()).unwrap_or(u32::MAX);
    let scale = preferred_scale
        .min(
            u32::from(rect.width)
                .checked_div(count.saturating_mul(font.character_size.width).max(1))
                .unwrap_or(1),
        )
        .max(1);
    let maximum = u32::from(rect.width)
        .checked_div(font.character_size.width.saturating_mul(scale).max(1))
        .unwrap_or(1);
    let visible = prefix_chars(value, usize::try_from(maximum).unwrap_or(1).max(1));
    let text_width = u32::try_from(visible.chars().count())
        .unwrap_or(0)
        .saturating_mul(font.character_size.width)
        .saturating_mul(scale);
    let text_height = font.character_size.height.saturating_mul(scale);
    let origin = Point::new(
        i32::from(rect.x) + i32::try_from((u32::from(rect.width) - text_width) / 2).unwrap_or(0),
        i32::from(rect.y) + i32::try_from((u32::from(rect.height) - text_height) / 2).unwrap_or(0),
    );
    let mut scaled = ScaledTarget {
        target,
        origin,
        scale,
        logical_size: Size::new(
            u32::from(rect.width) / scale,
            u32::from(rect.height) / scale,
        ),
    };
    unwrap_infallible(
        Text::with_baseline(
            visible,
            Point::zero(),
            MonoTextStyle::new(font, colour),
            Baseline::Top,
        )
        .draw(&mut scaled),
    );
}

fn prefix_chars(value: &str, maximum: usize) -> &str {
    value
        .char_indices()
        .nth(maximum)
        .map_or(value, |(index, _)| &value[..index])
}

fn rounded(top_left: Point, size: Size, radius: u32) -> RoundedRectangle {
    RoundedRectangle::with_equal_corners(Rectangle::new(top_left, size), Size::new(radius, radius))
}

fn theme_colour(model: &UiViewModel) -> Rgb565 {
    model.snapshot.preset_color.map_or_else(
        || profile_colour(model.tone_profile),
        |color| Rgb565::new(color.red >> 3, color.green >> 2, color.blue >> 3),
    )
}

const fn profile_colour(profile: ToneProfile) -> Rgb565 {
    match profile {
        ToneProfile::Clean => Rgb565::new(8, 47, 29),
        ToneProfile::British => Rgb565::new(29, 35, 8),
        ToneProfile::American => Rgb565::new(8, 36, 27),
        ToneProfile::Modern => Rgb565::new(31, 17, 8),
        ToneProfile::Boutique => Rgb565::new(25, 18, 24),
        ToneProfile::Bass => Rgb565::new(10, 25, 28),
        ToneProfile::Acoustic => Rgb565::new(24, 31, 13),
        ToneProfile::Pedal => Rgb565::new(27, 13, 22),
        ToneProfile::Neutral => Rgb565::new(22, 28, 18),
    }
}

const fn slot_label(slot: Slot) -> &'static str {
    match slot {
        Slot::A => "A",
        Slot::B => "B",
        Slot::C => "C",
    }
}

#[derive(Debug)]
struct TextBuffer<const CAPACITY: usize> {
    bytes: [u8; CAPACITY],
    len: usize,
}

impl<const CAPACITY: usize> Default for TextBuffer<CAPACITY> {
    fn default() -> Self {
        Self {
            bytes: [0; CAPACITY],
            len: 0,
        }
    }
}

impl<const CAPACITY: usize> TextBuffer<CAPACITY> {
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len]).unwrap_or_default()
    }
}

impl<const CAPACITY: usize> Write for TextBuffer<CAPACITY> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        let destination = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        destination.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

fn decimal(value: u8, output: &mut [u8; 3]) -> &str {
    let tens = value / 10;
    let ones = value % 10;
    output[0] = b'0' + tens;
    output[1] = b'0' + ones;
    core::str::from_utf8(&output[..2]).unwrap_or_default()
}

fn unwrap_infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

struct Framebuffer<'a> {
    pixels: &'a mut [u16],
    size: Size,
}

struct ScaledTarget<'a, 'b> {
    target: &'a mut Framebuffer<'b>,
    origin: Point,
    scale: u32,
    logical_size: Size,
}

impl OriginDimensions for ScaledTarget<'_, '_> {
    fn size(&self) -> Size {
        self.logical_size
    }
}

impl DrawTarget for ScaledTarget<'_, '_> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, colour) in pixels {
            if point.x < 0 || point.y < 0 {
                continue;
            }
            let top_left = self.origin
                + Size::new(
                    u32::try_from(point.x)
                        .unwrap_or(0)
                        .saturating_mul(self.scale),
                    u32::try_from(point.y)
                        .unwrap_or(0)
                        .saturating_mul(self.scale),
                );
            unwrap_infallible(
                Rectangle::new(top_left, Size::new(self.scale, self.scale))
                    .into_styled(PrimitiveStyle::with_fill(colour))
                    .draw(self.target),
            );
        }
        Ok(())
    }
}

impl OriginDimensions for Framebuffer<'_> {
    fn size(&self) -> Size {
        self.size
    }
}

impl DrawTarget for Framebuffer<'_> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let width = self.size.width as usize;
        for Pixel(point, colour) in pixels {
            if point.x < 0 || point.y < 0 {
                continue;
            }
            let Ok(x) = usize::try_from(point.x) else {
                continue;
            };
            let Ok(y) = usize::try_from(point.y) else {
                continue;
            };
            if x < width && y < self.size.height as usize {
                self.pixels[y * width + x] = colour.into_storage();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::{vec, vec::Vec};
    use tonex_ui_model::{TunerSignalState, TunerState, UiSnapshot, UiViewModel};

    use super::*;

    fn render_medium_tuner(tuner: TunerState) -> Vec<u16> {
        let class = UiClass::Medium480x320;
        let metrics = class.metrics();
        let mut pixels = vec![0_u16; usize::from(metrics.width) * usize::from(metrics.height)];
        let snapshot = UiSnapshot {
            page: UiPage::Tuner,
            tuner,
            ..UiSnapshot::default()
        };
        render(
            &UiViewModel::new(class, snapshot).with_touch(true),
            &mut pixels,
        )
        .expect("medium tuner renders");
        pixels
    }

    #[test]
    fn tuner_meter_distinguishes_flat_in_tune_sharp_and_signal_states() {
        let base = TunerState {
            signal: TunerSignalState::Stable,
            note: Some(40),
            cents: 0.0,
            frequency_hz: 82.41,
            target_hz: 82.41,
            reference_hz: 440,
            muted: true,
        };
        let exact = render_medium_tuner(base);
        let flat = render_medium_tuner(TunerState {
            cents: -25.0,
            frequency_hz: 81.23,
            ..base
        });
        let sharp = render_medium_tuner(TunerState {
            cents: 25.0,
            frequency_hz: 83.61,
            ..base
        });
        let no_signal = render_medium_tuner(TunerState {
            signal: TunerSignalState::Waiting,
            note: None,
            frequency_hz: 0.0,
            target_hz: 0.0,
            ..base
        });
        let weak_signal = render_medium_tuner(TunerState {
            signal: TunerSignalState::Weak,
            note: None,
            frequency_hz: 0.0,
            target_hz: 0.0,
            ..base
        });
        let unavailable = render_medium_tuner(TunerState::default());

        let width = 480_usize;
        let rail_y = 320_usize * 2 / 3;
        let margin = usize::from(UiClass::Medium480x320.metrics().margin);
        let rail_left = margin + 14;
        let rail_right = 480 - margin - 14;
        let centre = rail_left.midpoint(rail_right);
        let quarter_span = (rail_right - rail_left) / 4;
        let at = |pixels: &[u16], x: usize| pixels[rail_y * width + x];

        assert_eq!(at(&exact, centre), Rgb565::new(7, 55, 18).into_storage());
        assert_eq!(
            at(&flat, centre - quarter_span),
            Rgb565::new(30, 42, 7).into_storage()
        );
        assert_eq!(
            at(&sharp, centre + quarter_span),
            Rgb565::new(30, 42, 7).into_storage()
        );
        assert_ne!(exact, no_signal);
        assert_ne!(weak_signal, no_signal);
        assert_ne!(no_signal, unavailable);
    }

    #[test]
    fn embedded_web_amp_skins_are_valid_png_files() {
        const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

        for index in 0..WEB_AMP_SKIN_COUNT {
            let id = tonex_skins::SkinId::new(u8::try_from(index).expect("skin index fits u8"))
                .expect("amp skin id");
            let png = web_amp_skin_png(id).expect("embedded amp PNG");
            assert!(
                png.starts_with(PNG_SIGNATURE),
                "invalid PNG for skin {index}"
            );
        }
    }

    #[test]
    fn every_compressed_skin_row_decodes_to_exact_rgb565_width() {
        let mut row = [0_u16; SKIN_SOURCE_WIDTH];
        for index in 0..SKIN_COUNT {
            let id = tonex_skins::SkinId::new(u8::try_from(index).expect("skin index fits u8"))
                .expect("skin id");
            for source_y in 0..SKIN_SOURCE_HEIGHT {
                assert!(
                    decode_skin_row(id, source_y, &mut row),
                    "skin {index}, row {source_y}"
                );
            }
        }
        let first = tonex_skins::SkinId::new(0).expect("first skin");
        assert!(!decode_skin_row(first, SKIN_SOURCE_HEIGHT, &mut row));
        assert!(
            SKIN_RLE.len() + core::mem::size_of_val(&SKIN_ROW_OFFSETS)
                < SKIN_COUNT * SKIN_SOURCE_WIDTH * SKIN_SOURCE_HEIGHT * 2
        );
    }

    #[test]
    fn pedal_skins_use_the_existing_bmp_fallback() {
        assert!(web_amp_skin_png(tonex_skins::SkinId::BIG_MUFF).is_none());
    }

    #[test]
    fn every_visual_class_renders_with_exact_bounds() {
        let classes = [
            UiClass::Tiny128,
            UiClass::Compact320x170,
            UiClass::Portrait240x280,
            UiClass::Landscape280x240,
            UiClass::Medium480x320,
            UiClass::Large800x480,
        ];
        for class in classes {
            for page in [
                UiPage::Stage,
                UiPage::Presets { offset: 0 },
                UiPage::EditMenu { offset: 0 },
                UiPage::Edit {
                    block: tonex_ui_model::EditBlock::Amp,
                    offset: 0,
                },
                UiPage::Global { offset: 0 },
                UiPage::Settings,
                UiPage::QuickConnect,
                UiPage::Display,
                UiPage::DeviceInfo,
                UiPage::Tuner,
            ] {
                let metrics = class.metrics();
                let mut pixels =
                    vec![0_u16; usize::from(metrics.width) * usize::from(metrics.height)];
                let snapshot = UiSnapshot {
                    page,
                    ..UiSnapshot::default()
                };
                render(
                    &UiViewModel::new(class, snapshot).with_touch(true),
                    &mut pixels,
                )
                .expect("class and page render");
                assert!(
                    pixels
                        .iter()
                        .any(|pixel| *pixel != BACKGROUND.into_storage())
                );
                if page == UiPage::QuickConnect {
                    assert!(pixels.contains(&Rgb565::WHITE.into_storage()));
                    assert!(pixels.contains(&Rgb565::BLACK.into_storage()));
                }
            }
        }
    }

    #[test]
    fn medium_panel_uses_the_full_physical_surface() {
        let class = UiClass::Medium480x320;
        let metrics = class.metrics();
        let mut pixels = vec![0_u16; usize::from(metrics.width) * usize::from(metrics.height)];
        render(&UiViewModel::new(class, UiSnapshot::default()), &mut pixels)
            .expect("medium panel renders");

        let panel = PANEL.into_storage();
        let width = usize::from(metrics.width);
        let height = usize::from(metrics.height);
        assert_eq!(pixels[0], panel);
        assert_eq!(pixels[width - 1], panel);
        assert_eq!(pixels[(height - 1) * width], panel);
        assert_eq!(pixels[height * width - 1], panel);
    }

    #[test]
    fn invalid_surface_is_rejected_without_partial_render() {
        let model = UiViewModel::new(UiClass::Tiny128, UiSnapshot::default());
        let mut pixels = [0x55aa; 8];
        assert_eq!(
            render(&model, &mut pixels),
            Err(RenderError::BufferSize {
                expected: 128 * 128,
                actual: 8
            })
        );
        assert_eq!(pixels, [0x55aa; 8]);
        assert_eq!(
            render(
                &UiViewModel::new(UiClass::Headless, UiSnapshot::default()),
                &mut []
            ),
            Err(RenderError::Headless)
        );
    }

    #[test]
    fn every_tonex_one_slot_has_a_stable_label() {
        assert_eq!(slot_label(Slot::A), "A");
        assert_eq!(slot_label(Slot::B), "B");
        assert_eq!(slot_label(Slot::C), "C");
    }

    #[test]
    fn responsive_regions_do_not_overlap() {
        for class in [
            UiClass::Tiny128,
            UiClass::Compact320x170,
            UiClass::Portrait240x280,
            UiClass::Landscape280x240,
            UiClass::Medium480x320,
            UiClass::Large800x480,
        ] {
            let metrics = class.metrics();
            let layout = stage_layout(class, true);
            let regions = [
                layout.preset_number,
                layout.preset_name,
                layout.slot,
                layout.master,
                layout.bpm,
            ];
            for rect in regions
                .into_iter()
                .chain(layout.signal_chain)
                .chain(layout.skin)
                .chain(layout.settings)
                .chain(layout.back)
            {
                assert!(rect.width > 0 && rect.height > 0, "{class:?} empty rect");
                assert!(
                    rect.right() <= metrics.width && rect.bottom() <= metrics.height,
                    "{class:?} rect escapes the display: {rect:?}"
                );
            }

            for (index, left) in layout.signal_chain.iter().enumerate() {
                assert_eq!(
                    (left.width, left.height),
                    (layout.signal_chain[0].width, layout.signal_chain[0].height),
                    "{class:?} AMP, CAB and FX must share one size"
                );
                for right in &layout.signal_chain[index + 1..] {
                    assert!(
                        !rects_overlap(*left, *right),
                        "{class:?} signal blocks overlap: {left:?} / {right:?}"
                    );
                }
            }
            assert!(
                layout.preset_name.right() <= layout.preset_number.x,
                "{class:?} preset name touches the preset number"
            );
            assert!(
                layout.preset_number.right() <= layout.slot.x,
                "{class:?} preset number touches the slot"
            );
            if let Some(settings) = layout.settings {
                assert!(
                    settings.right() <= layout.preset_name.x,
                    "{class:?} settings gear touches the preset name"
                );
            }
            if class != UiClass::Tiny128 {
                assert_eq!(
                    (layout.master.y, layout.master.height),
                    (layout.bpm.y, layout.bpm.height),
                    "{class:?} performance rail is not level"
                );
                assert!(
                    layout.master.right() < layout.bpm.x,
                    "{class:?} performance rail cells overlap"
                );
            }
        }
    }

    #[test]
    fn medium_signal_chain_has_exact_equal_spacing() {
        let chain = stage_layout(UiClass::Medium480x320, true).signal_chain;
        for rect in chain {
            assert_eq!((rect.width, rect.height), (58, 94));
        }
        for pair in chain.windows(2) {
            assert_eq!(pair[1].x - pair[0].right(), 7);
        }
    }

    const fn rects_overlap(left: UiRect, right: UiRect) -> bool {
        left.x < right.right()
            && left.right() > right.x
            && left.y < right.bottom()
            && left.bottom() > right.y
    }
}
