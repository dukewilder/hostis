//! Station art: a cover, a banner, and the art inside each track.
//!
//! Every picture is decoded and drawn again before it leaves the broadcaster's device, so
//! what travels is a plain JPEG of a fixed size: no location, no camera details, nothing
//! hidden in the file. Listeners decode art here, in memory-safe code, and never hand a
//! stranger's file to the phone's own image decoder.
//!
//! No rights reserved.

use std::io::Cursor;

use image::{ImageFormat, ImageReader, Limits, RgbImage, codecs::jpeg::JpegEncoder, imageops::FilterType};

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Cover,
    Banner,
    Track,
}

impl Kind {
    pub fn size(self) -> (u32, u32) {
        match self {
            Kind::Cover | Kind::Track => (640, 640),
            Kind::Banner => (1500, 500),
        }
    }
}

/// The largest art file any node will fetch, carry or decode.
pub const MAX_BYTES: usize = 400_000;

pub struct Prepared {
    pub jpeg: Vec<u8>,
    pub w: u16,
    pub h: u16,
    pub accent: [u8; 3],
}

fn limits(max_side: u32) -> Limits {
    let mut l = Limits::default();
    l.max_image_width = Some(max_side);
    l.max_image_height = Some(max_side);
    l.max_alloc = Some(300 << 20);
    l
}

/// Takes any picture the broadcaster chose and makes station art of it.
pub fn prepare(raw: &[u8], kind: Kind) -> Result<Prepared, String> {
    let mut r = ImageReader::new(Cursor::new(raw)).with_guessed_format().map_err(|e| e.to_string())?;
    r.limits(limits(12_000));
    let img = r.decode().map_err(|e| format!("not a picture we can read: {e}"))?;
    let (w, h) = kind.size();
    let img = img.resize_to_fill(w, h, FilterType::Lanczos3).to_rgb8();
    let accent = accent(&img);
    for q in [86u8, 78, 70, 60] {
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, q).encode_image(&img).map_err(|e| e.to_string())?;
        if jpeg.len() <= MAX_BYTES {
            return Ok(Prepared { jpeg, w: w as u16, h: h as u16, accent });
        }
    }
    Err("the picture would not fit".into())
}

/// Decodes art fetched from the network into RGBA, refusing anything that is not a plain
/// JPEG of a size a station could have made.
pub fn decode(jpeg: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if jpeg.len() > MAX_BYTES {
        return Err("too large".into());
    }
    let mut r = ImageReader::with_format(Cursor::new(jpeg), ImageFormat::Jpeg);
    r.limits(limits(1500));
    let img = r.decode().map_err(|e| e.to_string())?.to_rgba8();
    Ok((img.width(), img.height(), img.into_raw()))
}

/// The colour a station's page is drawn in: the strongest colour in its cover, lifted until
/// it reads on black. A grey cover gets bone.
pub fn accent(img: &RgbImage) -> [u8; 3] {
    let mut buckets = std::collections::HashMap::<u16, (f64, [f64; 3])>::new();
    for p in img.pixels().step_by(7) {
        let [r, g, b] = p.0;
        let (mx, mn) = (r.max(g).max(b) as f64, r.min(g).min(b) as f64);
        if mx < 40.0 {
            continue;
        }
        let sat = (mx - mn) / mx;
        let w = sat * sat * (mx / 255.0) + 0.0001;
        let key = ((r as u16 >> 4) << 8) | ((g as u16 >> 4) << 4) | (b as u16 >> 4);
        let e = buckets.entry(key).or_insert((0.0, [0.0; 3]));
        e.0 += w;
        e.1[0] += r as f64 * w;
        e.1[1] += g as f64 * w;
        e.1[2] += b as f64 * w;
    }
    let bone = [0xED, 0xE4, 0xCE];
    let Some((_, (w, sum))) = buckets.into_iter().max_by(|a, b| a.1.0.total_cmp(&b.1.0)) else { return bone };
    let mut c = [sum[0] / w, sum[1] / w, sum[2] / w];
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    if mx == 0.0 || (mx - mn) / mx < 0.18 {
        return bone;
    }
    let lum = |c: &[f64; 3]| (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255.0;
    while lum(&c) < 0.42 {
        for v in &mut c {
            *v += (255.0 - *v) * 0.08;
        }
    }
    [c[0] as u8, c[1] as u8, c[2] as u8]
}
