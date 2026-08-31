use std::env;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::System::LibraryLoader::{
    BeginUpdateResourceW, EndUpdateResourceW, UpdateResourceW,
};

const RT_ICON: usize = 3;
const RT_GROUP_ICON: usize = 14;
const ICON_SIZES: [u32; 4] = [16, 32, 48, 256];
const COLOR_ICON_GROUP_ID: usize = 1;
const BLACK_ICON_GROUP_ID: usize = 2;
const COLOR_ICON_RESOURCE_BASE: usize = 1;
const BLACK_ICON_RESOURCE_BASE: usize = 101;
const DEEPSEEK_BLUE: [u8; 3] = [0x4d, 0x6b, 0xfe];
const DEEPSEEK_BLACK: [u8; 3] = [0x00, 0x00, 0x00];

// Geometry is extracted from DeepSeek's official logo asset:
// https://github.com/deepseek-ai/DeepSeek-LLM/blob/main/images/logo.svg
const DEEPSEEK_WHALE_PATH: &str = r#"M55.6128 3.47119C55.0175 3.17944 54.7611 3.73535 54.413 4.01782C54.2939 4.10889 54.1932 4.22729 54.0924 4.33667C53.2223 5.26587 52.2057 5.87646 50.8776 5.80347C48.9359 5.69409 47.2781 6.30469 45.8126 7.78979C45.5012 5.9585 44.4663 4.86499 42.8909 4.16357C42.0667 3.79907 41.2332 3.43457 40.6561 2.64185C40.2532 2.07715 40.1432 1.44849 39.9418 0.828857C39.8135 0.455322 39.6853 0.0725098 39.2548 0.00878906C38.7877 -0.0639648 38.6045 0.327637 38.4213 0.655762C37.6886 1.99512 37.4047 3.47119 37.4321 4.96533C37.4962 8.32739 38.9159 11.0059 41.7369 12.9102C42.0575 13.1289 42.1399 13.3474 42.0392 13.6665C41.8468 14.3225 41.6178 14.9602 41.4164 15.6162C41.2881 16.0354 41.0957 16.1265 40.647 15.9441C39.0991 15.2974 37.7618 14.3406 36.5803 13.1836C34.5745 11.2429 32.761 9.10181 30.4988 7.42529C29.9675 7.03345 29.4363 6.66919 28.8867 6.32275C26.5786 4.08154 29.189 2.24097 29.7935 2.02246C30.4254 1.79468 30.0133 1.01099 27.9708 1.02026C25.9283 1.0293 24.0599 1.71265 21.6786 2.62378C21.3306 2.7605 20.9641 2.8606 20.5886 2.94263C18.4271 2.53271 16.1831 2.44141 13.8384 2.70581C9.42371 3.19775 5.89758 5.28418 3.30554 8.84668C0.191406 13.1289 -0.54126 17.9941 0.356323 23.0691C1.29968 28.4172 4.02905 32.8452 8.22388 36.3076C12.5745 39.8972 17.5845 41.6558 23.2997 41.3186C26.771 41.1182 30.6361 40.6536 34.9958 36.9636C36.0948 37.5103 37.2489 37.7288 39.1632 37.8928C40.6378 38.0295 42.0575 37.8201 43.1565 37.5923C44.8784 37.2278 44.7594 35.6333 44.1366 35.3418C39.09 32.9912 40.1981 33.9478 39.1907 33.1733C41.7552 30.1394 45.6204 26.9868 47.1316 16.7732C47.2506 15.9624 47.1499 15.4521 47.1316 14.7961C47.1224 14.3953 47.214 14.2405 47.672 14.1948C48.9359 14.0491 50.1632 13.7029 51.2898 13.0833C54.5596 11.2976 55.8784 8.36377 56.1898 4.84692C56.2357 4.30933 56.1807 3.75342 55.6128 3.47119ZM27.119 35.123C22.2281 31.2783 19.856 30.0117 18.8759 30.0664C17.96 30.1211 18.1249 31.1689 18.3263 31.8523C18.537 32.5264 18.8118 32.9912 19.1964 33.5833C19.462 33.9751 19.6453 34.5581 18.9309 34.9956C17.3555 35.9705 14.6169 34.6675 14.4886 34.6038C11.3014 32.7268 8.63611 30.2485 6.75842 26.8594C4.94495 23.5974 3.89172 20.0989 3.71765 16.3633C3.67188 15.4614 3.9375 15.1423 4.83508 14.9785C6.0166 14.7598 7.23474 14.7141 8.41626 14.8872C13.408 15.6162 17.6577 17.8484 21.2206 21.3835C23.2539 23.397 24.7926 25.8025 26.3772 28.1531C28.0624 30.6494 29.8759 33.0276 32.184 34.9773C32.9991 35.6606 33.6494 36.1799 34.2722 36.5627C32.3947 36.7722 29.2622 36.8179 27.119 35.123ZM29.4637 20.0442C29.4637 19.6433 29.7843 19.3245 30.1874 19.3245C30.2789 19.3245 30.3613 19.3425 30.4346 19.3699C30.5354 19.4065 30.627 19.4612 30.7002 19.543C30.8285 19.6707 30.9017 19.8528 30.9017 20.0442C30.9017 20.4451 30.5812 20.7639 30.1782 20.7639C29.7751 20.7639 29.4637 20.4451 29.4637 20.0442ZM36.7452 23.7798C36.2781 23.9712 35.811 24.135 35.3622 24.1533C34.6661 24.1897 33.9059 23.9072 33.4938 23.561C32.8527 23.0234 32.3947 22.7229 32.2023 21.7844C32.1199 21.3835 32.1656 20.7639 32.239 20.4087C32.4038 19.6433 32.2206 19.1514 31.6803 18.7048C31.2406 18.3403 30.6819 18.2402 30.0682 18.2402C29.8392 18.2402 29.6287 18.1399 29.4729 18.0579C29.2164 17.9304 29.0059 17.6116 29.2073 17.2197C29.2714 17.0923 29.5829 16.7825 29.6561 16.7278C30.4896 16.2539 31.4513 16.4089 32.3397 16.7642C33.1641 17.1013 33.7869 17.7209 34.6844 18.5955C35.6003 19.6523 35.7651 19.9441 36.2872 20.7366C36.6995 21.3562 37.075 21.9939 37.3314 22.7229C37.4871 23.1785 37.2856 23.552 36.7452 23.7798Z"#;

fn main() {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("usage: embed_icon <exe>"));
    if !path.is_file() {
        panic!("executable not found: {}", path.display());
    }

    let mut images = Vec::with_capacity(ICON_SIZES.len());
    let mut black_images = Vec::with_capacity(ICON_SIZES.len());
    for size in ICON_SIZES {
        let (color, black) = generate_icon_pair(size);
        images.push(color);
        black_images.push(black);
    }
    let group = make_group_icon(&images, COLOR_ICON_RESOURCE_BASE);
    let black_group = make_group_icon(&black_images, BLACK_ICON_RESOURCE_BASE);
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

        for (index, image) in black_images.iter().enumerate() {
            if UpdateResourceW(
                update,
                resource_id(RT_ICON),
                resource_id(BLACK_ICON_RESOURCE_BASE + index),
                0,
                image.as_ptr().cast::<c_void>(),
                image.len() as u32,
            ) == 0
            {
                let error = GetLastError();
                EndUpdateResourceW(update, 1);
                panic!("UpdateResourceW(black RT_ICON) failed: {error}");
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
            resource_id(BLACK_ICON_GROUP_ID),
            0,
            black_group.as_ptr().cast::<c_void>(),
            black_group.len() as u32,
        ) == 0
        {
            let error = GetLastError();
            EndUpdateResourceW(update, 1);
            panic!("UpdateResourceW(black RT_GROUP_ICON) failed: {error}");
        }

        if EndUpdateResourceW(update, 0) == 0 {
            panic!("EndUpdateResourceW failed: {}", GetLastError());
        }
    }

    println!("embedded DSH icon into {}", path.display());
}

const WHALE_MIN_X: f32 = -0.54126;
const WHALE_MAX_X: f32 = 56.2357;
const WHALE_MIN_Y: f32 = -0.0639648;
const WHALE_MAX_Y: f32 = 41.6558;
const WHALE_CONTENT_FRACTION: f32 = 0.92;
const WHALE_WIDTH: f32 = WHALE_MAX_X - WHALE_MIN_X;
const WHALE_HEIGHT: f32 = WHALE_MAX_Y - WHALE_MIN_Y;
const WHALE_SCALE: f32 = WHALE_CONTENT_FRACTION / WHALE_WIDTH;
const WHALE_OFFSET_X: f32 = (1.0 - WHALE_CONTENT_FRACTION) / 2.0;
const WHALE_OFFSET_Y: f32 = (1.0 - WHALE_HEIGHT * WHALE_SCALE) / 2.0;
const CUBIC_SUBDIVISIONS: usize = 16;

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
enum PathToken {
    Command(char),
    Number(f32),
}

fn generate_icon_pair(size: u32) -> (Vec<u8>, Vec<u8>) {
    let alpha = generate_alpha_mask(size);
    (
        encode_icon_dib(size, &alpha, false),
        encode_icon_dib(size, &alpha, true),
    )
}

#[cfg(test)]
fn generate_icon_dib(size: u32, black: bool) -> Vec<u8> {
    let alpha = generate_alpha_mask(size);
    encode_icon_dib(size, &alpha, black)
}

fn generate_alpha_mask(size: u32) -> Vec<u8> {
    let contours = whale_contours();
    let mut alpha = vec![0u8; (size * size) as usize];
    for bottom_y in 0..size {
        let y = size - 1 - bottom_y;
        for x in 0..size {
            alpha[(y * size + x) as usize] = sample_opacity(x, y, size, contours);
        }
    }
    alpha
}

fn encode_icon_dib(size: u32, alpha: &[u8], black: bool) -> Vec<u8> {
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

    let [red, green, blue] = if black { DEEPSEEK_BLACK } else { DEEPSEEK_BLUE };
    for bottom_y in 0..size {
        let y = size - 1 - bottom_y;
        for x in 0..size {
            let opacity = alpha[(y * size + x) as usize];
            if opacity == 0 {
                output.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                output.extend_from_slice(&[blue, green, red, opacity]);
            }
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

fn sample_opacity(x: u32, y: u32, size: u32, contours: &[Vec<Point>]) -> u8 {
    const SAMPLES: u32 = 4;
    let mut alpha_sum = 0.0f32;

    for sy in 0..SAMPLES {
        for sx in 0..SAMPLES {
            let px = (x as f32 + (sx as f32 + 0.5) / SAMPLES as f32) / size as f32;
            let py = (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32) / size as f32;
            if whale_contains(Point { x: px, y: py }, contours) {
                alpha_sum += 1.0;
            }
        }
    }

    let count = (SAMPLES * SAMPLES) as f32;
    let alpha = alpha_sum / count;
    if alpha <= f32::EPSILON {
        return 0;
    }
    (alpha * 255.0).round() as u8
}

fn whale_contours() -> &'static [Vec<Point>] {
    static CONTOURS: OnceLock<Vec<Vec<Point>>> = OnceLock::new();
    CONTOURS.get_or_init(parse_whale_contours).as_slice()
}

fn parse_whale_contours() -> Vec<Vec<Point>> {
    let tokens = tokenize_path(DEEPSEEK_WHALE_PATH);
    let mut contours = Vec::new();
    let mut index = 0usize;
    let mut command = None;
    let mut current = Point { x: 0.0, y: 0.0 };
    let mut contour_start = current;

    while index < tokens.len() {
        if let PathToken::Command(next) = tokens[index] {
            command = Some(next);
            index += 1;
            if matches!(next, 'Z' | 'z') {
                current = contour_start;
                command = None;
            }
            continue;
        }

        match command.expect("DeepSeek whale path must begin with a command") {
            'M' => {
                let point = next_point(&tokens, &mut index);
                contour_start = point;
                current = point;
                contours.push(vec![normalize_whale_point(point)]);
                command = Some('L');
            }
            'L' => {
                current = next_point(&tokens, &mut index);
                contours
                    .last_mut()
                    .expect("DeepSeek whale line has no contour")
                    .push(normalize_whale_point(current));
            }
            'C' => {
                let control_1 = next_point(&tokens, &mut index);
                let control_2 = next_point(&tokens, &mut index);
                let end = next_point(&tokens, &mut index);
                let contour = contours
                    .last_mut()
                    .expect("DeepSeek whale curve has no contour");
                append_cubic(contour, current, control_1, control_2, end);
                current = end;
            }
            other => panic!("unsupported DeepSeek whale path command: {other}"),
        }
    }

    assert!(!contours.is_empty(), "DeepSeek whale path is empty");
    contours
}

fn tokenize_path(path: &str) -> Vec<PathToken> {
    let bytes = path.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() || byte == b',' {
            index += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() {
            tokens.push(PathToken::Command(byte as char));
            index += 1;
            continue;
        }

        let start = index;
        if matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
        if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
            index += 1;
            if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
                index += 1;
            }
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
        }
        let number = path[start..index]
            .parse::<f32>()
            .expect("invalid number in DeepSeek whale path");
        tokens.push(PathToken::Number(number));
    }

    tokens
}

fn next_point(tokens: &[PathToken], index: &mut usize) -> Point {
    Point {
        x: next_number(tokens, index),
        y: next_number(tokens, index),
    }
}

fn next_number(tokens: &[PathToken], index: &mut usize) -> f32 {
    match tokens.get(*index) {
        Some(PathToken::Number(value)) => {
            *index += 1;
            *value
        }
        other => panic!("expected DeepSeek whale path number, got {other:?}"),
    }
}

fn append_cubic(
    contour: &mut Vec<Point>,
    start: Point,
    control_1: Point,
    control_2: Point,
    end: Point,
) {
    for step in 1..=CUBIC_SUBDIVISIONS {
        let t = step as f32 / CUBIC_SUBDIVISIONS as f32;
        contour.push(normalize_whale_point(cubic_point(
            start, control_1, control_2, end, t,
        )));
    }
}

fn cubic_point(start: Point, control_1: Point, control_2: Point, end: Point, t: f32) -> Point {
    let inverse = 1.0 - t;
    Point {
        x: inverse.powi(3) * start.x
            + 3.0 * inverse.powi(2) * t * control_1.x
            + 3.0 * inverse * t.powi(2) * control_2.x
            + t.powi(3) * end.x,
        y: inverse.powi(3) * start.y
            + 3.0 * inverse.powi(2) * t * control_1.y
            + 3.0 * inverse * t.powi(2) * control_2.y
            + t.powi(3) * end.y,
    }
}

fn normalize_whale_point(point: Point) -> Point {
    Point {
        x: WHALE_OFFSET_X + (point.x - WHALE_MIN_X) * WHALE_SCALE,
        y: WHALE_OFFSET_Y + (point.y - WHALE_MIN_Y) * WHALE_SCALE,
    }
}

fn whale_contains(point: Point, contours: &[Vec<Point>]) -> bool {
    let mut winding = 0i32;
    for contour in contours {
        for index in 0..contour.len() {
            let start = contour[index];
            let end = contour[(index + 1) % contour.len()];
            let is_left =
                (end.x - start.x) * (point.y - start.y) - (point.x - start.x) * (end.y - start.y);
            if start.y <= point.y {
                if end.y > point.y && is_left > 0.0 {
                    winding += 1;
                }
            } else if end.y <= point.y && is_left < 0.0 {
                winding -= 1;
            }
        }
    }
    winding != 0
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
    fn official_whale_path_has_four_contours() {
        let contours = whale_contours();
        assert_eq!(contours.len(), 4);
        assert!(contours.iter().all(|contour| contour.len() > 2));
    }

    #[test]
    fn black_icon_preserves_alpha_and_uses_official_colors() {
        let color = generate_icon_dib(16, false);
        let black = generate_icon_dib(16, true);
        let color_pixels = &color[40..40 + 16 * 16 * 4];
        let black_pixels = &black[40..40 + 16 * 16 * 4];
        let mut visible_pixel_found = false;
        for (color_pixel, black_pixel) in color_pixels
            .chunks_exact(4)
            .zip(black_pixels.chunks_exact(4))
        {
            assert_eq!(color_pixel[3], black_pixel[3]);
            if black_pixel[3] > 0 {
                visible_pixel_found = true;
                assert_eq!(&color_pixel[0..3], &[0xfe, 0x6b, 0x4d]);
                assert_eq!(&black_pixel[0..3], &[0x00, 0x00, 0x00]);
            } else {
                assert_eq!(&color_pixel[0..3], &[0x00, 0x00, 0x00]);
                assert_eq!(&black_pixel[0..3], &[0x00, 0x00, 0x00]);
            }
        }
        assert!(visible_pixel_found);
    }
}
