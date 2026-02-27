use ab_glyph::{FontRef, PxScale};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader, Rgba, RgbaImage};
use imageproc::drawing::{draw_text_mut, text_size};
use serde::Deserialize;
use std::io::Cursor;

#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    #[default]
    Material,
    Intent,
}

pub const CARD_W: u32 = 750;
pub const CARD_H: u32 = 1050;

// Content area inside the frame's ornate border
const CONTENT_X: i32 = 56;
const CONTENT_Y: i32 = 80;
const CONTENT_W: u32 = 638;

// Name banner overlaid on top of the art
const NAME_BANNER_H: u32 = 80;
const BANNER_R: u32 = 8;

// Material card colors (warm gold)
const COLOR_BANNER: Rgba<u8> = Rgba([30, 20, 12, 190]);
const COLOR_NAME: Rgba<u8> = Rgba([220, 195, 130, 255]);

// Intent card colors (cool purple/silver)
const COLOR_INTENT_BANNER: Rgba<u8> = Rgba([20, 12, 35, 200]);
const COLOR_INTENT_NAME: Rgba<u8> = Rgba([180, 160, 220, 255]);

/// Brightness threshold below which frame pixels are treated as transparent.
const BLACK_THRESHOLD: u16 = 30;

static FONT_BYTES: &[u8] = include_bytes!("../assets/Cinzel-Bold.ttf");
static FRAME_BYTES: &[u8] = include_bytes!("../assets/card-frame.png");
static FRAME_INTENT_BYTES: &[u8] = include_bytes!("../assets/card-frame-intent.png");

pub fn render_card(
    name: &str,
    image_bytes: &[u8],
    kind: &CardKind,
) -> Result<Vec<u8>, String> {
    let font = FontRef::try_from_slice(FONT_BYTES).map_err(|e| format!("font error: {e}"))?;

    // Load the appropriate frame for the card kind
    let frame_bytes = match kind {
        CardKind::Intent => FRAME_INTENT_BYTES,
        CardKind::Material => FRAME_BYTES,
    };

    // Resize the frame, making its black interior transparent
    let mut frame_img = ImageReader::new(Cursor::new(frame_bytes))
        .with_guessed_format()
        .map_err(|e| format!("frame format error: {e}"))?
        .decode()
        .map_err(|e| format!("frame decode error: {e}"))?
        .resize_exact(CARD_W, CARD_H, FilterType::Lanczos3)
        .to_rgba8();
    remove_black_background(&mut frame_img);

    // 1. Draw art as full card background
    let art_img = ImageReader::new(Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|e| format!("image format error: {e}"))?
        .decode()
        .map_err(|e| format!("image decode error: {e}"))?;
    let mut card = resize_cover(&art_img, CARD_W, CARD_H);

    // 2. Overlay the ornate frame on top (black interior is now transparent)
    alpha_overlay(&mut card, &frame_img);

    // 3. Draw semi-transparent name banner over the top of the art
    let (banner_color, name_color) = match kind {
        CardKind::Intent => (COLOR_INTENT_BANNER, COLOR_INTENT_NAME),
        CardKind::Material => (COLOR_BANNER, COLOR_NAME),
    };

    let banner_y = CONTENT_Y;
    draw_rounded_rect(
        &mut card,
        CONTENT_X,
        banner_y,
        CONTENT_W,
        NAME_BANNER_H,
        BANNER_R,
        banner_color,
    );

    // 4. Draw name text (centered in banner)
    let max_name_w = CONTENT_W - 40;
    let mut name_px = 60.0_f32;
    loop {
        let (tw, _) = text_size(PxScale::from(name_px), &font, name);
        if tw <= max_name_w || name_px <= 22.0 {
            break;
        }
        name_px -= 2.0;
    }
    let name_scale = PxScale::from(name_px);
    let (name_w, name_h) = text_size(name_scale, &font, name);
    let name_x = CONTENT_X + (CONTENT_W as i32 - name_w as i32) / 2;
    let name_y = banner_y + (NAME_BANNER_H as i32 - name_h as i32) / 2;
    draw_text_mut(&mut card, name_color, name_x, name_y, name_scale, &font, name);

    // Encode to PNG
    let mut buf = Cursor::new(Vec::new());
    card.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("png encode error: {e}"))?;
    Ok(buf.into_inner())
}

/// Make near-black pixels in the frame transparent so the art shows through.
fn remove_black_background(img: &mut RgbaImage) {
    for pixel in img.pixels_mut() {
        let brightness = pixel[0] as u16 + pixel[1] as u16 + pixel[2] as u16;
        if brightness < BLACK_THRESHOLD {
            pixel[3] = 0;
        }
    }
}

/// Overlay src onto dst using alpha compositing.
fn alpha_overlay(dst: &mut RgbaImage, src: &RgbaImage) {
    for (x, y, src_px) in src.enumerate_pixels() {
        if x >= dst.width() || y >= dst.height() {
            continue;
        }
        let sa = src_px[3] as f32 / 255.0;
        if sa < 0.01 {
            continue;
        }
        let dst_px = dst.get_pixel(x, y);
        let da = dst_px[3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);
        if out_a < 0.01 {
            continue;
        }
        let blend = |s: u8, d: u8| -> u8 {
            ((s as f32 * sa + d as f32 * da * (1.0 - sa)) / out_a) as u8
        };
        dst.put_pixel(
            x,
            y,
            Rgba([
                blend(src_px[0], dst_px[0]),
                blend(src_px[1], dst_px[1]),
                blend(src_px[2], dst_px[2]),
                (out_a * 255.0) as u8,
            ]),
        );
    }
}

/// Resize image to cover target area (crop to fill, no stretching or letterboxing).
fn resize_cover(img: &DynamicImage, target_w: u32, target_h: u32) -> RgbaImage {
    let (src_w, src_h) = img.dimensions();
    let scale = f64::max(
        target_w as f64 / src_w as f64,
        target_h as f64 / src_h as f64,
    );
    let scaled_w = (src_w as f64 * scale).ceil() as u32;
    let scaled_h = (src_h as f64 * scale).ceil() as u32;
    let scaled = img.resize_exact(scaled_w, scaled_h, FilterType::Lanczos3);
    let crop_x = scaled_w.saturating_sub(target_w) / 2;
    let crop_y = scaled_h.saturating_sub(target_h) / 2;
    scaled
        .crop_imm(crop_x, crop_y, target_w, target_h)
        .to_rgba8()
}

/// Draw a filled rectangle with rounded corners, respecting alpha.
fn draw_rounded_rect(
    img: &mut RgbaImage,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    r: u32,
    color: Rgba<u8>,
) {
    let r_f = r as f64;
    let sa = color[3] as f32 / 255.0;
    for py in 0..h {
        for px in 0..w {
            let ix = x + px as i32;
            let iy = y + py as i32;
            if ix < 0 || iy < 0 || ix >= img.width() as i32 || iy >= img.height() as i32 {
                continue;
            }
            let inside = if px < r && py < r {
                corner_inside(px, py, r_f, r_f, r_f)
            } else if px >= w - r && py < r {
                corner_inside(px, py, w as f64 - r_f, r_f, r_f)
            } else if px < r && py >= h - r {
                corner_inside(px, py, r_f, h as f64 - r_f, r_f)
            } else if px >= w - r && py >= h - r {
                corner_inside(px, py, w as f64 - r_f, h as f64 - r_f, r_f)
            } else {
                true
            };
            if inside {
                if sa >= 0.99 {
                    img.put_pixel(ix as u32, iy as u32, color);
                } else {
                    let dst = img.get_pixel(ix as u32, iy as u32);
                    let da = dst[3] as f32 / 255.0;
                    let out_a = sa + da * (1.0 - sa);
                    let blend = |s: u8, d: u8| -> u8 {
                        ((s as f32 * sa + d as f32 * da * (1.0 - sa)) / out_a.max(0.01)) as u8
                    };
                    img.put_pixel(
                        ix as u32,
                        iy as u32,
                        Rgba([
                            blend(color[0], dst[0]),
                            blend(color[1], dst[1]),
                            blend(color[2], dst[2]),
                            (out_a * 255.0) as u8,
                        ]),
                    );
                }
            }
        }
    }
}

fn corner_inside(px: u32, py: u32, cx: f64, cy: f64, r: f64) -> bool {
    let dx = px as f64 + 0.5 - cx;
    let dy = py as f64 + 0.5 - cy;
    dx * dx + dy * dy <= r * r
}
