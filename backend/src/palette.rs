use image::GenericImageView;
use serde::{Deserialize, Serialize};

/// Colours pulled off a poster, used to tint a title page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Palette {
    /// Most saturated usable colour: buttons, links, progress bars.
    pub accent: String,
    /// Dark companion used behind the hero so text keeps its contrast.
    pub shade: String,
    /// Light companion for subtle fills.
    pub tint: String,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            accent: "#7a5cff".into(),
            shade: "#140f2b".into(),
            tint: "#cfc4ff".into(),
        }
    }
}

/// Quantise into 4x4x4 RGB buckets and score each by population and saturation.
/// This picks what a person would call "the poster colour" far more often than
/// an average does, because averaging always comes out mud.
pub fn extract(bytes: &[u8]) -> Option<Palette> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img.thumbnail(80, 120);

    let mut buckets = [(0u64, 0u64, 0u64, 0u64); 64];
    for (_, _, px) in img.pixels() {
        let [r, g, b, a] = px.0;
        if a < 128 {
            continue;
        }
        let (_, s, l) = rgb_to_hsl(r, g, b);
        // Near-black, near-white and grey are background, not identity.
        if !(0.08..=0.92).contains(&l) || s < 0.12 {
            continue;
        }
        let idx = (r as usize / 64) * 16 + (g as usize / 64) * 4 + (b as usize / 64);
        let bucket = &mut buckets[idx];
        bucket.0 += 1;
        bucket.1 += r as u64;
        bucket.2 += g as u64;
        bucket.3 += b as u64;
    }

    let mut best: Option<(f64, (u8, u8, u8))> = None;
    for (count, rs, gs, bs) in buckets.iter().copied() {
        if count == 0 {
            continue;
        }
        let r = (rs / count) as u8;
        let g = (gs / count) as u8;
        let b = (bs / count) as u8;
        let (_, s, l) = rgb_to_hsl(r, g, b);
        // Population matters, but a vivid minority beats a dull majority.
        let score = (count as f64).sqrt() * (0.25 + s) * (1.0 - (l - 0.5).abs());
        if best.as_ref().map(|(bs, _)| score > *bs).unwrap_or(true) {
            best = Some((score, (r, g, b)));
        }
    }

    let (r, g, b) = best?.1;
    let (h, s, _) = rgb_to_hsl(r, g, b);
    let s = s.clamp(0.45, 0.9);

    Some(Palette {
        accent: hex(hsl_to_rgb(h, s, 0.62)),
        shade: hex(hsl_to_rgb(h, (s * 0.8).min(0.7), 0.1)),
        tint: hex(hsl_to_rgb(h, (s * 0.6).min(0.5), 0.82)),
    })
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d.abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f64::EPSILON {
        ((g - b) / d) % 6.0
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let h = (h * 60.0 + 360.0) % 360.0;
    (h, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let f = |v: f64| (((v + m) * 255.0).round().clamp(0.0, 255.0)) as u8;
    (f(r), f(g), f(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_roundtrip_is_close_enough() {
        for rgb in [(200u8, 30u8, 40u8), (12, 180, 90), (40, 60, 200)] {
            let (h, s, l) = rgb_to_hsl(rgb.0, rgb.1, rgb.2);
            let back = hsl_to_rgb(h, s, l);
            assert!(
                (back.0 as i32 - rgb.0 as i32).abs() <= 2,
                "{back:?} vs {rgb:?}"
            );
            assert!(
                (back.1 as i32 - rgb.1 as i32).abs() <= 2,
                "{back:?} vs {rgb:?}"
            );
            assert!(
                (back.2 as i32 - rgb.2 as i32).abs() <= 2,
                "{back:?} vs {rgb:?}"
            );
        }
    }

    #[test]
    fn picks_the_vivid_colour_over_the_grey_background() {
        // Grey field with a red block in one corner.
        let mut img = image::RgbImage::from_pixel(40, 40, image::Rgb([90, 90, 92]));
        for y in 0..14 {
            for x in 0..14 {
                img.put_pixel(x, y, image::Rgb([210, 20, 30]));
            }
        }
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();

        let p = extract(&buf.into_inner()).expect("a palette");
        let c = u32::from_str_radix(&p.accent[1..], 16).unwrap();
        let (h, _, _) = rgb_to_hsl((c >> 16) as u8, (c >> 8) as u8, c as u8);
        assert!(
            h < 25.0 || h > 340.0,
            "expected a red accent, got {}",
            p.accent
        );
    }
}
