use std::time::Instant;

use kohbor_minimap_cv::{analyze, Blob, PixelReport, Team};

const SIDE: u32 = 256;
const STRIDE: u32 = SIDE * 4;
const DIRE_BGRA: [u8; 4] = [41, 41, 219, 255];
const RADIANT_BGRA: [u8; 4] = [77, 219, 102, 255];

fn crop(stride: u32) -> Vec<u8> {
    let mut pixels = vec![0; (stride * SIDE) as usize];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let border = x < 3 || y < 3 || x >= SIDE - 3 || y >= SIDE - 3;
            let color = if border {
                [6, 9, 5, 255]
            } else if (x / 32 + y / 32) % 2 == 0 {
                [18, 24, 14, 255]
            } else {
                [35, 44, 29, 255]
            };
            put_pixel(&mut pixels, stride, x, y, color);
        }
    }
    pixels
}

fn put_pixel(pixels: &mut [u8], stride: u32, x: u32, y: u32, bgra: [u8; 4]) {
    let offset = (y * stride + x * 4) as usize;
    pixels[offset..offset + 4].copy_from_slice(&bgra);
}

fn paint_ring(pixels: &mut [u8], stride: u32, center: (u32, u32), color: [u8; 4]) {
    for dy in -9_i32..=9 {
        for dx in -9_i32..=9 {
            let distance_squared = dx * dx + dy * dy;
            if (25..=81).contains(&distance_squared) {
                put_pixel(
                    pixels,
                    stride,
                    (center.0 as i32 + dx) as u32,
                    (center.1 as i32 + dy) as u32,
                    color,
                );
            }
        }
    }
}

fn paint_dot(pixels: &mut [u8], stride: u32, center: (u32, u32), color: [u8; 4]) {
    for dy in -4_i32..=4 {
        for dx in -4_i32..=4 {
            if dx * dx + dy * dy <= 16 {
                put_pixel(
                    pixels,
                    stride,
                    (center.0 as i32 + dx) as u32,
                    (center.1 as i32 + dy) as u32,
                    color,
                );
            }
        }
    }
}

fn report(pixels: &[u8], stride: u32, team: Team) -> PixelReport {
    analyze(pixels, SIDE, SIDE, stride, team)
}

#[test]
fn counts_each_synthetic_enemy_ring_from_zero_through_five() {
    let centers = [(50, 50), (110, 50), (170, 50), (50, 150), (110, 150)];
    for count in 0..=5 {
        let mut pixels = crop(STRIDE);
        for &center in centers.iter().take(count) {
            paint_ring(&mut pixels, STRIDE, center, DIRE_BGRA);
        }
        let result = report(&pixels, STRIDE, Team::Dire);
        assert_eq!(result.enemy_blobs.len(), count, "count={count}");
        assert!(result
            .enemy_blobs
            .iter()
            .all(|blob| { blob.x_milli <= 1000 && blob.y_milli <= 1000 && blob.area > 0 }));
    }
}

#[test]
fn ignores_allied_colour_rings() {
    let mut pixels = crop(STRIDE);
    paint_ring(&mut pixels, STRIDE, (50, 50), RADIANT_BGRA);
    assert!(report(&pixels, STRIDE, Team::Dire).enemy_blobs.is_empty());
}

#[test]
fn counts_radiant_enemy_and_ignores_dire_enemy_after_team_swap() {
    let mut pixels = crop(STRIDE);
    paint_ring(&mut pixels, STRIDE, (50, 50), RADIANT_BGRA);
    assert_eq!(report(&pixels, STRIDE, Team::Radiant).enemy_blobs.len(), 1);
    assert!(report(&pixels, STRIDE, Team::Dire).enemy_blobs.is_empty());
}

#[test]
fn rejects_a_creep_size_dot_by_area() {
    let mut pixels = crop(STRIDE);
    paint_dot(&mut pixels, STRIDE, (50, 50), DIRE_BGRA);
    assert!(report(&pixels, STRIDE, Team::Dire).enemy_blobs.is_empty());
}

#[test]
fn honors_non_tight_row_stride() {
    let stride = STRIDE + 12;
    let mut pixels = crop(stride);
    for center in [(50, 50), (110, 50), (170, 50)] {
        paint_ring(&mut pixels, stride, center, DIRE_BGRA);
    }
    assert_eq!(report(&pixels, stride, Team::Dire).enemy_blobs.len(), 3);
}

#[test]
fn uniform_image_is_not_structurally_valid() {
    let pixel_count = (SIDE * SIDE) as usize;
    let pixels = [1, 2, 3, 255].repeat(pixel_count);
    let result = report(&pixels, STRIDE, Team::Dire);
    assert!(!result.structural_ok);
    assert!(result.enemy_blobs.is_empty());
}

#[test]
fn dark_border_and_textured_interior_are_structurally_valid() {
    let result = report(&crop(STRIDE), STRIDE, Team::Dire);
    assert!(result.structural_ok);
}

#[test]
fn malformed_or_empty_buffers_return_no_blobs() {
    let empty = analyze(&[], SIDE, SIDE, STRIDE, Team::Dire);
    assert!(!empty.structural_ok);
    assert!(empty.enemy_blobs.is_empty());

    let short = analyze(&[0; 16], SIDE, SIDE, STRIDE, Team::Dire);
    assert!(!short.structural_ok);
    assert!(short.enemy_blobs.is_empty());

    let bad_stride = analyze(&[0; 64], 4, 4, 15, Team::Dire);
    assert!(!bad_stride.structural_ok);
    assert!(bad_stride.enemy_blobs.is_empty());
}

#[test]
fn oversized_dimensions_do_not_panic_or_create_blobs() {
    let result = analyze(&[], u32::MAX, u32::MAX, u32::MAX, Team::Dire);
    assert!(!result.structural_ok);
    assert!(result.enemy_blobs.is_empty());
}

#[test]
fn timing_smoke_prints_elapsed_nanoseconds_without_a_threshold() {
    let mut pixels = crop(STRIDE);
    paint_ring(&mut pixels, STRIDE, (50, 50), DIRE_BGRA);
    let started = Instant::now();
    let _ = report(&pixels, STRIDE, Team::Dire);
    println!("analyze_ns={}", started.elapsed().as_nanos());
}

#[test]
fn blob_api_remains_publicly_constructible() {
    let _blob = Blob {
        x_milli: 500,
        y_milli: 500,
        area: 160,
    };
}
