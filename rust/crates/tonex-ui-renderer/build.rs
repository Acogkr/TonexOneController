use std::{
    env, fs,
    io::BufWriter,
    path::{Path, PathBuf},
};

const WIDTH: usize = 240;
const HEIGHT: usize = 80;
const WEB_WIDTH: usize = 200;
const WEB_HEIGHT: usize = 67;
const WEB_AMP_SKIN_COUNT: usize = 28;
const PANEL: [u8; 3] = [25, 24, 41];
const ASSETS: [&str; 50] = [
    "jcm.png",
    "slvrface.png",
    "tnxablk.png",
    "5150.png",
    "ampgchrm.png",
    "fndrtwdg.png",
    "fndrhtrd.png",
    "msbogdul.png",
    "elgntblu.png",
    "mdnwhplx.png",
    "roljazz.png",
    "orngr120.png",
    "mdnbkplx.png",
    "fndrtwin.png",
    "ba500.png",
    "msamkwd.png",
    "mesamkv.png",
    "jtm.png",
    "jbdumble.png",
    "jetcity.png",
    "ac30.png",
    "evh.png",
    "tnxared.png",
    "friedman.png",
    "supro.png",
    "diezel.png",
    "whtmdrn.png",
    "woodamp.png",
    "bigmuff.png",
    "bossblk.png",
    "bossslvr.png",
    "bossyel.png",
    "fuzzred.png",
    "fuzzslvr.png",
    "ibnzblue.png",
    "ibnzdblu.png",
    "ibnzgrn.png",
    "ibnzred.png",
    "klongld.png",
    "lifepdl.png",
    "mngglry.png",
    "mxrdbbl.png",
    "mxrdblrd.png",
    "mxrsngbk.png",
    "mxrsnggd.png",
    "mxrsnggn.png",
    "mxrsgorg.png",
    "mxrsngwh.png",
    "mxrsngyl.png",
    "ratyell.png",
];

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let assets = manifest.join("assets/skins");
    println!("cargo:rerun-if-changed={}", assets.display());
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let output = output_dir.join("skins.rle");
    let mut blob = Vec::new();
    let mut row_offsets = Vec::with_capacity(ASSETS.len() * HEIGHT + 1);
    row_offsets.push(0_u32);
    let mut web_blob = Vec::new();
    let mut web_offsets = Vec::with_capacity(WEB_AMP_SKIN_COUNT + 1);
    web_offsets.push(0_u32);
    for (index, asset) in ASSETS.into_iter().enumerate() {
        append_asset(&assets.join(asset), &mut blob, &mut row_offsets);
        if index < WEB_AMP_SKIN_COUNT {
            append_web_asset(&assets.join(asset), &mut web_blob);
            web_offsets.push(u32::try_from(web_blob.len()).expect("Web skin blob fits u32"));
        }
    }
    let file = fs::File::create(output).expect("create generated skin blob");
    let mut writer = BufWriter::new(file);
    std::io::Write::write_all(&mut writer, &blob).expect("write generated skin blob");
    fs::write(
        output_dir.join("skin_row_offsets.rs"),
        rust_u32_array(&row_offsets),
    )
    .expect("write generated skin row offsets");
    fs::write(output_dir.join("web_amp_skins.pngs"), web_blob)
        .expect("write generated Web skin blob");
    fs::write(
        output_dir.join("web_amp_skin_offsets.rs"),
        rust_u32_array(&web_offsets),
    )
    .expect("write generated Web skin offsets");
}

fn rust_u32_array(values: &[u32]) -> String {
    let values = values
        .iter()
        .map(|value| rust_integer(*value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{values}]")
}

fn rust_integer(value: u32) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push('_');
        }
        grouped.push(digit);
    }
    grouped
}

fn append_web_asset(path: &Path, output: &mut Vec<u8>) {
    let rgb = fitted_rgb(path);
    let mut png_bytes = Vec::new();
    {
        let width = u32::try_from(WEB_WIDTH).expect("Web skin width fits u32");
        let height = u32::try_from(WEB_HEIGHT).expect("Web skin height fits u32");
        let mut png_encoder = png::Encoder::new(&mut png_bytes, width, height);
        png_encoder.set_color(png::ColorType::Rgb);
        png_encoder.set_depth(png::BitDepth::Eight);
        png_encoder.set_compression(png::Compression::Best);
        let mut writer = png_encoder.write_header().expect("encode Web skin header");
        writer
            .write_image_data(&rgb)
            .expect("encode Web skin pixels");
    }
    output.extend_from_slice(&png_bytes);
}

fn fitted_rgb(path: &Path) -> Vec<u8> {
    let file = fs::File::open(path).unwrap_or_else(|error| {
        panic!("open skin {}: {error}", path.display());
    });
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("decode PNG header");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("decode PNG pixels");
    let bytes = &buffer[..info.buffer_size()];
    let source_width = info.width as usize;
    let source_height = info.height as usize;
    let (fitted_width, fitted_height) = if WEB_WIDTH * source_height <= WEB_HEIGHT * source_width {
        (
            WEB_WIDTH,
            (source_height * WEB_WIDTH + source_width / 2) / source_width,
        )
    } else {
        (
            (source_width * WEB_HEIGHT + source_height / 2) / source_height,
            WEB_HEIGHT,
        )
    };
    let fitted_width = fitted_width.max(1);
    let fitted_height = fitted_height.max(1);
    let left = (WEB_WIDTH - fitted_width) / 2;
    let top = (WEB_HEIGHT - fitted_height) / 2;
    let mut rgb = Vec::with_capacity(WEB_WIDTH * WEB_HEIGHT * 3);
    for y in 0..WEB_HEIGHT {
        for x in 0..WEB_WIDTH {
            let pixel =
                if x >= left && x < left + fitted_width && y >= top && y < top + fitted_height {
                    let source_x = ((x - left) * source_width / fitted_width).min(source_width - 1);
                    let source_y =
                        ((y - top) * source_height / fitted_height).min(source_height - 1);
                    composite_pixel(bytes, source_y * source_width + source_x, info.color_type)
                } else {
                    PANEL
                };
            rgb.extend_from_slice(&pixel);
        }
    }
    rgb
}

fn append_asset(path: &Path, output: &mut Vec<u8>, row_offsets: &mut Vec<u32>) {
    let file = fs::File::open(path).unwrap_or_else(|error| {
        panic!("open skin {}: {error}", path.display());
    });
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("decode PNG header");
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).expect("decode PNG pixels");
    let bytes = &buffer[..info.buffer_size()];
    let source_width = info.width as usize;
    let source_height = info.height as usize;
    let (fitted_width, fitted_height) = if WIDTH * source_height <= HEIGHT * source_width {
        (
            WIDTH,
            (source_height * WIDTH + source_width / 2) / source_width,
        )
    } else {
        (
            (source_width * HEIGHT + source_height / 2) / source_height,
            HEIGHT,
        )
    };
    let fitted_width = fitted_width.max(1);
    let fitted_height = fitted_height.max(1);
    let left = (WIDTH - fitted_width) / 2;
    let top = (HEIGHT - fitted_height) / 2;

    for y in 0..HEIGHT {
        let mut previous = None;
        let mut run_length = 0_u8;
        for x in 0..WIDTH {
            let rgb = if x >= left && x < left + fitted_width && y >= top && y < top + fitted_height
            {
                let source_x = ((x - left) * source_width / fitted_width).min(source_width - 1);
                let source_y = ((y - top) * source_height / fitted_height).min(source_height - 1);
                composite_pixel(bytes, source_y * source_width + source_x, info.color_type)
            } else {
                PANEL
            };
            let pixel = rgb565(rgb);
            if previous == Some(pixel) && run_length < u8::MAX {
                run_length += 1;
            } else {
                if let Some(previous) = previous {
                    append_run(output, run_length, previous);
                }
                previous = Some(pixel);
                run_length = 1;
            }
        }
        append_run(
            output,
            run_length,
            previous.expect("every skin row contains pixels"),
        );
        row_offsets.push(u32::try_from(output.len()).expect("compressed skin blob fits u32"));
    }
}

fn append_run(output: &mut Vec<u8>, length: u8, pixel: u16) {
    output.push(length);
    output.extend_from_slice(&pixel.to_le_bytes());
}

fn composite_pixel(bytes: &[u8], index: usize, color_type: png::ColorType) -> [u8; 3] {
    let (red, green, blue, alpha) = match color_type {
        png::ColorType::Rgba => {
            let offset = index * 4;
            (
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            )
        }
        png::ColorType::Rgb => {
            let offset = index * 3;
            (bytes[offset], bytes[offset + 1], bytes[offset + 2], 255)
        }
        png::ColorType::GrayscaleAlpha => {
            let offset = index * 2;
            (
                bytes[offset],
                bytes[offset],
                bytes[offset],
                bytes[offset + 1],
            )
        }
        png::ColorType::Grayscale => {
            let value = bytes[index];
            (value, value, value, 255)
        }
        png::ColorType::Indexed => unreachable!("EXPAND removes indexed pixels"),
    };
    let alpha = u16::from(alpha);
    let mut result = [0; 3];
    for (index, (channel, source)) in result.iter_mut().zip([red, green, blue]).enumerate() {
        let composited =
            (u16::from(source) * alpha + u16::from(PANEL[index]) * (255 - alpha)) / 255;
        *channel = u8::try_from(composited).expect("alpha composite remains in byte range");
    }
    result
}

const fn rgb565(rgb: [u8; 3]) -> u16 {
    ((rgb[0] as u16 >> 3) << 11) | ((rgb[1] as u16 >> 2) << 5) | (rgb[2] as u16 >> 3)
}
