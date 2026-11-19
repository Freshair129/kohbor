//! Stage-trace observer: the traced detector must return exactly what the existing filtered
//! detector returns, and its counters must classify the stage that ended the pass.
use kohbor_minimap_cv::{
    propose_frame_above_confidence, propose_frame_above_confidence_traced, CvTerminal,
    FrameProposal, StageTrace,
};

const BYTES_PER_PIXEL: u32 = 4;
const FRAME_BORDER: u32 = 3;
const MIN_CONFIDENCE: u16 = 600;

#[derive(Clone, Copy)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn put_pixel(pixels: &mut [u8], stride: u32, x: u32, y: u32, bgra: [u8; 4]) {
    let offset = (y * stride + x * BYTES_PER_PIXEL) as usize;
    pixels[offset..offset + BYTES_PER_PIXEL as usize].copy_from_slice(&bgra);
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

fn paint_frame(pixels: &mut [u8], stride: u32, rect: Rect, border: [u8; 4], texture: [u8; 2]) {
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let on_border = x < rect.x + FRAME_BORDER
                || y < rect.y + FRAME_BORDER
                || x >= rect.x + rect.width - FRAME_BORDER
                || y >= rect.y + rect.height - FRAME_BORDER;
            let pixel = if on_border {
                border
            } else if ((x / 8) + (y / 8)) % 2 == 0 {
                [texture[0], texture[0], texture[0], 255]
            } else {
                [texture[1], texture[1], texture[1], 255]
            };
            put_pixel(pixels, stride, x, y, pixel);
        }
    }
}

const SIDE: u32 = 328;
const STRIDE: u32 = SIDE * BYTES_PER_PIXEL;
const OUTER: Rect = Rect {
    x: 0,
    y: 48,
    width: 280,
    height: 280,
};
const INNER: Rect = Rect {
    x: 30,
    y: 108,
    width: 160,
    height: 160,
};

/// Native-sized search: grey outer frame (scores <= 600) with a strong inner frame (1000).
fn nested_frame() -> Vec<u8> {
    let mut pixels = [35, 55, 83, 255].repeat((SIDE * SIDE) as usize);
    paint_frame(&mut pixels, STRIDE, OUTER, [80, 80, 80, 255], [22, 48]);
    paint_frame(&mut pixels, STRIDE, INNER, [255; 4], [0, 100]);
    pixels
}

/// Bright-grey outer frame on a noisy backdrop: the unfiltered engine scores 379.
fn marginal_frame() -> Vec<u8> {
    let mut pixels = noisy_image(SIDE, SIDE, STRIDE);
    paint_frame(&mut pixels, STRIDE, OUTER, [170, 170, 170, 255], [22, 48]);
    pixels
}

fn dark_frame() -> Vec<u8> {
    let mut pixels = noisy_image(SIDE, SIDE, STRIDE);
    paint_frame(&mut pixels, STRIDE, OUTER, [20, 20, 20, 255], [22, 48]);
    pixels
}

fn uniform_frame() -> Vec<u8> {
    [35, 55, 83, 255].repeat((SIDE * SIDE) as usize)
}

fn traced(
    pixels: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    min: u16,
) -> (Option<FrameProposal>, StageTrace) {
    let mut trace = StageTrace::default();
    let proposal =
        propose_frame_above_confidence_traced(pixels, width, height, stride, min, &mut trace);
    (proposal, trace)
}

#[test]
fn observer_never_changes_the_detector_result() {
    let frames = [
        nested_frame(),
        marginal_frame(),
        dark_frame(),
        uniform_frame(),
    ];
    for (index, pixels) in frames.iter().enumerate() {
        for min in [0, 379, 380, 599, MIN_CONFIDENCE, 601, 999, 1000] {
            let plain = propose_frame_above_confidence(pixels, SIDE, SIDE, STRIDE, min);
            let (observed, _) = traced(pixels, SIDE, SIDE, STRIDE, min);
            assert_eq!(plain, observed, "frame {index} min {min}");
        }
    }
    // Malformed shapes fail closed identically.
    let pixels = nested_frame();
    for (width, height, stride) in [
        (SIDE, SIDE, 10),
        (SIDE, SIDE + 1, STRIDE),
        (0, 0, 0),
        (100, 100, 400),
    ] {
        let plain = propose_frame_above_confidence(&pixels, width, height, stride, MIN_CONFIDENCE);
        let (observed, _) = traced(&pixels, width, height, stride, MIN_CONFIDENCE);
        assert_eq!(plain, observed, "shape {width}x{height} stride {stride}");
    }
}

#[test]
fn proposed_pass_reports_eligible_candidates_and_the_top_score() {
    let (proposal, trace) = traced(&nested_frame(), SIDE, SIDE, STRIDE, MIN_CONFIDENCE);
    let proposal = proposal.expect("inner frame");
    assert_eq!(proposal.confidence_milli, 1000);
    assert_eq!(trace.terminal(true), CvTerminal::Proposed);
    assert!(trace.eligible >= 1, "{trace:?}");
    assert!(trace.scores_produced >= trace.eligible, "{trace:?}");
    assert_eq!(trace.max_score_milli, 1000);
    assert!(
        trace.horizontal_edges > 0 && trace.vertical_edges > 0,
        "{trace:?}"
    );
}

#[test]
fn score_equal_to_the_minimum_is_not_above_it() {
    let (proposal, trace) = traced(&nested_frame(), SIDE, SIDE, STRIDE, 1000);
    assert_eq!(proposal, None);
    assert_eq!(trace.terminal(false), CvTerminal::ConfidenceNotAboveMinimum);
    assert_eq!(trace.eligible, 0);
    assert_eq!(trace.max_score_milli, 1000);
    assert!(trace.scores_not_above_minimum >= 1, "{trace:?}");
}

#[test]
fn marginal_frame_ends_at_the_confidence_filter_with_its_real_score() {
    let (proposal, trace) = traced(&marginal_frame(), SIDE, SIDE, STRIDE, MIN_CONFIDENCE);
    assert_eq!(proposal, None);
    assert_eq!(trace.terminal(false), CvTerminal::ConfidenceNotAboveMinimum);
    assert_eq!(trace.max_score_milli, 379);
    assert_eq!(trace.eligible, 0);
}

#[test]
fn uniform_frame_has_no_edges() {
    let (proposal, trace) = traced(&uniform_frame(), SIDE, SIDE, STRIDE, MIN_CONFIDENCE);
    assert_eq!(proposal, None);
    assert_eq!(trace.terminal(false), CvTerminal::NoEdges);
    assert_eq!(trace.scored_attempts, 0);
}

#[test]
fn small_or_malformed_input_stops_before_any_search() {
    let small = [35, 55, 83, 255].repeat(100 * 100);
    let (proposal, trace) = traced(&small, 100, 100, 400, MIN_CONFIDENCE);
    assert_eq!(proposal, None);
    assert_eq!(trace.terminal(false), CvTerminal::TooSmall);

    let pixels = nested_frame();
    let (proposal, trace) = traced(&pixels, SIDE, SIDE, 10, MIN_CONFIDENCE);
    assert_eq!(proposal, None);
    assert_eq!(trace.terminal(false), CvTerminal::InvalidFrame);
    assert_eq!(trace.scored_attempts, 0);
}

#[test]
fn dark_border_frame_is_classified_without_inventing_a_score() {
    let (proposal, trace) = traced(&dark_frame(), SIDE, SIDE, STRIDE, MIN_CONFIDENCE);
    assert_eq!(proposal, None);
    let terminal = trace.terminal(false);
    assert!(
        matches!(
            terminal,
            CvTerminal::NoEdges | CvTerminal::NoRectangleCandidate | CvTerminal::ScoreRejected
        ),
        "{terminal:?} {trace:?}"
    );
    if trace.scores_produced == 0 {
        assert_eq!(
            trace.max_score_milli, 0,
            "no score must not look like zero confidence"
        );
    }
}

#[test]
fn terminal_order_is_exclusive_and_counters_saturate() {
    let mut trace = StageTrace {
        invalid_frame: 1,
        too_small: 1,
        horizontal_edges: 3,
        vertical_edges: 3,
        scored_attempts: 4,
        scores_produced: 2,
        ..StageTrace::default()
    };
    assert_eq!(trace.terminal(true), CvTerminal::Proposed);
    assert_eq!(trace.terminal(false), CvTerminal::InvalidFrame);
    trace.invalid_frame = 0;
    assert_eq!(trace.terminal(false), CvTerminal::TooSmall);
    trace.too_small = 0;
    assert_eq!(trace.terminal(false), CvTerminal::ConfidenceNotAboveMinimum);
    trace.scores_produced = 0;
    assert_eq!(trace.terminal(false), CvTerminal::ScoreRejected);
    trace.scored_attempts = 0;
    assert_eq!(trace.terminal(false), CvTerminal::NoRectangleCandidate);
    trace.vertical_edges = 0;
    assert_eq!(trace.terminal(false), CvTerminal::NoEdges);
}
