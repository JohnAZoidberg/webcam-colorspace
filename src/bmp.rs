use std::path::Path;

/// Write an RGB24 image as a BMP file.
///
/// Uses a 14-byte file header + 40-byte BITMAPINFOHEADER.
/// Negative biHeight for top-down row order. Rows padded to 4-byte boundary.
pub fn write_bmp(path: &Path, width: u32, height: u32, rgb_data: &[u8]) -> anyhow::Result<()> {
    let row_stride = (width * 3 + 3) & !3; // pad each row to 4-byte boundary
    let pixel_data_size = row_stride * height;
    let file_size = 14 + 40 + pixel_data_size;

    let mut buf = Vec::with_capacity(file_size as usize);

    // -- File header (14 bytes) --
    buf.extend_from_slice(b"BM"); // signature
    buf.extend_from_slice(&file_size.to_le_bytes()); // file size
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved1
    buf.extend_from_slice(&0u16.to_le_bytes()); // reserved2
    buf.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset (14 + 40)

    // -- BITMAPINFOHEADER (40 bytes) --
    buf.extend_from_slice(&40u32.to_le_bytes()); // header size
    buf.extend_from_slice(&width.to_le_bytes()); // biWidth
    buf.extend_from_slice(&(-(height as i32)).to_le_bytes()); // biHeight (negative = top-down)
    buf.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    buf.extend_from_slice(&24u16.to_le_bytes()); // biBitCount (24-bit RGB)
    buf.extend_from_slice(&0u32.to_le_bytes()); // biCompression (BI_RGB)
    buf.extend_from_slice(&pixel_data_size.to_le_bytes()); // biSizeImage
    buf.extend_from_slice(&2835u32.to_le_bytes()); // biXPelsPerMeter (~72 DPI)
    buf.extend_from_slice(&2835u32.to_le_bytes()); // biYPelsPerMeter
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    // -- Pixel data (BGR, padded rows) --
    let pad_bytes = (row_stride - width * 3) as usize;
    for row in 0..height as usize {
        for col in 0..width as usize {
            let src = (row * width as usize + col) * 3;
            let r = rgb_data[src];
            let g = rgb_data[src + 1];
            let b = rgb_data[src + 2];
            buf.push(b); // BMP stores BGR
            buf.push(g);
            buf.push(r);
        }
        buf.extend(std::iter::repeat_n(0u8, pad_bytes));
    }

    std::fs::write(path, &buf)?;
    Ok(())
}
