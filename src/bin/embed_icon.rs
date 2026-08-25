use std::env;
use std::ffi::c_void;
use std::path::PathBuf;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
};

const RT_ICON: usize = 3;
const RT_GROUP_ICON: usize = 14;
const ICON_SIZES: [u32; 4] = [16, 32, 48, 256];
const COLOR_ICON_GROUP_ID: usize = 1;
const GRAYSCALE_ICON_GROUP_ID: usize = 2;
const COLOR_ICON_RESOURCE_BASE: usize = 1;
const GRAYSCALE_ICON_RESOURCE_BASE: usize = 101;

fn main() {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("usage: embed_icon <exe>"));
    if !path.is_file() {
        panic!("executable not found: {}", path.display());
    }

    let images: Vec<Vec<u8>> = ICON_SIZES
        .into_iter()
        .map(|size| generate_icon_dib(size, false))
        .collect();
    let grayscale_images: Vec<Vec<u8>> = ICON_SIZES
        .into_iter()
        .map(|size| generate_icon_dib(size, true))
        .collect();
    let group = make_group_icon(&images, COLOR_ICON_RESOURCE_BASE);
    let grayscale_group = make_group_icon(&grayscale_images, GRAYSCALE_ICON_RESOURCE_BASE);
    let path_wide = to_wide(&path.to_string_lossy());

    unsafe {
        let update = BeginUpdateResourceW(path_wide.as_ptr(), 0);
        if update.is_null() {
            panic!("BeginUpdateResourceW failed: {}", GetLastError());
        }

        for (index, image) in images.iter().enumerate() {
            if UpdateResourceW(
                update,
                resource_id(RT_ICON),
                resource_id(COLOR_ICON_RESOURCE_BASE + index),
                0,
                image.as_ptr().cast::<c_void>(),
                image.len() as u32,
            ) == 0
            {
                let error = GetLastError();
                EndUpdateResourceW(update, 1);
                panic!("UpdateResourceW(RT_ICON) failed: {error}");
            }
        }

        for (index, image) in grayscale_images.iter().enumerate() {
            if UpdateResourceW(
                update,
                resource_id(RT_ICON),
                resource_id(GRAYSCALE_ICON_RESOURCE_BASE + index),
                0,
                image.as_ptr().cast::<c_void>(),
                image.len() as u32,
            ) == 0
            {
                let error = GetLastError();
                EndUpdateResourceW(update, 1);
                panic!("UpdateResourceW(grayscale RT_ICON) failed: {error}");
            }
        }

        if UpdateResourceW(
            update,
            resource_id(RT_GROUP_ICON),
            resource_id(COLOR_ICON_GROUP_ID),
            0,
            group.as_ptr().cast::<c_void>(),
            group.len() as u32,
        ) == 0
        {
            let error = GetLastError();
            EndUpdateResourceW(update, 1);
            panic!("UpdateResourceW(RT_GROUP_ICON) failed: {error}");
        }

        if UpdateResourceW(
            update,
            resource_id(RT_GROUP_ICON),
            resource_id(GRAYSCALE_ICON_GROUP_ID),
            0,
            grayscale_group.as_ptr().cast::<c_void>(),
            grayscale_group.len() as u32,
        ) == 0
        {
            let error = GetLastError();
            EndUpdateResourceW(update, 1);
            panic!("UpdateResourceW(grayscale RT_GROUP_ICON) failed: {error}");
        }

        if EndUpdateResourceW(update, 0) == 0 {
            panic!("EndUpdateResourceW failed: {}", GetLastError());
        }
    }

    println!("embedded DSH icon into {}", path.display());
}

fn generate_icon_dib(size: u32, grayscale: bool) -> Vec<u8> {
    let mut output = Vec::with_capacity((40 + size * size * 4) as usize);
    push_u32(&mut output, 40);
    push_i32(&mut output, size as i32);
    push_i32(&mut output, (size * 2) as i32);
    push_u16(&mut output, 1);
    push_u16(&mut output, 32);
    push_u32(&mut output, 0);
    push_u32(&mut output, size * size * 4);
    push_i32(&mut output, 0);
    push_i32(&mut output, 0);
    push_u32(&mut output, 0);
    push_u32(&mut output, 0);

    let mut alpha = vec![0u8; (size * size) as usize];
    for bottom_y in 0..size {
        let y = size - 1 - bottom_y;
        for x in 0..size {
            let [red, green, blue, opacity] = sample_pixel(x, y, size, grayscale);
            output.extend_from_slice(&[blue, green, red, opacity]);
            alpha[(y * size + x) as usize] = opacity;
        }
    }

    let mask_row_bytes = size.div_ceil(32) * 4;
    for bottom_y in 0..size {
        let y = size - 1 - bottom_y;
        let mut row = vec![0u8; mask_row_bytes as usize];
        for x in 0..size {
            if alpha[(y * size + x) as usize] < 128 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        output.extend_from_slice(&row);
    }
    output
}

fn sample_pixel(x: u32, y: u32, size: u32, grayscale: bool) -> [u8; 4] {
    const SAMPLES: u32 = 4;
    let mut alpha_sum = 0.0f32;
    let mut red_sum = 0.0f32;
    let mut green_sum = 0.0f32;
    let mut blue_sum = 0.0f32;

    for sy in 0..SAMPLES {
        for sx in 0..SAMPLES {
            let px = (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
            let py = (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
            let [red, green, blue, alpha] = vector_color(px, py);
            alpha_sum += alpha;
            red_sum += red * alpha;
            green_sum += green * alpha;
            blue_sum += blue * alpha;
        }
    }

    let count = (SAMPLES * SAMPLES) as f32;
    let alpha = alpha_sum / count;
    if alpha <= f32::EPSILON {
        return [0, 0, 0, 0];
    }
    let [red, green, blue] = [
        (red_sum / alpha_sum).round() as u8,
        (green_sum / alpha_sum).round() as u8,
        (blue_sum / alpha_sum).round() as u8,
    ];
    if grayscale {
        let value = grayscale_value(red, green, blue);
        [value, value, value, (alpha * 255.0).round() as u8]
    } else {
        [red, green, blue, (alpha * 255.0).round() as u8]
    }
}

fn grayscale_value(red: u8, green: u8, blue: u8) -> u8 {
    ((u32::from(red) * 77 + u32::from(green) * 150 + u32::from(blue) * 29 + 128) / 256) as u8
}

fn vector_color(x: f32, y: f32) -> [f32; 4] {
    let margin = 0.055;
    let radius = 0.19;
    let nearest_x = x.clamp(margin + radius, 1.0 - margin - radius);
    let nearest_y = y.clamp(margin + radius, 1.0 - margin - radius);
    let dx = x - nearest_x;
    let dy = y - nearest_y;
    if dx * dx + dy * dy > radius * radius {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let gradient = y.clamp(0.0, 1.0);
    let mut red = 11.0 + 10.0 * gradient;
    let mut green = 34.0 + 17.0 * gradient;
    let mut blue = 58.0 + 20.0 * gradient;
    if x + y > 1.12 {
        let blend = ((x + y - 1.12) / 0.65).clamp(0.0, 0.42);
        red = red * (1.0 - blend) + 20.0 * blend;
        green = green * (1.0 - blend) + 194.0 * blend;
        blue = blue * (1.0 - blend) + 157.0 * blend;
    }

    let bar = (0.255..=0.355).contains(&x) && (0.22..=0.78).contains(&y);
    let outer = ((x - 0.40) / 0.305).powi(2) + ((y - 0.50) / 0.285).powi(2) <= 1.0;
    let inner = ((x - 0.40) / 0.17).powi(2) + ((y - 0.50) / 0.155).powi(2) < 1.0;
    let bowl = outer && !inner && x >= 0.31;
    if bar || bowl {
        [240.0, 249.0, 255.0, 1.0]
    } else {
        [red, green, blue, 1.0]
    }
}

fn make_group_icon(images: &[Vec<u8>], resource_base: usize) -> Vec<u8> {
    let mut group = Vec::with_capacity(6 + images.len() * 14);
    push_u16(&mut group, 0);
    push_u16(&mut group, 1);
    push_u16(&mut group, images.len() as u16);
    for (index, (size, image)) in ICON_SIZES.iter().zip(images).enumerate() {
        group.push(if *size == 256 { 0 } else { *size as u8 });
        group.push(if *size == 256 { 0 } else { *size as u8 });
        group.push(0);
        group.push(0);
        push_u16(&mut group, 1);
        push_u16(&mut group, 32);
        push_u32(&mut group, image.len() as u32);
        push_u16(&mut group, (resource_base + index) as u16);
    }
    group
}

fn resource_id(id: usize) -> *const u16 {
    id as *const u16
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_dib_has_header_pixels_and_mask() {
        let icon = generate_icon_dib(16, false);
        assert_eq!(&icon[0..4], &40u32.to_le_bytes());
        assert!(icon.len() > 40 + 16 * 16 * 4);
    }

    #[test]
    fn group_icon_references_all_images() {
        let images: Vec<Vec<u8>> = ICON_SIZES
            .into_iter()
            .map(|size| generate_icon_dib(size, false))
            .collect();
        let group = make_group_icon(&images, COLOR_ICON_RESOURCE_BASE);
        assert_eq!(&group[4..6], &(images.len() as u16).to_le_bytes());
        assert_eq!(group.len(), 6 + images.len() * 14);
    }

    #[test]
    fn grayscale_icon_pixels_have_equal_channels() {
        let color = generate_icon_dib(16, false);
        let grayscale = generate_icon_dib(16, true);
        let color_pixels = &color[40..40 + 16 * 16 * 4];
        let grayscale_pixels = &grayscale[40..40 + 16 * 16 * 4];
        for (color_pixel, grayscale_pixel) in color_pixels
            .chunks_exact(4)
            .zip(grayscale_pixels.chunks_exact(4))
        {
            assert_eq!(color_pixel[3], grayscale_pixel[3]);
            assert_eq!(grayscale_pixel[0], grayscale_pixel[1]);
            assert_eq!(grayscale_pixel[1], grayscale_pixel[2]);
        }
    }

    #[test]
    fn grayscale_conversion_uses_luminance() {
        assert_eq!(grayscale_value(255, 0, 0), 77);
        assert_eq!(grayscale_value(0, 255, 0), 149);
        assert_eq!(grayscale_value(0, 0, 255), 29);
    }
}
