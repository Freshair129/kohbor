use std::time::Instant;

use kohbor_minimap_cv::{propose_frame, FrameProposal};

const BYTES_PER_PIXEL: u32 = 4;
const FRAME_BORDER: u32 = 3;
const BORDER_BGRA: [u8; 4] = [170, 170, 170, 255];

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn noisy_image(width: u32, height: u32, stride: u32) -> Vec<u8> {
    let mut pixels = vec![0; (stride * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let noise = ((x.wrapping_mul(37) ^ y.wrapping_mul(71) ^ x.wrapping_mul(y)) % 29) as u8;
            put_pixel(
                &mut pixels,
                stride,
                x,
                y,
                [35 + noise, 55 + noise, 83 + noise, 255],
            );
        }
    }
    pixels
}

fn put_pixel(pixels: &mut [u8], stride: u32, x: u32, y: u32, bgra: [u8; 4]) {
    let offset = (y * stride + x * BYTES_PER_PIXEL) as usize;
    pixels[offset..offset + BYTES_PER_PIXEL as usize].copy_from_slice(&bgra);
}

fn paint_frame(pixels: &mut [u8], stride: u32, rect: Rect, border: [u8; 4]) {
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let on_border = x < rect.x + FRAME_BORDER
                || y < rect.y + FRAME_BORDER
                || x >= rect.x + rect.width - FRAME_BORDER
                || y >= rect.y + rect.height - FRAME_BORDER;
            let pixel = if on_border {
                border
            } else if ((x / 8) + (y / 8)) % 2 == 0 {
                [22, 22, 22, 255]
            } else {
                [48, 48, 48, 255]
            };
            put_pixel(pixels, stride, x, y, pixel);
        }
    }
}

fn assert_near(proposal: FrameProposal, expected: Rect) {
    assert!(
        (proposal.x - expected.x as i32).abs() <= 2,
        "proposal={proposal:?}"
    );
    assert!(
        (proposal.y - expected.y as i32).abs() <= 2,
        "proposal={proposal:?}"
    );
    assert!(
        proposal.width.abs_diff(expected.width) <= 2,
        "proposal={proposal:?}"
    );
    assert!(
        proposal.height.abs_diff(expected.height) <= 2,
        "proposal={proposal:?}"
    );
}

#[test]
fn detects_owner_layout_margins_on_noisy_backdrop() {
    let width = 292;
    let height = 292;
    let stride = width * BYTES_PER_PIXEL;
    let mut pixels = noisy_image(width, height, stride);
    let expected = Rect {
        x: 0,
        y: 12,
        width: 280,
        height: 274,
    };
    paint_frame(&mut pixels, stride, expected, BORDER_BGRA);

    let proposal = propose_frame(&pixels, width, height, stride).expect("frame proposal");
    assert_near(proposal, expected);
    assert!(proposal.confidence_milli <= 1000);
}

#[test]
fn detects_several_other_offsets_on_noisy_backdrops() {
    let cases = [
        (
            310,
            300,
            Rect {
                x: 8,
                y: 20,
                width: 225,
                height: 220,
            },
        ),
        (
            330,
            305,
            Rect {
                x: 17,
                y: 7,
                width: 260,
                height: 250,
            },
        ),
        (
            300,
            270,
            Rect {
                x: 24,
                y: 18,
                width: 220,
                height: 215,
            },
        ),
    ];

    for (width, height, expected) in cases {
        let stride = width * BYTES_PER_PIXEL;
        let mut pixels = noisy_image(width, height, stride);
        paint_frame(&mut pixels, stride, expected, BORDER_BGRA);
        let proposal = propose_frame(&pixels, width, height, stride).expect("frame proposal");
        assert_near(proposal, expected);
    }
}

#[test]
fn detects_frame_touching_all_image_edges() {
    let width = 230;
    let height = 224;
    let stride = width * BYTES_PER_PIXEL;
    let mut pixels = noisy_image(width, height, stride);
    let expected = Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    paint_frame(&mut pixels, stride, expected, BORDER_BGRA);

    let proposal = propose_frame(&pixels, width, height, stride).expect("edge-touching frame");
    assert_near(proposal, expected);
}

#[test]
fn returns_none_for_low_contrast_frame() {
    let width = 270;
    let height = 265;
    let stride = width * BYTES_PER_PIXEL;
    let mut pixels = noisy_image(width, height, stride);
    let expected = Rect {
        x: 7,
        y: 5,
        width: 250,
        height: 245,
    };
    paint_frame(&mut pixels, stride, expected, [51, 51, 51, 255]);

    let proposal = propose_frame(&pixels, width, height, stride);
    assert!(proposal.is_none() || proposal.is_some_and(|value| value.confidence_milli < 250));
}

#[test]
fn uniform_image_has_no_proposal() {
    let width = 292;
    let height = 292;
    let stride = width * BYTES_PER_PIXEL;
    let pixels = [44, 44, 44, 255].repeat((width * height) as usize);

    assert_eq!(propose_frame(&pixels, width, height, stride), None);
}

#[test]
fn selects_the_larger_of_two_consistent_rectangles() {
    let width = 500;
    let height = 320;
    let stride = width * BYTES_PER_PIXEL;
    let mut pixels = noisy_image(width, height, stride);
    let larger = Rect {
        x: 12,
        y: 10,
        width: 300,
        height: 290,
    };
    let smaller = Rect {
        x: 332,
        y: 145,
        width: 150,
        height: 150,
    };
    paint_frame(&mut pixels, stride, larger, BORDER_BGRA);
    paint_frame(&mut pixels, stride, smaller, BORDER_BGRA);

    let proposal = propose_frame(&pixels, width, height, stride).expect("one consistent frame");
    assert_near(proposal, larger);
}

#[test]
fn handles_non_tight_stride() {
    let width = 300;
    let height = 292;
    let stride = width * BYTES_PER_PIXEL + 28;
    let mut pixels = noisy_image(width, height, stride);
    let expected = Rect {
        x: 6,
        y: 11,
        width: 280,
        height: 274,
    };
    paint_frame(&mut pixels, stride, expected, BORDER_BGRA);

    let proposal = propose_frame(&pixels, width, height, stride).expect("frame with padded rows");
    assert_near(proposal, expected);
}

#[test]
fn tiny_odd_empty_and_oversized_inputs_fail_closed() {
    assert_eq!(propose_frame(&[], 0, 0, 0), None);
    assert_eq!(propose_frame(&[0; 16], 2, 2, 8), None);
    assert_eq!(propose_frame(&[0; 64], 151, 149, 604), None);
    assert_eq!(propose_frame(&[], u32::MAX, u32::MAX, u32::MAX), None);
}

#[test]
fn rejects_invalid_stride_and_short_buffer() {
    let width = 160;
    let height = 160;
    assert_eq!(
        propose_frame(&[0; 1024], width, height, width * 4 - 1),
        None
    );
    assert_eq!(propose_frame(&[0; 1024], width, height, width * 4), None);
}

#[test]
fn accepts_the_minimum_configured_square_size() {
    let width = 164;
    let height = 160;
    let stride = width * BYTES_PER_PIXEL;
    let mut pixels = noisy_image(width, height, stride);
    let expected = Rect {
        x: 10,
        y: 5,
        width: 150,
        height: 150,
    };
    paint_frame(&mut pixels, stride, expected, BORDER_BGRA);

    let proposal = propose_frame(&pixels, width, height, stride).expect("minimum-size frame");
    assert_near(proposal, expected);
}

#[test]
fn rejects_rectangles_outside_the_aspect_range() {
    let width = 300;
    let height = 180;
    let stride = width * BYTES_PER_PIXEL;
    let mut pixels = noisy_image(width, height, stride);
    let too_wide = Rect {
        x: 15,
        y: 10,
        width: 250,
        height: 150,
    };
    paint_frame(&mut pixels, stride, too_wide, BORDER_BGRA);

    assert_eq!(propose_frame(&pixels, width, height, stride), None);
}

#[test]
fn timing_smoke_prints_elapsed_nanoseconds_without_a_threshold() {
    let width = 292;
    let height = 292;
    let stride = width * BYTES_PER_PIXEL;
    let mut pixels = noisy_image(width, height, stride);
    paint_frame(
        &mut pixels,
        stride,
        Rect {
            x: 0,
            y: 12,
            width: 280,
            height: 274,
        },
        BORDER_BGRA,
    );
    let started = Instant::now();
    let proposal = propose_frame(&pixels, width, height, stride);
    println!("propose_frame_ns={}", started.elapsed().as_nanos());
    assert!(proposal.is_some());
}
