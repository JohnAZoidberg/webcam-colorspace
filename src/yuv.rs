pub struct YuvMatrix {
    pub name: &'static str,
    pub kr: f64,
    pub kb: f64,
}

pub const BT601: YuvMatrix = YuvMatrix {
    name: "BT.601",
    kr: 0.299,
    kb: 0.114,
};

pub const BT709: YuvMatrix = YuvMatrix {
    name: "BT.709",
    kr: 0.2126,
    kb: 0.0722,
};

/// Convert NV12 frame to RGB24.
///
/// NV12 layout: Y plane (width * height bytes), then interleaved UV plane (width * height/2 bytes).
/// `full_range`: true = Y/UV 0–255; false = limited range Y 16–235, UV 16–240.
pub fn nv12_to_rgb24(
    data: &[u8],
    width: u32,
    height: u32,
    matrix: &YuvMatrix,
    full_range: bool,
) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let y_plane = &data[..w * h];
    let uv_plane = &data[w * h..];

    let kg = 1.0 - matrix.kr - matrix.kb;

    // Range parameters
    let (y_offset, y_scale, uv_scale) = if full_range {
        (0.0, 255.0, 255.0)
    } else {
        (16.0, 219.0, 224.0)
    };

    let mut rgb = vec![0u8; w * h * 3];

    for row in 0..h {
        for col in 0..w {
            let y_idx = row * w + col;
            let uv_row = row / 2;
            let uv_col = (col / 2) * 2; // each UV pair covers 2 pixels
            let uv_idx = uv_row * w + uv_col;

            let y = (y_plane[y_idx] as f64 - y_offset) / y_scale;
            let cb = (uv_plane[uv_idx] as f64 - 128.0) / uv_scale;
            let cr = (uv_plane[uv_idx + 1] as f64 - 128.0) / uv_scale;

            let r = y + (2.0 * (1.0 - matrix.kr)) * cr;
            let g = y
                - (2.0 * (1.0 - matrix.kb) * matrix.kb / kg) * cb
                - (2.0 * (1.0 - matrix.kr) * matrix.kr / kg) * cr;
            let b = y + (2.0 * (1.0 - matrix.kb)) * cb;

            let out_idx = y_idx * 3;
            rgb[out_idx] = clamp_u8(r * 255.0);
            rgb[out_idx + 1] = clamp_u8(g * 255.0);
            rgb[out_idx + 2] = clamp_u8(b * 255.0);
        }
    }

    rgb
}

fn clamp_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}
