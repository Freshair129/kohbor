// Provenance: ported from owner-authored G-Maiden 0.13.6 src-tauri/src/cv/prefilter.rs; src-tauri/src/cv/mod.rs; src-tauri/src/cv/region.rs, owner consent recorded in claim vision-risk__mvp1-minimap-cv.
//! Pure CPU colour-ring detection for minimap crops.

use std::convert::TryFrom;

mod proposed_unverified_constants {
    // PROPOSED / UNVERIFIED on the current game patch.
    pub const FIRST_INDEX: usize = 0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const ONE_INDEX: usize = 1; // PROPOSED / UNVERIFIED on the current game patch.
    pub const FIRST_SIGNED_INDEX: isize = 0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const BLUE_CHANNEL: usize = 0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const GREEN_CHANNEL: usize = 1; // PROPOSED / UNVERIFIED on the current game patch.
    pub const RED_CHANNEL: usize = 2; // PROPOSED / UNVERIFIED on the current game patch.
    pub const RED_RGB_INDEX: usize = 0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const GREEN_RGB_INDEX: usize = 1; // PROPOSED / UNVERIFIED on the current game patch.
    pub const BLUE_RGB_INDEX: usize = 2; // PROPOSED / UNVERIFIED on the current game patch.
    pub const CHANNELS_PER_PIXEL: usize = 4; // PROPOSED / UNVERIFIED on the current game patch.
    pub const COLOR_CHANNEL_MAX: f64 = 255.0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const EXACT_COLOR_MATCH: f64 = 1.0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const CELL_CENTER_DIVISOR: usize = 2; // PROPOSED / UNVERIFIED on the current game patch.
    pub const ZERO_SCORE: f64 = 0.0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const RED_LUMA_WEIGHT: f64 = 0.299; // PROPOSED / UNVERIFIED on the current game patch.
    pub const GREEN_LUMA_WEIGHT: f64 = 0.587; // PROPOSED / UNVERIFIED on the current game patch.
    pub const BLUE_LUMA_WEIGHT: f64 = 0.114; // PROPOSED / UNVERIFIED on the current game patch.
    pub const ICON_FRACTION_OF_MINIMAP: f64 = 0.078; // PROPOSED / UNVERIFIED on the current game patch.
    pub const MINIMUM_CELL_SIDE: usize = 1; // PROPOSED / UNVERIFIED on the current game patch.
    pub const PEAK_SCORE_FRACTION: f64 = 0.5; // PROPOSED / UNVERIFIED on the current game patch.
    pub const ABSOLUTE_CELL_SCORE_FLOOR: f64 = 0.02; // PROPOSED / UNVERIFIED on the current game patch.
    pub const AREA_PIXEL_SCORE_FLOOR: f64 = 0.08; // PROPOSED / UNVERIFIED on the current game patch.
    pub const MINIMUM_ICON_AREA_FRACTION: f64 = 0.4; // PROPOSED / UNVERIFIED on the current game patch.
    pub const MERGE_DISTANCE_ICON_MULTIPLIER: f64 = 1.0; // PROPOSED / UNVERIFIED on the current game patch.
    pub const BORDER_WIDTH: u32 = 3; // PROPOSED / UNVERIFIED on the current game patch.
    pub const MINIMUM_STRUCTURAL_SIDE: u32 = 7; // PROPOSED / UNVERIFIED on the current game patch.
    pub const MINIMUM_INTERIOR_LUMA_STDDEV: f64 = 0.015; // PROPOSED / UNVERIFIED on the current game patch.
    pub const MAXIMUM_INTERIOR_LUMA_STDDEV: f64 = 0.45; // PROPOSED / UNVERIFIED on the current game patch.
    pub const COORDINATE_SCALE: u64 = 1000; // PROPOSED / UNVERIFIED on the current game patch.
    pub const DIRE_RING_RGB: [f64; 3] = [0.86, 0.16, 0.16]; // PROPOSED / UNVERIFIED on the current game patch.
    pub const RADIANT_RING_RGB: [f64; 3] = [0.40, 0.86, 0.30]; // PROPOSED / UNVERIFIED on the current game patch.
    pub const GRID_NEIGHBORS: [(isize, isize); 8] = [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ]; // PROPOSED / UNVERIFIED on the current game patch.

    // Frame proposal thresholds: PROPOSED / UNVERIFIED on real game pixels.
    pub const FRAME_PROPOSAL_MIN_SIDE: usize = 150;
    pub const FRAME_PROPOSAL_MAX_SIDE: usize = 400;
    pub const FRAME_PROPOSAL_MIN_ASPECT: f32 = 0.8;
    pub const FRAME_PROPOSAL_MAX_ASPECT: f32 = 1.25;
    pub const FRAME_PROPOSAL_MIN_EDGE_STRENGTH: f32 = 12.0;
    pub const FRAME_PROPOSAL_EDGE_BASELINE_MIN_DISTANCE: usize = 6;
    pub const FRAME_PROPOSAL_EDGE_BASELINE_MAX_DISTANCE: usize = 9;
    pub const FRAME_PROPOSAL_BASELINE_SAMPLE_COUNT: usize =
        (FRAME_PROPOSAL_EDGE_BASELINE_MAX_DISTANCE - FRAME_PROPOSAL_EDGE_BASELINE_MIN_DISTANCE + 1)
            * 2;
    pub const FRAME_PROPOSAL_MAX_EDGE_THICKNESS: usize = 6;
    pub const FRAME_PROPOSAL_EDGE_PEAK_TOLERANCE: f32 = 8.0;
    pub const FRAME_PROPOSAL_MAX_EDGE_CANDIDATES: usize = 48;
    pub const FRAME_PROPOSAL_MIN_LOCAL_CONTRAST: f32 = 18.0;
    pub const FRAME_PROPOSAL_MIN_SIDE_AGREEMENT: f32 = 0.65;
    pub const FRAME_PROPOSAL_BORDER_SAMPLE_COUNT: usize = 9;
    pub const FRAME_PROPOSAL_CORNER_INSET: usize = 4;
    pub const FRAME_PROPOSAL_INTERIOR_INSET: usize = 2;
    pub const FRAME_PROPOSAL_TEXTURE_GRID_SIDE: usize = 9;
    pub const FRAME_PROPOSAL_TEXTURE_SAMPLE_COUNT: usize =
        FRAME_PROPOSAL_TEXTURE_GRID_SIDE * FRAME_PROPOSAL_TEXTURE_GRID_SIDE;
    pub const FRAME_PROPOSAL_TEXTURE_PATCH_RADIUS: usize = 1;
    pub const FRAME_PROPOSAL_MIN_TEXTURE_STDDEV: f32 = 4.0;
    pub const FRAME_PROPOSAL_MIN_DARKNESS_GAP: f32 = 8.0;
    pub const FRAME_PROPOSAL_FULL_EDGE_STRENGTH: f32 = 80.0;
    pub const FRAME_PROPOSAL_FULL_LOCAL_CONTRAST: f32 = 80.0;
    pub const FRAME_PROPOSAL_FULL_TEXTURE_STDDEV: f32 = 24.0;
    pub const FRAME_PROPOSAL_FULL_DARKNESS_GAP: f32 = 80.0;
    pub const FRAME_PROPOSAL_CHROMA_PENALTY: f32 = 0.5;
    pub const FRAME_PROPOSAL_CONFIDENCE_MAX: f32 = 1000.0;
}

use proposed_unverified_constants as constants;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Radiant,
    Dire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blob {
    pub x_milli: u16,
    pub y_milli: u16,
    pub area: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelReport {
    pub structural_ok: bool,
    pub enemy_blobs: Vec<Blob>,
}

/// The crop profile determines whether its pixels include an outer HUD frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuralMode {
    /// Require the existing three-pixel border to be darker than the interior.
    Framed,
    /// The crop is already inside the frame; test its texture without a border requirement.
    Inner,
}

/// Count colour-ring marker candidates in a BGRA crop.
///
/// `stride` is the byte distance between row starts. A malformed or empty
/// buffer returns `structural_ok == false` and no blobs. `enemy` selects the
/// enemy ring colour; the reported coordinates are crop-relative milli-units.
/// All thresholds and target colours are proposed and unverified for the
/// current game patch.
pub fn analyze(bgra: &[u8], width: u32, height: u32, stride: u32, enemy: Team) -> PixelReport {
    analyze_with_grid_limit(bgra, width, height, stride, enemy, usize::MAX)
}

/// Cheap per-frame structural check only (no blob search). Equals `analyze(..).structural_ok`.
pub fn structural_check(bgra: &[u8], width: u32, height: u32, stride: u32) -> bool {
    structural_check_mode(bgra, width, height, stride, StructuralMode::Framed)
}

/// Per-frame structural validation with unchanged size and luminance-variance thresholds.
/// Inner treats the entire crop as map interior. Neither mode accepts uniform pixels.
/// Inner is only a texture check: unrelated textured images can pass. It does not
/// identify the game or the minimap, nor confirm that the crop matches the intended HUD.
pub fn structural_check_mode(
    bgra: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    mode: StructuralMode,
) -> bool {
    FrameView::new(bgra, width, height, stride).is_some_and(|frame| frame.structural_ok_mode(mode))
}

fn analyze_with_grid_limit(
    bgra: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    enemy: Team,
    max_grid_cells: usize,
) -> PixelReport {
    let Some(frame) = FrameView::new(bgra, width, height, stride) else {
        return empty_report();
    };

    let structural_ok = frame.structural_ok();
    let target = match enemy {
        Team::Dire => constants::DIRE_RING_RGB,
        Team::Radiant => constants::RADIANT_RING_RGB,
    };
    let icon_side = ((frame.width.min(frame.height) as f64 * constants::ICON_FRACTION_OF_MINIMAP)
        .round() as usize)
        .max(constants::MINIMUM_CELL_SIDE);
    let grid_width = constants::ONE_INDEX + (frame.width - constants::ONE_INDEX) / icon_side;
    let grid_height = constants::ONE_INDEX + (frame.height - constants::ONE_INDEX) / icon_side;
    let Some(grid_len) = grid_width.checked_mul(grid_height) else {
        return empty_report();
    };
    if grid_len > max_grid_cells {
        return empty_report();
    };

    let mut grid = Vec::new();
    if grid.try_reserve_exact(grid_len).is_err() {
        return empty_report();
    }
    grid.resize(grid_len, GridCell::default());

    for y in constants::FIRST_INDEX..frame.height {
        let row_offset = y * frame.stride;
        for x in constants::FIRST_INDEX..frame.width {
            let offset = row_offset + x * constants::CHANNELS_PER_PIXEL;
            let score = color_brightness_score(&frame.bgra[offset..], target);
            let cell_index = (y / icon_side) * grid_width + (x / icon_side);
            let cell = &mut grid[cell_index];
            cell.score_sum += score;
            cell.pixel_count += constants::ONE_INDEX;
            if score >= constants::AREA_PIXEL_SCORE_FLOOR {
                cell.colour_pixel_count += constants::ONE_INDEX;
            }
        }
    }

    let peak = grid
        .iter()
        .map(GridCell::average_score)
        .fold(constants::ZERO_SCORE, f64::max);
    let cell_floor =
        (constants::PEAK_SCORE_FRACTION * peak).max(constants::ABSOLUTE_CELL_SCORE_FLOOR);
    for cell in &mut grid {
        cell.eligible = cell.average_score() >= cell_floor;
    }

    let Some(mut candidates) =
        connected_blobs(&mut grid, grid_width, icon_side, frame.width, frame.height)
    else {
        return empty_report();
    };
    let mut merged = Vec::new();
    for candidate in candidates.drain(..) {
        if !merge_candidate(&mut merged, candidate, icon_side) {
            return empty_report();
        }
    }

    let mut enemy_blobs = Vec::new();
    if enemy_blobs.try_reserve_exact(merged.len()).is_err() {
        return empty_report();
    }
    let minimum_area =
        constants::MINIMUM_ICON_AREA_FRACTION * (icon_side as f64 * icon_side as f64);
    for candidate in merged {
        if (candidate.area as f64) < minimum_area {
            continue;
        }
        enemy_blobs.push(Blob {
            x_milli: to_milli(candidate.center_x, frame.width),
            y_milli: to_milli(candidate.center_y, frame.height),
            area: u32::try_from(candidate.area).unwrap_or(u32::MAX),
        });
    }

    PixelReport {
        structural_ok,
        enemy_blobs,
    }
}

fn empty_report() -> PixelReport {
    PixelReport {
        structural_ok: false,
        enemy_blobs: Vec::new(),
    }
}

#[derive(Clone, Copy)]
struct FrameView<'a> {
    bgra: &'a [u8],
    width: usize,
    height: usize,
    stride: usize,
}

impl<'a> FrameView<'a> {
    fn new(bgra: &'a [u8], width: u32, height: u32, stride: u32) -> Option<Self> {
        if bgra.is_empty()
            || width == constants::FIRST_INDEX as u32
            || height == constants::FIRST_INDEX as u32
        {
            return None;
        }
        let width = usize::try_from(width).ok()?;
        let height = usize::try_from(height).ok()?;
        let stride = usize::try_from(stride).ok()?;
        let row_bytes = width.checked_mul(constants::CHANNELS_PER_PIXEL)?;
        if stride < row_bytes {
            return None;
        }
        let required_len = stride
            .checked_mul(height.checked_sub(constants::ONE_INDEX)?)?
            .checked_add(row_bytes)?;
        if bgra.len() < required_len {
            return None;
        }
        Some(Self {
            bgra,
            width,
            height,
            stride,
        })
    }

    fn structural_ok(&self) -> bool {
        self.structural_ok_mode(StructuralMode::Framed)
    }

    fn structural_ok_mode(&self, mode: StructuralMode) -> bool {
        let border = match mode {
            StructuralMode::Framed => constants::BORDER_WIDTH as usize,
            StructuralMode::Inner => 0,
        };
        if self.width < constants::MINIMUM_STRUCTURAL_SIDE as usize
            || self.height < constants::MINIMUM_STRUCTURAL_SIDE as usize
        {
            return false;
        }

        let interior_right = self.width - border;
        let interior_bottom = self.height - border;
        let mut border_sum = constants::ZERO_SCORE;
        let mut border_count = constants::FIRST_INDEX;
        let mut interior_sum = constants::ZERO_SCORE;
        let mut interior_square_sum = constants::ZERO_SCORE;
        let mut interior_count = constants::FIRST_INDEX;
        let mut first_color = None;
        let mut all_equal = true;

        for y in constants::FIRST_INDEX..self.height {
            let row_offset = y * self.stride;
            for x in constants::FIRST_INDEX..self.width {
                let offset = row_offset + x * constants::CHANNELS_PER_PIXEL;
                let pixel = &self.bgra[offset..];
                let color = [
                    pixel[constants::BLUE_CHANNEL],
                    pixel[constants::GREEN_CHANNEL],
                    pixel[constants::RED_CHANNEL],
                ];
                if let Some(reference) = first_color {
                    if color != reference {
                        all_equal = false;
                    }
                } else {
                    first_color = Some(color);
                }

                let luminance = luma(
                    f64::from(color[constants::RED_CHANNEL]) / constants::COLOR_CHANNEL_MAX,
                    f64::from(color[constants::GREEN_CHANNEL]) / constants::COLOR_CHANNEL_MAX,
                    f64::from(color[constants::BLUE_CHANNEL]) / constants::COLOR_CHANNEL_MAX,
                );
                let is_interior =
                    x >= border && x < interior_right && y >= border && y < interior_bottom;
                if is_interior {
                    interior_sum += luminance;
                    interior_square_sum += luminance * luminance;
                    interior_count += constants::ONE_INDEX;
                } else {
                    border_sum += luminance;
                    border_count += constants::ONE_INDEX;
                }
            }
        }

        if all_equal
            || (mode == StructuralMode::Framed && border_count == constants::FIRST_INDEX)
            || interior_count == constants::FIRST_INDEX
        {
            return false;
        }

        let interior_mean = interior_sum / interior_count as f64;
        let variance = (interior_square_sum / interior_count as f64
            - interior_mean * interior_mean)
            .max(constants::ZERO_SCORE);
        let interior_stddev = variance.sqrt();
        (mode == StructuralMode::Inner || border_sum / (border_count as f64) < interior_mean)
            && (constants::MINIMUM_INTERIOR_LUMA_STDDEV..=constants::MAXIMUM_INTERIOR_LUMA_STDDEV)
                .contains(&interior_stddev)
    }
}

fn luma(red: f64, green: f64, blue: f64) -> f64 {
    constants::RED_LUMA_WEIGHT * red
        + constants::GREEN_LUMA_WEIGHT * green
        + constants::BLUE_LUMA_WEIGHT * blue
}

fn color_brightness_score(pixel: &[u8], target: [f64; 3]) -> f64 {
    let blue = f64::from(pixel[constants::BLUE_CHANNEL]) / constants::COLOR_CHANNEL_MAX;
    let green = f64::from(pixel[constants::GREEN_CHANNEL]) / constants::COLOR_CHANNEL_MAX;
    let red = f64::from(pixel[constants::RED_CHANNEL]) / constants::COLOR_CHANNEL_MAX;
    let red_delta = red - target[constants::RED_RGB_INDEX];
    let green_delta = green - target[constants::GREEN_RGB_INDEX];
    let blue_delta = blue - target[constants::BLUE_RGB_INDEX];
    let distance =
        (red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta).sqrt();
    let similarity = (constants::EXACT_COLOR_MATCH - distance).max(constants::ZERO_SCORE);
    let brightness = luma(red, green, blue);
    similarity * similarity * brightness
}

#[derive(Clone, Copy, Default)]
struct GridCell {
    score_sum: f64,
    pixel_count: usize,
    colour_pixel_count: usize,
    eligible: bool,
    visited: bool,
}

impl GridCell {
    fn average_score(&self) -> f64 {
        if self.pixel_count == constants::FIRST_INDEX {
            constants::ZERO_SCORE
        } else {
            self.score_sum / self.pixel_count as f64
        }
    }
}

#[derive(Clone, Copy)]
struct Candidate {
    center_x: f64,
    center_y: f64,
    area: u64,
}

fn connected_blobs(
    grid: &mut [GridCell],
    grid_width: usize,
    icon_side: usize,
    width: usize,
    height: usize,
) -> Option<Vec<Candidate>> {
    let grid_height = grid.len().checked_div(grid_width)?;
    let mut queue = Vec::new();
    queue.try_reserve_exact(grid.len()).ok()?;
    let mut candidates = Vec::new();
    for start in constants::FIRST_INDEX..grid.len() {
        if !grid[start].eligible || grid[start].visited {
            continue;
        }
        queue.clear();
        grid[start].visited = true;
        queue.push(start);
        let mut head = constants::FIRST_INDEX;
        let mut min_x = usize::MAX;
        let mut max_x = constants::FIRST_INDEX;
        let mut min_y = usize::MAX;
        let mut max_y = constants::FIRST_INDEX;
        let mut area = 0_u64;

        while head < queue.len() {
            let current = queue[head];
            head += constants::ONE_INDEX;
            let grid_x = current % grid_width;
            let grid_y = current / grid_width;
            let cell = grid[current];
            let cell_center_x = (grid_x * icon_side + icon_side / constants::CELL_CENTER_DIVISOR)
                .min(width - constants::ONE_INDEX);
            let cell_center_y = (grid_y * icon_side + icon_side / constants::CELL_CENTER_DIVISOR)
                .min(height - constants::ONE_INDEX);
            min_x = min_x.min(cell_center_x);
            max_x = max_x.max(cell_center_x);
            min_y = min_y.min(cell_center_y);
            max_y = max_y.max(cell_center_y);
            area = area.saturating_add(cell.colour_pixel_count as u64);

            for (delta_x, delta_y) in constants::GRID_NEIGHBORS {
                let neighbor_x = grid_x as isize + delta_x;
                let neighbor_y = grid_y as isize + delta_y;
                if neighbor_x < constants::FIRST_SIGNED_INDEX
                    || neighbor_y < constants::FIRST_SIGNED_INDEX
                    || neighbor_x >= grid_width as isize
                    || neighbor_y >= grid_height as isize
                {
                    continue;
                }
                let neighbor_index = neighbor_y as usize * grid_width + neighbor_x as usize;
                if grid[neighbor_index].eligible && !grid[neighbor_index].visited {
                    grid[neighbor_index].visited = true;
                    queue.push(neighbor_index);
                }
            }
        }

        let center_x = (min_x + max_x) as f64 / constants::CELL_CENTER_DIVISOR as f64;
        let center_y = (min_y + max_y) as f64 / constants::CELL_CENTER_DIVISOR as f64;
        if candidates.try_reserve(constants::ONE_INDEX).is_err() {
            return None;
        }
        candidates.push(Candidate {
            center_x,
            center_y,
            area,
        });
    }
    Some(candidates)
}

fn merge_candidate(
    merged: &mut Vec<Candidate>,
    mut candidate: Candidate,
    icon_side: usize,
) -> bool {
    let merge_distance = icon_side as f64 * constants::MERGE_DISTANCE_ICON_MULTIPLIER;
    let merge_distance_squared = merge_distance * merge_distance;
    let mut index = constants::FIRST_INDEX;
    while index < merged.len() {
        let existing = merged[index];
        let delta_x = candidate.center_x - existing.center_x;
        let delta_y = candidate.center_y - existing.center_y;
        if delta_x * delta_x + delta_y * delta_y < merge_distance_squared {
            let combined_area = candidate.area.saturating_add(existing.area);
            if combined_area > constants::FIRST_INDEX as u64 {
                candidate.center_x = (candidate.center_x * candidate.area as f64
                    + existing.center_x * existing.area as f64)
                    / combined_area as f64;
                candidate.center_y = (candidate.center_y * candidate.area as f64
                    + existing.center_y * existing.area as f64)
                    / combined_area as f64;
            }
            candidate.area = combined_area;
            merged.remove(index);
            index = constants::FIRST_INDEX;
        } else {
            index += constants::ONE_INDEX;
        }
    }
    if merged.try_reserve(constants::ONE_INDEX).is_err() {
        return false;
    }
    merged.push(candidate);
    true
}

fn to_milli(coordinate: f64, extent: usize) -> u16 {
    if extent <= constants::ONE_INDEX {
        return constants::FIRST_INDEX as u16;
    }
    let maximum = (extent - constants::ONE_INDEX) as f64;
    let clamped = coordinate.clamp(constants::ZERO_SCORE, maximum);
    let milli = (clamped * constants::COORDINATE_SCALE as f64 / maximum).round();
    milli.clamp(constants::ZERO_SCORE, constants::COORDINATE_SCALE as f64) as u16
}

#[cfg(test)]
mod tests {
    use super::{analyze_with_grid_limit, merge_candidate, Candidate, Team};

    fn structural_mode_fixture() -> (Vec<u8>, Vec<u8>) {
        let mut framed = Vec::new();
        let mut inner = Vec::new();
        for y in 0..70 {
            for x in 0..70 {
                let in_map = (3..67).contains(&x) && (3..67).contains(&y);
                let value = if !in_map {
                    5
                } else if x < 6 || y < 6 || x >= 64 || y >= 64 {
                    160
                } else if (x + y) % 2 == 0 {
                    60
                } else {
                    100
                };
                let pixel = [value, value, value, 255];
                framed.extend_from_slice(&pixel);
                if in_map {
                    inner.extend_from_slice(&pixel);
                }
            }
        }
        (framed, inner)
    }

    #[test]
    fn structural_modes_inner_texture_passes_without_a_dark_frame() {
        let (_, inner) = structural_mode_fixture();
        assert!(!super::structural_check(&inner, 64, 64, 256));
        assert!(!super::structural_check_mode(
            &inner,
            64,
            64,
            256,
            super::StructuralMode::Framed
        ));
        assert!(super::structural_check_mode(
            &inner,
            64,
            64,
            256,
            super::StructuralMode::Inner
        ));
        assert!(!super::analyze(&inner, 64, 64, 256, Team::Dire).structural_ok);
    }

    #[test]
    fn structural_modes_framed_preserves_the_existing_dark_border_check() {
        let (framed, _) = structural_mode_fixture();
        assert!(super::structural_check(&framed, 70, 70, 280));
        assert!(super::structural_check_mode(
            &framed,
            70,
            70,
            280,
            super::StructuralMode::Framed
        ));
        assert!(super::structural_check_mode(
            &framed,
            70,
            70,
            280,
            super::StructuralMode::Inner
        ));
    }

    #[test]
    fn structural_modes_uniform_pixels_fail_both_modes() {
        for mode in [super::StructuralMode::Inner, super::StructuralMode::Framed] {
            for value in [0, 70, 255] {
                assert!(!super::structural_check_mode(
                    &vec![value; 64 * 64 * 4],
                    64,
                    64,
                    256,
                    mode
                ));
            }
        }
    }

    #[test]
    fn structural_modes_reject_small_malformed_and_excessive_variance() {
        let extreme = (0..64 * 64)
            .flat_map(|index| {
                let value = if index % 2 == 0 { 0 } else { 255 };
                [value, value, value, 255]
            })
            .collect::<Vec<_>>();
        for mode in [super::StructuralMode::Inner, super::StructuralMode::Framed] {
            assert!(!super::structural_check_mode(&[], 0, 0, 0, mode));
            assert!(!super::structural_check_mode(&[20; 144], 6, 6, 24, mode));
            assert!(!super::structural_check_mode(&[20; 10], 64, 64, 256, mode));
            assert!(!super::structural_check_mode(&extreme, 64, 64, 256, mode));
        }
    }

    #[test]
    fn structural_check_matches_analyze_and_rejects_bad_input() {
        assert!(!crate::structural_check(&[], 0, 0, 0));
        let uniform = vec![40u8; 292 * 292 * 4];
        assert!(!crate::structural_check(&uniform, 292, 292, 292 * 4));
        let mut textured = vec![0u8; 292 * 292 * 4];
        for (i, px) in textured.chunks_mut(4).enumerate() {
            let v = ((i * 37) % 97) as u8;
            px.copy_from_slice(&[v, v / 2, v / 3, 255]);
        }
        assert_eq!(
            crate::structural_check(&textured, 292, 292, 292 * 4),
            crate::analyze(&textured, 292, 292, 292 * 4, crate::Team::Dire).structural_ok
        );
    }

    #[test]
    fn grid_size_limit_failure_is_structurally_invalid() {
        let side = 16_u32;
        let stride = side * 4;
        let mut bgra = Vec::new();
        for y in 0..side {
            for x in 0..side {
                let border = x < 3 || y < 3 || x >= side - 3 || y >= side - 3;
                let pixel = if border {
                    [5, 8, 5, 255]
                } else if (x + y) % 2 == 0 {
                    [18, 24, 14, 255]
                } else {
                    [70, 80, 55, 255]
                };
                bgra.extend_from_slice(&pixel);
            }
        }
        assert!(super::FrameView::new(&bgra, side, side, stride)
            .expect("valid synthetic frame")
            .structural_ok());

        let report = analyze_with_grid_limit(&bgra, side, side, stride, Team::Dire, 0);
        assert!(!report.structural_ok);
        assert!(report.enemy_blobs.is_empty());
    }

    #[test]
    fn merges_candidate_centres_closer_than_one_icon() {
        let mut merged = Vec::new();
        assert!(merge_candidate(
            &mut merged,
            Candidate {
                center_x: 10.0,
                center_y: 10.0,
                area: 100,
            },
            20,
        ));
        assert!(merge_candidate(
            &mut merged,
            Candidate {
                center_x: 29.0,
                center_y: 10.0,
                area: 100,
            },
            20,
        ));
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].area, 200);
        assert_eq!(merged[0].center_x, 19.5);
    }
}

/// Candidate outer rectangle of the minimap frame.
///
/// Coordinates are relative to the image passed to [`propose_frame`]. Accuracy
/// on real Dota pixels is unknown and has not been validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameProposal {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub confidence_milli: u16,
}

/// Proposes a minimap frame rectangle inside a BGRA search image.
///
/// Coordinates are relative to the passed image. The detector looks for four
/// bright, greyish edges enclosing a darker textured interior. It is based on
/// synthetic evidence only; accuracy on real game pixels is unknown.
pub fn propose_frame(bgra: &[u8], width: u32, height: u32, stride: u32) -> Option<FrameProposal> {
    let frame = FrameView::new(bgra, width, height, stride)?;
    if frame.width < constants::FRAME_PROPOSAL_MIN_SIDE
        || frame.height < constants::FRAME_PROPOSAL_MIN_SIDE
    {
        return None;
    }

    let (row_profile, column_profile) = frame_luminance_profiles(&frame)?;
    let horizontal_edges = find_frame_edges(&row_profile)?;
    let vertical_edges = find_frame_edges(&column_profile)?;
    let mut best: Option<(u64, FrameProposal)> = None;

    for (left_index, left) in vertical_edges.iter().enumerate() {
        for right in vertical_edges
            .iter()
            .skip(left_index + constants::ONE_INDEX)
        {
            let Some(width) = right.end.checked_sub(left.start) else {
                continue;
            };
            if !(constants::FRAME_PROPOSAL_MIN_SIDE..=constants::FRAME_PROPOSAL_MAX_SIDE)
                .contains(&width)
            {
                continue;
            }

            for (top_index, top) in horizontal_edges.iter().enumerate() {
                for bottom in horizontal_edges
                    .iter()
                    .skip(top_index + constants::ONE_INDEX)
                {
                    let Some(height) = bottom.end.checked_sub(top.start) else {
                        continue;
                    };
                    if !(constants::FRAME_PROPOSAL_MIN_SIDE..=constants::FRAME_PROPOSAL_MAX_SIDE)
                        .contains(&height)
                    {
                        continue;
                    }

                    let aspect = width as f32 / height as f32;
                    if !(constants::FRAME_PROPOSAL_MIN_ASPECT
                        ..=constants::FRAME_PROPOSAL_MAX_ASPECT)
                        .contains(&aspect)
                    {
                        continue;
                    }
                    let Some(aspect_agreement) = frame_aspect_agreement(aspect) else {
                        continue;
                    };
                    let Some(confidence) = score_frame_rectangle(
                        &frame,
                        *left,
                        *right,
                        *top,
                        *bottom,
                        aspect_agreement,
                    ) else {
                        continue;
                    };
                    let (Ok(x), Ok(y), Ok(width), Ok(height)) = (
                        i32::try_from(left.start),
                        i32::try_from(top.start),
                        u32::try_from(width),
                        u32::try_from(height),
                    ) else {
                        continue;
                    };
                    let proposal = FrameProposal {
                        x,
                        y,
                        width,
                        height,
                        confidence_milli: confidence,
                    };
                    let area = u64::from(width) * u64::from(height);
                    let should_replace = best.is_none_or(|(best_area, best_proposal)| {
                        area > best_area
                            || (area == best_area
                                && proposal.confidence_milli > best_proposal.confidence_milli)
                    });
                    if should_replace {
                        best = Some((area, proposal));
                    }
                }
            }
        }
    }

    best.map(|(_, proposal)| proposal)
}

#[derive(Clone, Copy)]
struct FrameEdge {
    start: usize,
    end: usize,
    strength: f32,
}

#[derive(Clone, Copy)]
struct SideEvidence {
    agreement: f32,
    mean_contrast: f32,
    mean_border_luma: f32,
}

#[derive(Clone, Copy)]
struct TextureEvidence {
    mean_luma: f32,
    standard_deviation: f32,
}

fn frame_luminance_profiles(frame: &FrameView<'_>) -> Option<(Vec<f32>, Vec<f32>)> {
    let mut row_sums = Vec::new();
    row_sums.try_reserve_exact(frame.height).ok()?;
    row_sums.resize(frame.height, 0_u64);

    let mut column_sums = Vec::new();
    column_sums.try_reserve_exact(frame.width).ok()?;
    column_sums.resize(frame.width, 0_u64);

    for (y, row_sum) in row_sums.iter_mut().enumerate() {
        let row_offset = y.checked_mul(frame.stride)?;
        for (x, column_sum) in column_sums.iter_mut().enumerate() {
            let offset = row_offset.checked_add(x.checked_mul(constants::CHANNELS_PER_PIXEL)?)?;
            let pixel_end = offset.checked_add(constants::CHANNELS_PER_PIXEL)?;
            let pixel = frame.bgra.get(offset..pixel_end)?;
            let signal = u64::from(gray_luminance_signal(pixel)? as u8);
            *row_sum = row_sum.checked_add(signal)?;
            *column_sum = column_sum.checked_add(signal)?;
        }
    }

    let width = frame.width as f32;
    let height = frame.height as f32;
    let mut row_profile = Vec::new();
    row_profile.try_reserve_exact(frame.height).ok()?;
    row_profile.extend(row_sums.into_iter().map(|sum| sum as f32 / width));
    let mut column_profile = Vec::new();
    column_profile.try_reserve_exact(frame.width).ok()?;
    column_profile.extend(column_sums.into_iter().map(|sum| sum as f32 / height));
    Some((row_profile, column_profile))
}

fn find_frame_edges(profile: &[f32]) -> Option<Vec<FrameEdge>> {
    if profile.len() < constants::FRAME_PROPOSAL_MIN_SIDE {
        return None;
    }

    let mut deltas = Vec::new();
    deltas.try_reserve_exact(profile.len()).ok()?;
    for index in constants::FIRST_INDEX..profile.len() {
        let mut neighbours = [0.0_f32; constants::FRAME_PROPOSAL_BASELINE_SAMPLE_COUNT];
        let mut neighbour_count = constants::FIRST_INDEX;
        for distance in constants::FRAME_PROPOSAL_EDGE_BASELINE_MIN_DISTANCE
            ..=constants::FRAME_PROPOSAL_EDGE_BASELINE_MAX_DISTANCE
        {
            if let Some(before) = index.checked_sub(distance) {
                neighbours[neighbour_count] = profile[before];
                neighbour_count += constants::ONE_INDEX;
            }
            if let Some(after) = index
                .checked_add(distance)
                .filter(|after| *after < profile.len())
            {
                neighbours[neighbour_count] = profile[after];
                neighbour_count += constants::ONE_INDEX;
            }
        }
        if neighbour_count == constants::FIRST_INDEX {
            return None;
        }
        neighbours[..neighbour_count].sort_by(f32::total_cmp);
        let baseline = if neighbour_count % 2 == constants::FIRST_INDEX {
            neighbours[neighbour_count / 2]
        } else {
            (neighbours[neighbour_count / 2 - constants::ONE_INDEX]
                + neighbours[neighbour_count / 2])
                / 2.0
        };
        deltas.push(profile[index] - baseline);
    }

    let mut edges = Vec::new();
    let mut index = constants::FIRST_INDEX;
    while index < deltas.len() {
        if deltas[index] < constants::FRAME_PROPOSAL_MIN_EDGE_STRENGTH {
            index += constants::ONE_INDEX;
            continue;
        }

        let detected_start = index;
        while index < deltas.len() && deltas[index] >= constants::FRAME_PROPOSAL_MIN_EDGE_STRENGTH {
            index += constants::ONE_INDEX;
        }
        let detected_end = index;
        let peak = (detected_start..detected_end)
            .max_by(|left, right| profile[*left].total_cmp(&profile[*right]))?;
        let peak_value = profile[peak];
        let peak_floor = peak_value - constants::FRAME_PROPOSAL_EDGE_PEAK_TOLERANCE;
        let mut start = peak;
        while start > detected_start && profile[start - constants::ONE_INDEX] >= peak_floor {
            start -= constants::ONE_INDEX;
        }
        let mut end = peak + constants::ONE_INDEX;
        while end < detected_end && profile[end] >= peak_floor {
            end += constants::ONE_INDEX;
        }
        let thickness = end.checked_sub(start)?;
        if thickness <= constants::FRAME_PROPOSAL_MAX_EDGE_THICKNESS {
            edges.try_reserve(constants::ONE_INDEX).ok()?;
            edges.push(FrameEdge {
                start,
                end,
                strength: *deltas[detected_start..detected_end]
                    .iter()
                    .max_by(|left, right| left.total_cmp(right))?,
            });
        }
    }

    edges.sort_by(|left, right| {
        right
            .strength
            .total_cmp(&left.strength)
            .then_with(|| left.start.cmp(&right.start))
    });
    edges.truncate(constants::FRAME_PROPOSAL_MAX_EDGE_CANDIDATES);
    edges.sort_by_key(|edge| edge.start);
    Some(edges)
}

fn frame_aspect_agreement(aspect: f32) -> Option<f32> {
    if !(constants::FRAME_PROPOSAL_MIN_ASPECT..=constants::FRAME_PROPOSAL_MAX_ASPECT)
        .contains(&aspect)
    {
        return None;
    }
    let allowed_deviation = if aspect < 1.0 {
        1.0 - constants::FRAME_PROPOSAL_MIN_ASPECT
    } else {
        constants::FRAME_PROPOSAL_MAX_ASPECT - 1.0
    };
    Some((1.0 - (aspect - 1.0).abs() / allowed_deviation).clamp(0.0, 1.0))
}

fn score_frame_rectangle(
    frame: &FrameView<'_>,
    left: FrameEdge,
    right: FrameEdge,
    top: FrameEdge,
    bottom: FrameEdge,
    aspect_agreement: f32,
) -> Option<u16> {
    let x_start = left.start;
    let x_end = right.end;
    let y_start = top.start;
    let y_end = bottom.end;
    if x_end <= x_start || y_end <= y_start {
        return None;
    }
    let right_inside = right.start.checked_sub(constants::ONE_INDEX)?;
    let bottom_inside = bottom.start.checked_sub(constants::ONE_INDEX)?;

    let top_evidence = measure_horizontal_side(frame, x_start, x_end, top, top.end)?;
    let bottom_evidence = measure_horizontal_side(frame, x_start, x_end, bottom, bottom_inside)?;
    let left_evidence = measure_vertical_side(frame, y_start, y_end, left, left.end)?;
    let right_evidence = measure_vertical_side(frame, y_start, y_end, right, right_inside)?;
    let sides = [top_evidence, bottom_evidence, left_evidence, right_evidence];

    let interior_x_start = left
        .end
        .checked_add(constants::FRAME_PROPOSAL_INTERIOR_INSET)?;
    let interior_x_end = right
        .start
        .checked_sub(constants::FRAME_PROPOSAL_INTERIOR_INSET)?;
    let interior_y_start = top
        .end
        .checked_add(constants::FRAME_PROPOSAL_INTERIOR_INSET)?;
    let interior_y_end = bottom
        .start
        .checked_sub(constants::FRAME_PROPOSAL_INTERIOR_INSET)?;
    let texture = sample_interior_texture(
        frame,
        interior_x_start,
        interior_x_end,
        interior_y_start,
        interior_y_end,
    )?;
    if texture.standard_deviation < constants::FRAME_PROPOSAL_MIN_TEXTURE_STDDEV {
        return None;
    }

    let mean_border_luma =
        sides.iter().map(|side| side.mean_border_luma).sum::<f32>() / sides.len() as f32;
    let darkness_gap = mean_border_luma - texture.mean_luma;
    if darkness_gap < constants::FRAME_PROPOSAL_MIN_DARKNESS_GAP {
        return None;
    }

    let minimum_edge_strength = left
        .strength
        .min(right.strength)
        .min(top.strength)
        .min(bottom.strength);
    let edge_quality =
        (minimum_edge_strength / constants::FRAME_PROPOSAL_FULL_EDGE_STRENGTH).clamp(0.0, 1.0);
    let agreement = sides.iter().map(|side| side.agreement).sum::<f32>() / sides.len() as f32;
    let local_contrast =
        sides.iter().map(|side| side.mean_contrast).sum::<f32>() / sides.len() as f32;
    let contrast_quality =
        (local_contrast / constants::FRAME_PROPOSAL_FULL_LOCAL_CONTRAST).clamp(0.0, 1.0);
    let texture_quality = (texture.standard_deviation
        / constants::FRAME_PROPOSAL_FULL_TEXTURE_STDDEV)
        .clamp(0.0, 1.0);
    let darkness_quality =
        (darkness_gap / constants::FRAME_PROPOSAL_FULL_DARKNESS_GAP).clamp(0.0, 1.0);
    let confidence = (constants::FRAME_PROPOSAL_CONFIDENCE_MAX
        * edge_quality
        * agreement
        * contrast_quality
        * texture_quality
        * darkness_quality
        * aspect_agreement)
        .round()
        .clamp(0.0, constants::FRAME_PROPOSAL_CONFIDENCE_MAX) as u16;
    Some(confidence)
}

fn measure_horizontal_side(
    frame: &FrameView<'_>,
    x_start: usize,
    x_end: usize,
    edge: FrameEdge,
    inward_y: usize,
) -> Option<SideEvidence> {
    let line_y = edge
        .start
        .checked_add(edge.end.checked_sub(edge.start)? / 2)?;
    measure_side(frame, x_start, x_end, line_y, inward_y, true)
}

fn measure_vertical_side(
    frame: &FrameView<'_>,
    y_start: usize,
    y_end: usize,
    edge: FrameEdge,
    inward_x: usize,
) -> Option<SideEvidence> {
    let line_x = edge
        .start
        .checked_add(edge.end.checked_sub(edge.start)? / 2)?;
    measure_side(frame, y_start, y_end, line_x, inward_x, false)
}

fn measure_side(
    frame: &FrameView<'_>,
    along_start: usize,
    along_end: usize,
    border_coordinate: usize,
    interior_coordinate: usize,
    horizontal: bool,
) -> Option<SideEvidence> {
    let inset = constants::FRAME_PROPOSAL_CORNER_INSET;
    let sample_start = along_start.checked_add(inset)?;
    let sample_end = along_end.checked_sub(inset)?;
    let sample_span = sample_end.checked_sub(sample_start)?;
    if sample_span < constants::FRAME_PROPOSAL_BORDER_SAMPLE_COUNT {
        return None;
    }

    let mut agreements = constants::FIRST_INDEX;
    let mut contrast_sum = 0.0_f32;
    let mut border_luma_sum = 0.0_f32;
    for index in constants::FIRST_INDEX..constants::FRAME_PROPOSAL_BORDER_SAMPLE_COUNT {
        let along = sample_start
            + index * (sample_span - constants::ONE_INDEX)
                / (constants::FRAME_PROPOSAL_BORDER_SAMPLE_COUNT - constants::ONE_INDEX);
        let (border_x, border_y, inside_x, inside_y) = if horizontal {
            (along, border_coordinate, along, interior_coordinate)
        } else {
            (border_coordinate, along, interior_coordinate, along)
        };
        let border = pixel_metrics(frame, border_x, border_y)?;
        let inside = pixel_metrics(frame, inside_x, inside_y)?;
        let contrast = border.gray_signal - inside.gray_signal;
        if contrast >= constants::FRAME_PROPOSAL_MIN_LOCAL_CONTRAST {
            agreements += constants::ONE_INDEX;
        }
        contrast_sum += contrast.max(0.0);
        border_luma_sum += border.luma;
    }
    let agreement = agreements as f32 / constants::FRAME_PROPOSAL_BORDER_SAMPLE_COUNT as f32;
    if agreement < constants::FRAME_PROPOSAL_MIN_SIDE_AGREEMENT {
        return None;
    }
    Some(SideEvidence {
        agreement,
        mean_contrast: contrast_sum / constants::FRAME_PROPOSAL_BORDER_SAMPLE_COUNT as f32,
        mean_border_luma: border_luma_sum / constants::FRAME_PROPOSAL_BORDER_SAMPLE_COUNT as f32,
    })
}

fn sample_interior_texture(
    frame: &FrameView<'_>,
    x_start: usize,
    x_end: usize,
    y_start: usize,
    y_end: usize,
) -> Option<TextureEvidence> {
    let radius = constants::FRAME_PROPOSAL_TEXTURE_PATCH_RADIUS;
    let sample_x_start = x_start.checked_add(radius)?;
    let sample_x_end = x_end.checked_sub(radius)?;
    let sample_y_start = y_start.checked_add(radius)?;
    let sample_y_end = y_end.checked_sub(radius)?;
    let x_span = sample_x_end.checked_sub(sample_x_start)?;
    let y_span = sample_y_end.checked_sub(sample_y_start)?;
    if x_span < constants::FRAME_PROPOSAL_TEXTURE_GRID_SIDE
        || y_span < constants::FRAME_PROPOSAL_TEXTURE_GRID_SIDE
    {
        return None;
    }

    let mut values = [0.0_f32; constants::FRAME_PROPOSAL_TEXTURE_SAMPLE_COUNT];
    let mut value_index = constants::FIRST_INDEX;
    for grid_y in constants::FIRST_INDEX..constants::FRAME_PROPOSAL_TEXTURE_GRID_SIDE {
        let y = sample_y_start
            + grid_y * (y_span - constants::ONE_INDEX)
                / (constants::FRAME_PROPOSAL_TEXTURE_GRID_SIDE - constants::ONE_INDEX);
        for grid_x in constants::FIRST_INDEX..constants::FRAME_PROPOSAL_TEXTURE_GRID_SIDE {
            let x = sample_x_start
                + grid_x * (x_span - constants::ONE_INDEX)
                    / (constants::FRAME_PROPOSAL_TEXTURE_GRID_SIDE - constants::ONE_INDEX);
            let mut patch_sum = 0.0_f32;
            let mut patch_count = 0.0_f32;
            for patch_y in y - radius..=y + radius {
                for patch_x in x - radius..=x + radius {
                    patch_sum += pixel_metrics(frame, patch_x, patch_y)?.luma;
                    patch_count += 1.0;
                }
            }
            values[value_index] = patch_sum / patch_count;
            value_index += constants::ONE_INDEX;
        }
    }

    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f32>()
        / values.len() as f32;
    Some(TextureEvidence {
        mean_luma: mean,
        standard_deviation: variance.sqrt(),
    })
}

#[derive(Clone, Copy)]
struct PixelMetrics {
    gray_signal: f32,
    luma: f32,
}

fn pixel_metrics(frame: &FrameView<'_>, x: usize, y: usize) -> Option<PixelMetrics> {
    let pixel_offset = y
        .checked_mul(frame.stride)?
        .checked_add(x.checked_mul(constants::CHANNELS_PER_PIXEL)?)?;
    let pixel_end = pixel_offset.checked_add(constants::CHANNELS_PER_PIXEL)?;
    let pixel = frame.bgra.get(pixel_offset..pixel_end)?;
    Some(PixelMetrics {
        gray_signal: gray_luminance_signal(pixel)?,
        luma: pixel_luminance(pixel)?,
    })
}

fn gray_luminance_signal(pixel: &[u8]) -> Option<f32> {
    let b = *pixel.get(constants::BLUE_CHANNEL)?;
    let g = *pixel.get(constants::GREEN_CHANNEL)?;
    let r = *pixel.get(constants::RED_CHANNEL)?;
    let low = b.min(g).min(r) as f32;
    let high = b.max(g).max(r) as f32;
    Some(
        (pixel_luminance(pixel)? - (high - low) * constants::FRAME_PROPOSAL_CHROMA_PENALTY)
            .max(0.0),
    )
}

fn pixel_luminance(pixel: &[u8]) -> Option<f32> {
    Some(
        *pixel.get(constants::RED_CHANNEL)? as f32 * constants::RED_LUMA_WEIGHT as f32
            + *pixel.get(constants::GREEN_CHANNEL)? as f32 * constants::GREEN_LUMA_WEIGHT as f32
            + *pixel.get(constants::BLUE_CHANNEL)? as f32 * constants::BLUE_LUMA_WEIGHT as f32,
    )
}
