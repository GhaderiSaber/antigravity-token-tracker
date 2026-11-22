use image::{ImageBuffer, Rgba};
use ksni::Icon;
use std::path::PathBuf;

pub fn default_icon_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".config")
        .join("antigravity-token-tracker")
        .join("icons")
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

pub fn render_tray_icon(pct: f64, is_exhausted: bool) -> (Icon, String) {
    let width = 48;
    let height = 48;
    let center_x = 24.0f32;
    let center_y = 24.0f32;

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
    let mut img_buf = ImageBuffer::new(width as u32, height as u32);

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
                // Lightning bolt
                pixel_a = 255;
                pixel_r = 255;
                pixel_g = 255;
                pixel_b = 255;
            } else if (15.0..=22.0).contains(&dist) {
                // Ring track (thickened for crisp downsampling on 16x16 / 22x22 panels)
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

            // ksni expects ARGB32
            argb_data.push(pixel_a);
            argb_data.push(pixel_r);
            argb_data.push(pixel_g);
            argb_data.push(pixel_b);

            // image crate expects RGBA
            img_buf.put_pixel(
                x as u32,
                y as u32,
                Rgba([pixel_r, pixel_g, pixel_b, pixel_a]),
            );
        }
    }

    let icon_name = if is_exhausted {
        "quota_exhausted".to_string()
    } else {
        format!("quota_{}", pct.round() as i64)
    };

    // Save PNG to disk for caching / theme fallback
    let cache_dir = default_icon_cache_dir();
    let _ = std::fs::create_dir_all(&cache_dir);
    let png_path = cache_dir.join(format!("{icon_name}.png"));
    let _ = img_buf.save(&png_path);

    let icon = Icon {
        width: width as i32,
        height: height as i32,
        data: argb_data,
    };

    (icon, icon_name)
}
