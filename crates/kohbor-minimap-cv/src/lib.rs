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

/// Count colour-ring marker candidates in a BGRA crop.
///
/// `stride` is the byte distance between row starts. A malformed or empty
/// buffer returns `structural_ok == false` and no blobs. `enemy` selects the
/// enemy ring colour; the reported coordinates are crop-relative milli-units.
/// All thresholds and target colours are proposed and unverified for the
/// current game patch.
pub fn analyze(bgra: &[u8], width: u32, height: u32, stride: u32, enemy: Team) -> PixelReport {
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
        return PixelReport {
            structural_ok,
            enemy_blobs: Vec::new(),
        };
    };

    let mut grid = Vec::new();
    if grid.try_reserve_exact(grid_len).is_err() {
        return PixelReport {
            structural_ok,
            enemy_blobs: Vec::new(),
        };
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
        return PixelReport {
            structural_ok,
            enemy_blobs: Vec::new(),
        };
    };
    let mut merged = Vec::new();
    for candidate in candidates.drain(..) {
        if !merge_candidate(&mut merged, candidate, icon_side) {
            return PixelReport {
                structural_ok,
                enemy_blobs: Vec::new(),
            };
        }
    }

    let mut enemy_blobs = Vec::new();
    if enemy_blobs.try_reserve_exact(merged.len()).is_err() {
        return PixelReport {
            structural_ok,
            enemy_blobs,
        };
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
        let border = constants::BORDER_WIDTH as usize;
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
            || border_count == constants::FIRST_INDEX
            || interior_count == constants::FIRST_INDEX
        {
            return false;
        }

        let interior_mean = interior_sum / interior_count as f64;
        let border_mean = border_sum / border_count as f64;
        let variance = (interior_square_sum / interior_count as f64
            - interior_mean * interior_mean)
            .max(constants::ZERO_SCORE);
        let interior_stddev = variance.sqrt();
        border_mean < interior_mean
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
    use super::{merge_candidate, Candidate};

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
