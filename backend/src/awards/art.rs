//! An automatic badge's pictures, drawn from its SVG when first asked for.
//!
//! They are never written to the database: every nightly backup is a whole
//! copy pushed to git, and 150 large PNGs would add tens of megabytes to each
//! one. The small pictures, which every profile shows, stay in memory once
//! drawn; the large ones, opened one at a time, keep only the last few.

use resvg::{tiny_skia, usvg};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::badges::{Size, LARGE_PIXELS, SMALL_PIXELS};

/// Large pictures kept drawn: about 300 KB each.
const LARGE_KEPT: usize = 6;

/// An SVG drawn to a square PNG `side` pixels across.
pub fn render(svg: &str, side: u32) -> Option<Vec<u8>> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = side as f32 / size.width().max(size.height());
    let mut pixmap = tiny_skia::Pixmap::new(side, side)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().ok()
}

type Drawn = Mutex<HashMap<String, Arc<Vec<u8>>>>;

fn drawn(size: Size) -> &'static Drawn {
    static SMALL: OnceLock<Drawn> = OnceLock::new();
    static LARGE: OnceLock<Drawn> = OnceLock::new();
    match size {
        Size::Small => SMALL.get_or_init(Default::default),
        Size::Large => LARGE.get_or_init(Default::default),
    }
}

/// A catalogue badge's picture. Drawing a large one takes a moment, so call
/// this off the async threads (`picture`).
pub fn png(key: &str, size: Size) -> Option<Arc<Vec<u8>>> {
    let kept = drawn(size);
    if let Some(found) = kept.lock().ok()?.get(key) {
        return Some(found.clone());
    }
    let side = match size {
        Size::Large => LARGE_PIXELS,
        Size::Small => SMALL_PIXELS,
    };
    let bytes = Arc::new(render(super::catalogue::art(key)?, side)?);
    let mut kept = kept.lock().ok()?;
    if size == Size::Large && kept.len() >= LARGE_KEPT {
        kept.clear();
    }
    kept.insert(key.to_string(), bytes.clone());
    Some(bytes)
}

/// `png`, on a blocking thread.
pub async fn picture(key: String, size: Size) -> Option<Vec<u8>> {
    tokio::task::spawn_blocking(move || png(&key, size).map(|b| b.as_ref().clone()))
        .await
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_badge_draws_to_the_sizes_a_badge_is_kept_at() {
        let small = png("films.1", Size::Small).expect("drawn");
        assert_eq!(crate::badges::png_size(&small), Some((128, 128)));
        assert!(
            crate::badges::check_picture(&small, SMALL_PIXELS, crate::badges::SMALL_MAX_BYTES)
                .is_ok()
        );
        let large = png("triple-bill", Size::Large).expect("drawn");
        assert!(
            crate::badges::check_picture(&large, LARGE_PIXELS, crate::badges::LARGE_MAX_BYTES)
                .is_ok()
        );
    }
}
