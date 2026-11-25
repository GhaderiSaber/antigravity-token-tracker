//! System tray icon rendering and on-disk PNG cache helpers.

use image::{ImageBuffer, ImageError, Rgba};
use ksni::Icon;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Pixel width and height of rendered StatusNotifierItem tray icons.
const ICON_SIZE: u32 = 48;

/// Quota status rendered by the tray icon and tooltip title.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuotaIndicator {
    /// Quota is available at the given remaining percentage (`0.0..=100.0`).
    Available(f64),
    /// Quota is exhausted (`0.0%` or no active session).
    Exhausted,
}

impl QuotaIndicator {
    /// Converts a remaining percentage into a [`QuotaIndicator`].
    ///
    /// Non-finite or non-positive percentages map to [`QuotaIndicator::Exhausted`];
    /// positive values are clamped to `0.0..=100.0`.
    pub fn from_percent(pct: f64) -> Self {
        if pct <= 0.0 || !pct.is_finite() {
            Self::Exhausted
        } else {
            Self::Available(pct.clamp(0.0, 100.0))
        }
    }

    /// Returns the icon theme name corresponding to this indicator state.
    #[must_use]
    pub fn icon_name(self) -> String {
        match self {
            Self::Available(pct) => {
                let clamped = if pct.is_finite() {
                    pct.clamp(0.0, 100.0)
                } else {
                    0.0
                };
                format!("quota_{}", clamped.round() as i64)
            }
            Self::Exhausted => "quota_exhausted".to_string(),
        }
    }

    /// Returns the human-readable tray title string for this indicator state.
    #[must_use]
    pub fn title_text(self) -> String {
        match self {
            Self::Available(pct) => format!("Antigravity: {pct:.1}%"),
            Self::Exhausted => "Antigravity: Quota Exhausted".to_string(),
        }
    }

    /// Returns `true` when this indicator represents an exhausted quota.
    #[must_use]
    pub const fn is_exhausted(self) -> bool {
        matches!(self, Self::Exhausted)
    }
}

/// Returns the default directory used to cache rendered tray icon PNGs.
#[must_use]
pub fn default_icon_cache_dir() -> PathBuf {
    dirs::config_dir().map_or_else(
        || {
            PathBuf::from(".config")
                .join("antigravity-token-tracker")
                .join("icons")
        },
        |c| c.join("antigravity-token-tracker").join("icons"),
    )
}

fn point_in_polygon(x: f32, y: f32, poly: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        let intersect = ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Renders an in-memory ARGB32 tray [`Icon`] and icon theme name without performing any disk I/O.
#[must_use]
pub fn render_tray_icon(indicator: QuotaIndicator) -> (Icon, String) {
    let width = ICON_SIZE as usize;
    let height = ICON_SIZE as usize;
    let center_x = 24.0f32;
    let center_y = 24.0f32;

    let (pct, is_exhausted) = match indicator {
        QuotaIndicator::Available(p) if p.is_finite() => (p.clamp(0.0, 100.0), false),
        QuotaIndicator::Available(_) | QuotaIndicator::Exhausted => (0.0, true),
    };

    let (r, g, b) = if is_exhausted || pct <= 10.0 {
        (234, 67, 53) // Red
    } else if pct <= 25.0 {
        (251, 188, 4) // Yellow
    } else if pct <= 50.0 {
        (66, 133, 244) // Blue
    } else {
        (52, 168, 83) // Green
    };

    let fraction = if is_exhausted {
        0.0
    } else {
        (pct / 100.0).clamp(0.0, 1.0) as f32
    };

    // Lightning bolt points
    let bolt_pts = [
        (26.0f32, 12.0f32),
        (18.0, 25.0),
        (24.0, 25.0),
        (22.0, 36.0),
        (30.0, 23.0),
        (24.0, 23.0),
    ];

    let mut argb_data = Vec::with_capacity(width * height * 4);

    for y in 0..height {
        for x in 0..width {
            let px = x as f32;
            let py = y as f32;
            let dx = px - center_x;
            let dy = py - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            let mut pixel_a = 0u8;
            let mut pixel_r = 0u8;
            let mut pixel_g = 0u8;
            let mut pixel_b = 0u8;

            if point_in_polygon(px, py, &bolt_pts) {
                pixel_a = 255;
                pixel_r = 255;
                pixel_g = 255;
                pixel_b = 255;
            } else if (15.0..=22.0).contains(&dist) {
                // Thickened ring track for crisp downsampling on 16x16 / 22x22 panels.
                let theta = dy.atan2(dx);
                let deg = (theta.to_degrees() + 90.0).rem_euclid(360.0);
                if deg <= fraction * 360.0 {
                    pixel_a = 255;
                    pixel_r = r;
                    pixel_g = g;
                    pixel_b = b;
                } else {
                    pixel_a = 180;
                    pixel_r = 75;
                    pixel_g = 80;
                    pixel_b = 90;
                }
            }

            // ksni expects ARGB32 byte order.
            argb_data.push(pixel_a);
            argb_data.push(pixel_r);
            argb_data.push(pixel_g);
            argb_data.push(pixel_b);
        }
    }

    let icon = Icon {
        width: ICON_SIZE as i32,
        height: ICON_SIZE as i32,
        data: argb_data,
    };

    (icon, indicator.icon_name())
}

/// Ensures a PNG icon file exists in `cache_dir` for `indicator`, writing it only on cache miss.
///
/// # Errors
///
/// Returns an [`io::Error`] if `cache_dir` cannot be created or the PNG cannot be encoded/written.
pub fn ensure_cached_icon_png(indicator: QuotaIndicator, cache_dir: &Path) -> io::Result<PathBuf> {
    let png_path = cache_dir.join(format!("{}.png", indicator.icon_name()));
    if png_path.is_file() {
        return Ok(png_path);
    }

    fs::create_dir_all(cache_dir)?;
    let (icon, _) = render_tray_icon(indicator);
    let mut img_buf = ImageBuffer::new(ICON_SIZE, ICON_SIZE);
    for (idx, chunk) in icon.data.chunks_exact(4).enumerate() {
        let x = (idx as u32) % ICON_SIZE;
        let y = (idx as u32) / ICON_SIZE;
        img_buf.put_pixel(x, y, Rgba([chunk[1], chunk[2], chunk[3], chunk[0]]));
    }

    img_buf.save(&png_path).map_err(|err| match err {
        ImageError::IoError(io_err) => io_err,
        other => io::Error::other(other),
    })?;

    Ok(png_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_quota_indicator_variants_and_formatting() {
        let ok = QuotaIndicator::from_percent(85.0);
        assert_eq!(ok, QuotaIndicator::Available(85.0));
        assert_eq!(ok.icon_name(), "quota_85");
        assert_eq!(ok.title_text(), "Antigravity: 85.0%");
        assert!(!ok.is_exhausted());

        let zero = QuotaIndicator::from_percent(0.0);
        assert_eq!(zero, QuotaIndicator::Exhausted);
        assert_eq!(zero.icon_name(), "quota_exhausted");
        assert_eq!(zero.title_text(), "Antigravity: Quota Exhausted");
        assert!(zero.is_exhausted());

        let nan = QuotaIndicator::from_percent(f64::NAN);
        assert_eq!(nan, QuotaIndicator::Exhausted);
        let over = QuotaIndicator::from_percent(150.0);
        assert_eq!(over, QuotaIndicator::Available(100.0));
    }

    #[test]
    fn test_render_tray_icon_is_pure_and_cache_skips_existing_png() {
        let (icon, name) = render_tray_icon(QuotaIndicator::Available(72.0));
        assert_eq!(icon.width, 48);
        assert_eq!(icon.height, 48);
        assert_eq!(icon.data.len(), 48 * 48 * 4);
        assert_eq!(name, "quota_72");

        let dir = tempdir().unwrap();
        let path = ensure_cached_icon_png(QuotaIndicator::Available(72.0), dir.path()).unwrap();
        assert!(path.is_file());
        let first_mtime = fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        let path2 = ensure_cached_icon_png(QuotaIndicator::Available(72.0), dir.path()).unwrap();
        let second_mtime = fs::metadata(&path2).unwrap().modified().unwrap();
        assert_eq!(
            first_mtime, second_mtime,
            "existing cached PNG must not be rewritten"
        );
    }

    #[test]
    fn test_default_icon_cache_dir_uses_config_dir() {
        let dir = default_icon_cache_dir();
        assert!(dir.ends_with("antigravity-token-tracker/icons"));
    }
}
