//! Region capture: BMP writer + screen capture (see `selector.rs`).

use std::path::Path;

/// Write a 32-bit BMP (bottom-up, BGRX). `rgba` is top-down RGBA.
/// Used both for the builtin selector capture and the flameshot clipboard
/// image (rapidocr's cv2 reads BMP fine, so no PNG encoder is needed).
pub fn write_bmp(path: &Path, width: u32, height: u32, rgba: &[u8]) -> bool {
    let w = width as i32;
    let h = height as i32;
    let row = width * 4;
    let img_size = row * height;
    let mut file = Vec::with_capacity(54 + img_size as usize);

    // BITMAPFILEHEADER
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&(54u32 + img_size).to_le_bytes());
    file.extend_from_slice(&[0u8; 4]);
    file.extend_from_slice(&54u32.to_le_bytes());
    // BITMAPINFOHEADER (positive height = bottom-up)
    file.extend_from_slice(&40u32.to_le_bytes()); // biSize
    file.extend_from_slice(&(w as u32).to_le_bytes()); // biWidth
    file.extend_from_slice(&(h as u32).to_le_bytes()); // biHeight
    file.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    file.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    file.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    file.extend_from_slice(&img_size.to_le_bytes()); // biSizeImage
    file.extend_from_slice(&2835u32.to_le_bytes()); // biXPelsPerMeter
    file.extend_from_slice(&2835u32.to_le_bytes()); // biYPelsPerMeter
    file.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    file.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    // Pixels: source is top-down RGBA -> bottom-up BGRA.
    for y in (0..height as usize).rev() {
        let start = y * row as usize;
        let row_bytes = &rgba[start..start + row as usize];
        for px in row_bytes.as_chunks::<4>().0 {
            file.push(px[2]); // B
            file.push(px[1]); // G
            file.push(px[0]); // R
            file.push(0xff); // X
        }
    }
    std::fs::write(path, file).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_roundtrip_layout() {
        // 2x2, top-down RGBA: red, green / blue, white.
        let rgba = [
            255, 0, 0, 255, 0, 255, 0, 255, // top row
            0, 0, 255, 255, 255, 255, 255, 255, // bottom row
        ];
        let path = std::env::temp_dir().join(format!("screenocr_test_{}.bmp", std::process::id()));
        assert!(write_bmp(&path, 2, 2, &rgba));
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(&bytes[0..2], b"BM");
        assert_eq!(u32::from_le_bytes(bytes[2..6].try_into().unwrap()), 54 + 16);
        assert_eq!(u32::from_le_bytes(bytes[18..22].try_into().unwrap()), 2); // width
        assert_eq!(u32::from_le_bytes(bytes[22..26].try_into().unwrap()), 2); // height
                                                                              // Bottom-up: first stored row is the blue/white row, BGRA order.
        assert_eq!(&bytes[54..58], &[255, 0, 0, 255]); // blue -> BGRA(B=255,G=0,R=0)
        assert_eq!(&bytes[58..62], &[255, 255, 255, 255]); // white
        assert_eq!(&bytes[62..66], &[0, 0, 255, 255]); // red -> BGRA(R=255)
    }
}

#[path = "capture_selector.rs"]
#[cfg(windows)]
mod selector;

/// Run the overlay selector and capture the chosen region as BMP.
/// Returns false when the user cancels (Esc or too-small drag).
#[cfg(windows)]
pub fn select_and_capture(path: &Path) -> bool {
    let Some(rect) = selector::select_region() else {
        return false;
    };
    selector::capture_rect(rect, path)
}
