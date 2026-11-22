//! Geometry and local identity matching for the draft-bar view.
//!
//! Geometry is measured from one owner frame on 2026-11-21. It is supported only
//! for the exact 1920x1080, 100% output until another game patch is measured.

use kohbor_capture::Rect;
use kohbor_game_assets::{decode_vtex_c, hero_portrait_path, Image, Vpk};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::time::{Duration, Instant};

pub const OUTPUT_WIDTH: u32 = 1920;
pub const OUTPUT_HEIGHT: u32 = 1080;
pub const SCALE_PERCENT: u32 = 100;
pub const SLOT_COUNT: usize = 10;

// The supplied WebP is 1919x1072. Its measured origin is one pixel right and
// eight pixels down from the duplicated 1920x1080 output; geometry below applies
// that (1, 8) correction to the measured WebP coordinates.
const STRIP_X: i32 = 209;
const STRIP_Y: i32 = 8;
const STRIP_WIDTH: u32 = 1494;
const STRIP_HEIGHT: u32 = 119;
const SLOT_WIDTH: u32 = 119;
const SLOT_HEIGHT: u32 = 65;
const SLOT_STEP: i32 = 124;
const RADIANT_X0: i32 = 209;
const DIRE_X0: i32 = 1088;
const FEATURE_WIDTH: usize = 32;
const FEATURE_HEIGHT: usize = 18;
const TILE_COLUMNS: usize = 4;
const TILE_ROWS: usize = 3;
const MIN_VALID_TILES: usize = 8;
const MIN_SLOT_CONTRAST: f32 = 8.0;
const MIN_MATCH_CONFIDENCE: f32 = 0.82;
const MIN_MATCH_MARGIN: f32 = 0.08;
const ROSTER_PATH: &str = "scripts/npc/npc_heroes.txt";
const MAX_ROSTER_BYTES: usize = 8 * 1024 * 1024;
const FRAME_FRESHNESS: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub reason: String,
}

impl fmt::Display for Unsupported {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for Unsupported {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DraftBarGeometry {
    /// Complete top-bar mirror, including the clock gap and player labels.
    pub strip_rect: Rect,
    /// The portrait image rectangles used for identity matching; labels are below these.
    pub slot_rects: [Rect; SLOT_COUNT],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchReason {
    Identified,
    EmptySlot,
    LowContrast,
    LowConfidence,
    ReferencesUnavailable,
    AwaitingFrame,
}

/// Dedicated deck projection. It deliberately contains neither names/ranks nor pixels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftBarSlotView {
    pub slot: usize,
    pub team: &'static str,
    pub hero_id: Option<String>,
    pub confidence: f32,
    pub reason: MatchReason,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftBarDeckView {
    pub active: bool,
    pub slots: Vec<DraftBarSlotView>,
}

/// The control window receives one atomic snapshot. No event broadcasts carry this data.
#[derive(Serialize)]
struct DeckPacket<'a> {
    #[serde(flatten)]
    view: DraftBarDeckView,
    detail: Option<&'a str>,
    has_frame: bool,
}

#[derive(Default)]
pub(crate) struct DraftBarMemory {
    epoch: Option<u64>,
    active: bool,
    detail: Option<String>,
    matcher: Option<DraftBarMatcher>,
    pixels: Vec<u8>,
    frame_at: Option<Instant>,
}

impl DraftBarMemory {
    pub fn observe(&mut self, epoch: Option<u64>, match_ended: bool, active: bool) {
        let epoch = if match_ended { None } else { epoch };
        if should_clear_session(self.epoch, epoch, match_ended) {
            self.withdraw(None);
            self.matcher = None;
        }
        self.epoch = epoch;
        self.active = active && epoch.is_some();
        if !self.active {
            self.withdraw(None);
        }
    }

    pub fn withdraw(&mut self, detail: Option<String>) {
        self.pixels.fill(0);
        self.pixels.clear();
        self.frame_at = None;
        self.detail = detail;
        if let Some(matcher) = &mut self.matcher {
            matcher.clear_slots();
        }
    }

    /// Log-safe summary: whether a frame is held, the withdraw reason, and how many slots are
    /// identified. Never returns hero names, pixels or confidences.
    pub(crate) fn log_summary(&self) -> (bool, Option<&str>, usize) {
        let identified = self.matcher.as_ref().map_or(0, |matcher| {
            matcher
                .deck_view(true)
                .slots
                .iter()
                .filter(|slot| slot.reason == MatchReason::Identified)
                .count()
        });
        (self.frame_at.is_some(), self.detail.as_deref(), identified)
    }

    pub fn needs_references(&self) -> bool {
        self.active && self.matcher.is_none()
    }

    pub fn install_references(&mut self, matcher: DraftBarMatcher) {
        self.matcher = Some(matcher);
    }

    pub fn accept(&mut self, pixels: &[u8], geometry: &DraftBarGeometry, at: Instant) {
        if let Some(matcher) = &mut self.matcher {
            let width = geometry.strip_rect.width;
            matcher.identify_strip(
                pixels,
                width,
                geometry.strip_rect.height,
                width * 4,
                geometry,
            );
        }
        self.pixels.resize(pixels.len(), 0);
        self.pixels.copy_from_slice(pixels);
        self.frame_at = Some(at);
        self.detail = None;
    }

    /// Binary control-window IPC only. Four little-endian bytes give the JSON
    /// metadata length, followed by metadata and optionally tight 1494x119 BGRA.
    pub fn packet(
        &self,
        admitted_epoch: Option<u64>,
        now: Instant,
        display_enabled: bool,
    ) -> Result<Vec<u8>, String> {
        let active = self.active && admitted_epoch.is_some() && self.epoch == admitted_epoch;
        let has_frame = active
            && display_enabled
            && self
                .frame_at
                .is_some_and(|at| now.saturating_duration_since(at) <= FRAME_FRESHNESS);
        let view = if has_frame {
            self.matcher.as_ref().map_or_else(
                || DraftBarMatcher::empty().deck_view(true),
                |matcher| matcher.deck_view(true),
            )
        } else {
            DraftBarMatcher::empty().deck_view(active)
        };
        let metadata = serde_json::to_vec(&DeckPacket {
            view,
            detail: self.detail.as_deref().filter(|_| active),
            has_frame,
        })
        .map_err(|_| "draft bar response unavailable".to_owned())?;
        let pixels = if has_frame {
            self.pixels.as_slice()
        } else {
            &[]
        };
        let mut packet = Vec::with_capacity(4 + metadata.len() + pixels.len());
        packet.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        packet.extend_from_slice(&metadata);
        packet.extend_from_slice(pixels);
        Ok(packet)
    }
}

impl Drop for DraftBarMemory {
    fn drop(&mut self) {
        self.withdraw(None);
    }
}

/// Aggregate-only cost/sample fields; no hero IDs, names, ranks, or images enter diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DraftBarDiagnostics {
    pub changed_slots: u8,
    pub identified_slots: u8,
    pub unknown_slots: u8,
    pub confidence_bins: [u8; 5],
    pub cycle_ms: f32,
}

struct ReferenceTemplate {
    hero_id: Vec<u8>,
    features: Vec<u8>,
}

struct CachedSlot {
    fingerprint: [u8; 32],
    result: SlotResult,
}

struct SlotResult {
    hero_id: Option<Vec<u8>>,
    confidence: f32,
    reason: MatchReason,
}

struct BgraFrame<'a> {
    pixels: &'a [u8],
    width: u32,
    height: u32,
    stride: u32,
}

/// Session-local references and changed-slot cache. Images and IDs are never serialized here.
pub struct DraftBarMatcher {
    references: Vec<ReferenceTemplate>,
    slots: [Option<CachedSlot>; SLOT_COUNT],
}

impl DraftBarMatcher {
    pub fn empty() -> Self {
        Self {
            references: Vec::new(),
            slots: std::array::from_fn(|_| None),
        }
    }

    /// Read the hero roster and root hero images from the user's VPK into memory once.
    /// The root `heroes/` images are the landscape reference family for the top bar.
    pub fn from_vpk(vpk: &Vpk) -> Self {
        let mut matcher = Self::empty();
        let Some(roster_entry) = vpk.find(ROSTER_PATH) else {
            return matcher;
        };
        if roster_entry.encoded_len() > MAX_ROSTER_BYTES as u64 {
            return matcher;
        }
        let Ok(roster) = vpk.read(&roster_entry) else {
            return matcher;
        };
        if roster.len() > MAX_ROSTER_BYTES {
            return matcher;
        }
        for hero_id in parse_hero_ids(&roster) {
            let Some(path) = hero_portrait_path(&hero_id).into_iter().next() else {
                continue;
            };
            let Some(entry) = vpk.find(&path) else {
                continue;
            };
            let Ok(bytes) = vpk.read(&entry) else {
                continue;
            };
            let Ok(image) = decode_vtex_c(&bytes) else {
                continue;
            };
            let Some(features) = rgba_features(&image) else {
                continue;
            };
            matcher.references.push(ReferenceTemplate {
                hero_id: hero_id.into_bytes(),
                features,
            });
        }
        matcher
            .references
            .sort_by(|left, right| left.hero_id.cmp(&right.hero_id));
        matcher
    }

    #[cfg(test)]
    pub fn reference_count(&self) -> usize {
        self.references.len()
    }

    /// Match each slot in a captured strip. A slot is scored again only when its pixels change.
    pub fn identify_strip(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        stride: u32,
        geometry: &DraftBarGeometry,
    ) -> DraftBarDiagnostics {
        let started = std::time::Instant::now();
        let frame = BgraFrame {
            pixels,
            width,
            height,
            stride,
        };
        let mut changed_slots = 0_u8;
        for (index, rect) in geometry.slot_rects.iter().enumerate() {
            let local = Rect {
                x: rect.x - geometry.strip_rect.x,
                y: rect.y - geometry.strip_rect.y,
                ..*rect
            };
            if let Some(changed) = self.identify_slot(index, &frame, local) {
                changed_slots = changed_slots.saturating_add(u8::from(changed));
            }
        }
        let mut identified_slots = 0_u8;
        let mut confidence_bins = [0_u8; 5];
        for cached in self.slots.iter().flatten() {
            if cached.result.hero_id.is_some() {
                identified_slots = identified_slots.saturating_add(1);
            }
            let bin = ((cached.result.confidence.clamp(0.0, 0.999_999) * 5.0) as usize).min(4);
            confidence_bins[bin] = confidence_bins[bin].saturating_add(1);
        }
        DraftBarDiagnostics {
            changed_slots,
            identified_slots,
            unknown_slots: SLOT_COUNT as u8 - identified_slots,
            confidence_bins,
            cycle_ms: started.elapsed().as_secs_f32() * 1000.0,
        }
    }

    fn identify_slot(&mut self, slot: usize, frame: &BgraFrame<'_>, rect: Rect) -> Option<bool> {
        if slot >= SLOT_COUNT || !frame.contains(rect) {
            return None;
        }
        let fingerprint = frame.fingerprint(rect)?;
        if self.slots[slot]
            .as_ref()
            .is_some_and(|cached| cached.fingerprint == fingerprint)
        {
            return Some(false);
        }
        let result = if self.references.is_empty() {
            SlotResult::unknown(0.0, MatchReason::ReferencesUnavailable)
        } else {
            let features = frame.features(rect)?;
            self.classify(&features)
        };
        self.slots[slot] = Some(CachedSlot {
            fingerprint,
            result,
        });
        Some(true)
    }

    fn classify(&self, features: &[u8]) -> SlotResult {
        let contrast = grayscale_contrast(features);
        let mean = grayscale_mean(features);
        if mean < 14.0 && contrast < MIN_SLOT_CONTRAST {
            return SlotResult::unknown(0.0, MatchReason::EmptySlot);
        }
        if contrast < MIN_SLOT_CONTRAST {
            return SlotResult::unknown(0.0, MatchReason::LowContrast);
        }

        let mut scored = self
            .references
            .iter()
            .filter_map(|reference| {
                tile_similarity(&reference.features, features)
                    .map(|score| (score, reference.hero_id.as_slice()))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| right.0.total_cmp(&left.0).then_with(|| left.1.cmp(right.1)));
        let Some((confidence, hero_id)) = scored.first().copied() else {
            return SlotResult::unknown(0.0, MatchReason::LowConfidence);
        };
        let runner_up = scored.get(1).map_or(0.0, |score| score.0);
        if confidence < MIN_MATCH_CONFIDENCE || confidence - runner_up < MIN_MATCH_MARGIN {
            return SlotResult::unknown(confidence.clamp(0.0, 1.0), MatchReason::LowConfidence);
        }
        SlotResult {
            hero_id: Some(hero_id.to_vec()),
            confidence: confidence.clamp(0.0, 1.0),
            reason: MatchReason::Identified,
        }
    }

    /// Return the sole JSON view for the control-window deck, with no raw pixels or player data.
    pub fn deck_view(&self, active: bool) -> DraftBarDeckView {
        let slots = (0..SLOT_COUNT)
            .map(|index| {
                let cached = self.slots[index].as_ref();
                let result = cached.map(|cached| &cached.result);
                DraftBarSlotView {
                    slot: index,
                    team: if index < 5 { "radiant" } else { "dire" },
                    hero_id: result
                        .and_then(|result| result.hero_id.as_deref())
                        .and_then(|bytes| std::str::from_utf8(bytes).ok())
                        .map(str::to_owned),
                    confidence: result.map_or(0.0, |result| result.confidence),
                    reason: result.map_or(MatchReason::AwaitingFrame, |result| result.reason),
                }
            })
            .collect();
        DraftBarDeckView { active, slots }
    }

    /// Best-effort zeroing before a session transition or app exit.
    pub fn clear(&mut self) {
        for reference in &mut self.references {
            reference.hero_id.fill(0);
            reference.features.fill(0);
        }
        self.references.clear();
        self.clear_slots();
    }

    fn clear_slots(&mut self) {
        for cached in &mut self.slots {
            if let Some(mut cached) = cached.take() {
                cached.fingerprint.fill(0);
                if let Some(hero_id) = &mut cached.result.hero_id {
                    hero_id.fill(0);
                }
            }
        }
    }
}

/// The caller clears pixels/references when a session fence changes or post-game is admitted.
pub fn should_clear_session(
    previous_epoch: Option<u64>,
    next_epoch: Option<u64>,
    match_ended: bool,
) -> bool {
    match_ended || previous_epoch != next_epoch
}

impl Drop for DraftBarMatcher {
    fn drop(&mut self) {
        self.clear();
    }
}

impl SlotResult {
    fn unknown(confidence: f32, reason: MatchReason) -> Self {
        Self {
            hero_id: None,
            confidence: confidence.clamp(0.0, 1.0),
            reason,
        }
    }
}

fn parse_hero_ids(roster: &[u8]) -> Vec<String> {
    if roster.len() > MAX_ROSTER_BYTES {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(roster);
    let prefix = "npc_dota_hero_";
    let mut found = HashSet::new();
    let mut insert = |token: &str| {
        if let Some(suffix) = token.strip_prefix(prefix) {
            if (1..=64).contains(&suffix.len())
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            {
                found.insert(token.to_owned());
            }
        }
    };
    for line in text.lines() {
        let (code, _) = line.split_once("//").unwrap_or((line, ""));
        let code = code.trim_start();
        if let Some(include) = code.strip_prefix("#base ") {
            // The installed roster uses #base "…/npc_dota_hero_<id>.txt".
            if let Some(id) = include
                .split('"')
                .nth(1)
                .and_then(|path| path.rsplit(['/', '\\']).next())
                .and_then(|file| file.strip_suffix(".txt"))
            {
                insert(id);
            }
        } else {
            for token in code.split('"').skip(1).step_by(2) {
                insert(token);
            }
        }
    }
    let mut heroes = found.into_iter().collect::<Vec<_>>();
    heroes.sort();
    heroes
}

fn rgba_features(image: &Image) -> Option<Vec<u8>> {
    let expected = usize::try_from(image.width)
        .ok()?
        .checked_mul(usize::try_from(image.height).ok()?)?
        .checked_mul(4)?;
    if image.width == 0 || image.height == 0 || image.rgba.len() != expected {
        return None;
    }
    let mut output = vec![0; FEATURE_WIDTH * FEATURE_HEIGHT];
    for y in 0..FEATURE_HEIGHT {
        let source_y = resample_coordinate(y, FEATURE_HEIGHT, image.height as usize)?;
        for x in 0..FEATURE_WIDTH {
            let source_x = resample_coordinate(x, FEATURE_WIDTH, image.width as usize)?;
            let source = (source_y * image.width as usize + source_x) * 4;
            output[y * FEATURE_WIDTH + x] = luminance(
                image.rgba[source],
                image.rgba[source + 1],
                image.rgba[source + 2],
            );
        }
    }
    Some(output)
}

impl BgraFrame<'_> {
    fn features(&self, rect: Rect) -> Option<Vec<u8>> {
        let mut output = vec![0; FEATURE_WIDTH * FEATURE_HEIGHT];
        for target_y in 0..FEATURE_HEIGHT {
            let source_y = rect.y as usize
                + resample_coordinate(target_y, FEATURE_HEIGHT, rect.height as usize)?;
            for target_x in 0..FEATURE_WIDTH {
                let source_x = rect.x as usize
                    + resample_coordinate(target_x, FEATURE_WIDTH, rect.width as usize)?;
                let source = source_y * self.stride as usize + source_x * 4;
                output[target_y * FEATURE_WIDTH + target_x] = luminance(
                    self.pixels[source + 2],
                    self.pixels[source + 1],
                    self.pixels[source],
                );
            }
        }
        Some(output)
    }

    fn fingerprint(&self, rect: Rect) -> Option<[u8; 32]> {
        let mut hasher = Sha256::new();
        let row_bytes = usize::try_from(rect.width).ok()?.checked_mul(4)?;
        for row in rect.y as usize..(rect.y as usize).checked_add(rect.height as usize)? {
            let start = row
                .checked_mul(self.stride as usize)?
                .checked_add(rect.x as usize * 4)?;
            hasher.update(self.pixels.get(start..start.checked_add(row_bytes)?)?);
        }
        Some(hasher.finalize().into())
    }

    fn contains(&self, rect: Rect) -> bool {
        if rect.x < 0
            || rect.y < 0
            || rect.width == 0
            || rect.height == 0
            || self
                .width
                .checked_mul(4)
                .is_none_or(|row| self.stride < row)
        {
            return false;
        }
        let len = (self.height as usize).checked_mul(self.stride as usize);
        let right = (rect.x as u32).checked_add(rect.width);
        let bottom = (rect.y as u32).checked_add(rect.height);
        right.is_some_and(|right| right <= self.width)
            && bottom.is_some_and(|bottom| bottom <= self.height)
            && len == Some(self.pixels.len())
    }
}

fn tile_similarity(left: &[u8], right: &[u8]) -> Option<f32> {
    if left.len() != FEATURE_WIDTH * FEATURE_HEIGHT || right.len() != left.len() {
        return None;
    }
    let tile_width = FEATURE_WIDTH / TILE_COLUMNS;
    let tile_height = FEATURE_HEIGHT / TILE_ROWS;
    let mut scores = Vec::with_capacity(TILE_COLUMNS * TILE_ROWS);
    for tile_y in 0..TILE_ROWS {
        for tile_x in 0..TILE_COLUMNS {
            let mut left_values = Vec::with_capacity(tile_width * tile_height);
            let mut right_values = Vec::with_capacity(tile_width * tile_height);
            for y in tile_y * tile_height..(tile_y + 1) * tile_height {
                let row_start = y * FEATURE_WIDTH + tile_x * tile_width;
                left_values.extend_from_slice(&left[row_start..row_start + tile_width]);
                right_values.extend_from_slice(&right[row_start..row_start + tile_width]);
            }
            if let Some(score) = normalized_correlation(&left_values, &right_values) {
                scores.push(score);
            }
        }
    }
    if scores.len() < MIN_VALID_TILES {
        return None;
    }
    scores.sort_by(|a, b| b.total_cmp(a));
    let retained = scores.into_iter().take(MIN_VALID_TILES).collect::<Vec<_>>();
    Some(retained.iter().sum::<f32>() / retained.len() as f32)
}

fn normalized_correlation(left: &[u8], right: &[u8]) -> Option<f32> {
    if left.len() != right.len() || left.is_empty() {
        return None;
    }
    let left_mean = left.iter().map(|value| f32::from(*value)).sum::<f32>() / left.len() as f32;
    let right_mean = right.iter().map(|value| f32::from(*value)).sum::<f32>() / right.len() as f32;
    let mut covariance = 0.0;
    let mut left_variance = 0.0;
    let mut right_variance = 0.0;
    for (left, right) in left.iter().zip(right) {
        let left_delta = f32::from(*left) - left_mean;
        let right_delta = f32::from(*right) - right_mean;
        covariance += left_delta * right_delta;
        left_variance += left_delta * left_delta;
        right_variance += right_delta * right_delta;
    }
    if left_variance < 1.0 || right_variance < 1.0 {
        return None;
    }
    Some((covariance / (left_variance * right_variance).sqrt()).clamp(-1.0, 1.0))
}

fn grayscale_mean(features: &[u8]) -> f32 {
    features.iter().map(|value| f32::from(*value)).sum::<f32>() / features.len().max(1) as f32
}

fn grayscale_contrast(features: &[u8]) -> f32 {
    let mean = grayscale_mean(features);
    (features
        .iter()
        .map(|value| (f32::from(*value) - mean).powi(2))
        .sum::<f32>()
        / features.len().max(1) as f32)
        .sqrt()
}

fn resample_coordinate(target: usize, target_len: usize, source_len: usize) -> Option<usize> {
    if target_len == 0 || source_len == 0 {
        return None;
    }
    Some((((target * 2 + 1) * source_len) / (target_len * 2)).min(source_len - 1))
}

fn luminance(red: u8, green: u8, blue: u8) -> u8 {
    ((u32::from(red) * 77 + u32::from(green) * 150 + u32::from(blue) * 29) >> 8) as u8
}

/// Return the measured owner-frame geometry for the only supported output/scale.
pub fn draft_bar_geometry(
    output_width: u32,
    output_height: u32,
    scale_percent: u32,
) -> Result<DraftBarGeometry, Unsupported> {
    if output_width != OUTPUT_WIDTH || output_height != OUTPUT_HEIGHT {
        return Err(Unsupported {
            reason: format!(
                "draft bar supports {OUTPUT_WIDTH}x{OUTPUT_HEIGHT} only; observed {output_width}x{output_height}"
            ),
        });
    }
    if scale_percent != SCALE_PERCENT {
        return Err(Unsupported {
            reason: format!(
                "draft bar supports {SCALE_PERCENT}% scale only; observed {scale_percent}%"
            ),
        });
    }

    let team_slots = |x0| -> [Rect; 5] {
        std::array::from_fn(|index| Rect {
            x: x0 + SLOT_STEP * index as i32,
            y: STRIP_Y,
            width: SLOT_WIDTH,
            height: SLOT_HEIGHT,
        })
    };
    let radiant = team_slots(RADIANT_X0);
    let dire = team_slots(DIRE_X0);
    let slot_rects: [Rect; SLOT_COUNT] = std::array::from_fn(|index| {
        if index < 5 {
            radiant[index]
        } else {
            dire[index - 5]
        }
    });
    let strip_rect = Rect {
        x: STRIP_X,
        y: STRIP_Y,
        width: STRIP_WIDTH,
        height: STRIP_HEIGHT,
    };
    if strip_rect.x < 0
        || strip_rect.y < 0
        || strip_rect.x as u32 + strip_rect.width > output_width
        || strip_rect.y as u32 + strip_rect.height > output_height
        || slot_rects.iter().any(|rect| {
            rect.x < strip_rect.x
                || rect.y < strip_rect.y
                || rect.x + rect.width as i32 > strip_rect.x + strip_rect.width as i32
                || rect.y + rect.height as i32 > strip_rect.y + strip_rect.height as i32
        })
    {
        return Err(Unsupported {
            reason: "measured draft-bar geometry falls outside the observed output".to_owned(),
        });
    }

    Ok(DraftBarGeometry {
        strip_rect,
        slot_rects,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn feature_art(seed: u32) -> Vec<u8> {
        let mut state = seed.wrapping_add(1).wrapping_mul(0x9e37_79b9);
        let mut features = Vec::with_capacity(FEATURE_WIDTH * FEATURE_HEIGHT);
        for _ in 0..FEATURE_WIDTH * FEATURE_HEIGHT {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            features.push((state >> 24) as u8);
        }
        features
    }

    fn synthetic_medal_placeholder() -> Vec<u8> {
        let mut features = vec![22; FEATURE_WIDTH * FEATURE_HEIGHT];
        let center_x = (FEATURE_WIDTH as f32 - 1.0) / 2.0;
        let center_y = (FEATURE_HEIGHT as f32 - 1.0) / 2.0;
        for y in 0..FEATURE_HEIGHT {
            for x in 0..FEATURE_WIDTH {
                let dx = (x as f32 - center_x) / center_x;
                let dy = (y as f32 - center_y) / center_y;
                let radius = (dx * dx + dy * dy).sqrt();
                let value = if radius < 0.72 {
                    if radius > 0.58 {
                        224
                    } else if (x + y) % 5 == 0 {
                        172
                    } else {
                        86
                    }
                } else if x == 0 || x + 1 == FEATURE_WIDTH {
                    112
                } else {
                    22
                };
                features[y * FEATURE_WIDTH + x] = value;
            }
        }
        features
    }

    pub(crate) fn synthetic_matcher() -> DraftBarMatcher {
        let first = feature_art(1);
        let second = feature_art(2);
        DraftBarMatcher {
            references: vec![
                ReferenceTemplate {
                    hero_id: b"npc_dota_hero_synthetic_one".to_vec(),
                    features: first,
                },
                ReferenceTemplate {
                    hero_id: b"npc_dota_hero_synthetic_two".to_vec(),
                    features: second,
                },
            ],
            slots: std::array::from_fn(|_| None),
        }
    }

    fn blank_strip(geometry: &DraftBarGeometry, color: u8) -> Vec<u8> {
        vec![color; geometry.strip_rect.width as usize * geometry.strip_rect.height as usize * 4]
    }

    fn paint_features(
        strip: &mut [u8],
        geometry: &DraftBarGeometry,
        slot: usize,
        features: &[u8],
        dim: f32,
    ) {
        let rect = geometry.slot_rects[slot];
        let x = (rect.x - geometry.strip_rect.x) as usize;
        let y = (rect.y - geometry.strip_rect.y) as usize;
        let width = rect.width as usize;
        let height = rect.height as usize;
        let stride = geometry.strip_rect.width as usize * 4;
        for target_y in 0..height {
            let feature_y = resample_coordinate(target_y, height, FEATURE_HEIGHT).unwrap();
            for target_x in 0..width {
                let feature_x = resample_coordinate(target_x, width, FEATURE_WIDTH).unwrap();
                let value = (f32::from(features[feature_y * FEATURE_WIDTH + feature_x]) * dim)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                let offset = (y + target_y) * stride + (x + target_x) * 4;
                strip[offset] = value;
                strip[offset + 1] = value;
                strip[offset + 2] = value;
                strip[offset + 3] = 255;
            }
        }
    }

    pub(crate) fn synthetic_strip(geometry: &DraftBarGeometry) -> Vec<u8> {
        let mut strip = blank_strip(geometry, 0);
        paint_features(&mut strip, geometry, 0, &feature_art(1), 1.0);
        strip
    }

    #[test]
    fn owner_frame_geometry_corrects_the_saved_one_by_eight_crop() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        assert_eq!(
            geometry.strip_rect,
            Rect {
                x: 209,
                y: 8,
                width: 1494,
                height: 119
            }
        );
        assert_eq!(
            geometry.slot_rects[0],
            Rect {
                x: 209,
                y: 8,
                width: 119,
                height: 65
            }
        );
        assert_eq!(geometry.slot_rects[4].x, 705);
        assert_eq!(geometry.slot_rects[5].x, 1088);
        assert_eq!(geometry.slot_rects[9].x, 1584);
    }

    #[test]
    fn unsupported_resolution_reports_the_observed_output() {
        assert!(draft_bar_geometry(1919, 1080, 100)
            .unwrap_err()
            .reason
            .contains("1919x1080"));
        assert!(draft_bar_geometry(1920, 1072, 100)
            .unwrap_err()
            .reason
            .contains("1920x1072"));
    }

    #[test]
    fn unsupported_scale_reports_the_observed_percent() {
        assert!(draft_bar_geometry(1920, 1080, 125)
            .unwrap_err()
            .reason
            .contains("125%"));
    }

    #[test]
    fn matcher_returns_only_hero_ids_above_both_conservative_thresholds() {
        let matcher = synthetic_matcher();
        let features = matcher.references[0].features.clone();
        let result = matcher.classify(&features);
        assert_eq!(result.reason, MatchReason::Identified);
        assert_eq!(
            result.hero_id.as_deref(),
            Some(b"npc_dota_hero_synthetic_one".as_slice())
        );
        assert!(result.confidence >= MIN_MATCH_CONFIDENCE);
    }

    #[test]
    fn synthetic_ten_slot_composite_identifies_and_keeps_unknowns_unknown() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let mut strip = blank_strip(&geometry, 0);
        let first = feature_art(1);
        paint_features(&mut strip, &geometry, 0, &first, 1.0);
        let mut matcher = synthetic_matcher();
        let diagnostics = matcher.identify_strip(
            &strip,
            geometry.strip_rect.width,
            geometry.strip_rect.height,
            geometry.strip_rect.width * 4,
            &geometry,
        );
        assert_eq!(diagnostics.changed_slots, 10);
        let view = matcher.deck_view(true);
        assert_eq!(
            view.slots[0].hero_id.as_deref(),
            Some("npc_dota_hero_synthetic_one")
        );
        assert_eq!(view.slots[0].reason, MatchReason::Identified);
        assert_eq!(view.slots[1].hero_id, None);
        assert_eq!(view.slots[1].reason, MatchReason::EmptySlot);
    }

    #[test]
    fn unchanged_slots_are_not_rematched_and_changed_pixels_are() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let mut strip = blank_strip(&geometry, 0);
        let first = feature_art(1);
        paint_features(&mut strip, &geometry, 0, &first, 1.0);
        let mut matcher = synthetic_matcher();
        assert_eq!(
            matcher
                .identify_strip(
                    &strip,
                    geometry.strip_rect.width,
                    geometry.strip_rect.height,
                    geometry.strip_rect.width * 4,
                    &geometry,
                )
                .changed_slots,
            10
        );
        assert_eq!(
            matcher
                .identify_strip(
                    &strip,
                    geometry.strip_rect.width,
                    geometry.strip_rect.height,
                    geometry.strip_rect.width * 4,
                    &geometry,
                )
                .changed_slots,
            0
        );
        let offset = ((geometry.slot_rects[0].y - geometry.strip_rect.y) as usize)
            * geometry.strip_rect.width as usize
            * 4
            + (geometry.slot_rects[0].x - geometry.strip_rect.x) as usize * 4;
        strip[offset] ^= 0xff;
        assert_eq!(
            matcher
                .identify_strip(
                    &strip,
                    geometry.strip_rect.width,
                    geometry.strip_rect.height,
                    geometry.strip_rect.width * 4,
                    &geometry,
                )
                .changed_slots,
            1
        );
    }

    #[test]
    fn blank_medal_heavy_overlay_and_low_contrast_do_not_produce_a_guess() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let mut matcher = synthetic_matcher();
        let blank = vec![96; FEATURE_WIDTH * FEATURE_HEIGHT];
        assert_eq!(matcher.classify(&blank).hero_id, None);

        let medal_placeholder = synthetic_medal_placeholder();
        assert_eq!(matcher.classify(&medal_placeholder).hero_id, None);

        let mut heavily_covered = feature_art(1);
        for y in 0..FEATURE_HEIGHT * 2 / 3 {
            for x in 0..FEATURE_WIDTH {
                heavily_covered[y * FEATURE_WIDTH + x] = if y % 2 == 0 { 220 } else { 30 };
            }
        }
        assert_eq!(matcher.classify(&heavily_covered).hero_id, None);

        let mut low_contrast_strip = blank_strip(&geometry, 0);
        let low_contrast = feature_art(1)
            .iter()
            .map(|value| (96_i16 + i16::from(*value % 3) - 1) as u8)
            .collect::<Vec<_>>();
        paint_features(&mut low_contrast_strip, &geometry, 0, &low_contrast, 1.0);
        let result = matcher.identify_strip(
            &low_contrast_strip,
            geometry.strip_rect.width,
            geometry.strip_rect.height,
            geometry.strip_rect.width * 4,
            &geometry,
        );
        assert_eq!(matcher.deck_view(true).slots[0].hero_id, None);
        assert_eq!(
            matcher.deck_view(true).slots[0].reason,
            MatchReason::LowContrast
        );
        assert!(result.changed_slots > 0);
    }

    #[test]
    fn normalized_tiles_tolerate_dimmed_locked_portraits() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let mut strip = blank_strip(&geometry, 0);
        let first = feature_art(1)
            .into_iter()
            .map(|value| (f32::from(value) * 0.45 + 12.0).round() as u8)
            .collect::<Vec<_>>();
        paint_features(&mut strip, &geometry, 0, &first, 1.0);
        let mut matcher = synthetic_matcher();
        matcher.identify_strip(
            &strip,
            geometry.strip_rect.width,
            geometry.strip_rect.height,
            geometry.strip_rect.width * 4,
            &geometry,
        );
        assert_eq!(
            matcher.deck_view(true).slots[0].reason,
            MatchReason::Identified
        );
    }

    #[test]
    fn covered_top_row_still_leaves_eight_matching_tiles() {
        let matcher = synthetic_matcher();
        let mut covered = feature_art(1);
        for (i, value) in covered[..FEATURE_WIDTH * (FEATURE_HEIGHT / TILE_ROWS)]
            .iter_mut()
            .enumerate()
        {
            *value = if i % 3 == 0 { 225 } else { 16 };
        }
        assert_eq!(matcher.classify(&covered).reason, MatchReason::Identified);
    }

    #[test]
    fn session_transition_and_match_end_clear_references_and_cached_hero_ids() {
        let mut matcher = synthetic_matcher();
        assert_eq!(matcher.reference_count(), 2);
        assert!(!should_clear_session(Some(7), Some(7), false));
        if should_clear_session(Some(7), Some(8), false) {
            matcher.clear();
        }
        assert_eq!(matcher.reference_count(), 0);
        assert_eq!(matcher.deck_view(false).slots[0].hero_id, None);

        let mut matcher = synthetic_matcher();
        if should_clear_session(Some(8), Some(8), true) {
            matcher.clear();
        }
        assert_eq!(matcher.reference_count(), 0);
    }

    #[test]
    fn deck_and_diagnostic_serialization_are_closed_to_private_raw_fields() {
        let matcher = synthetic_matcher();
        let deck = serde_json::to_value(matcher.deck_view(true)).unwrap();
        assert_eq!(
            deck.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["active", "slots"]
        );
        let slot = deck["slots"][0].as_object().unwrap();
        assert_eq!(
            slot.keys().map(String::as_str).collect::<Vec<_>>(),
            ["confidence", "hero_id", "reason", "slot", "team"]
        );
        assert!(!deck.to_string().contains("player_name"));
        assert!(!deck.to_string().contains("rank"));
        assert!(!deck.to_string().contains("pixels"));

        let diagnostics = serde_json::to_value(DraftBarDiagnostics {
            changed_slots: 2,
            identified_slots: 1,
            unknown_slots: 9,
            confidence_bins: [2, 1, 0, 0, 0],
            cycle_ms: 0.75,
        })
        .unwrap();
        assert_eq!(
            diagnostics,
            serde_json::json!({
                "changed_slots": 2, "identified_slots": 1, "unknown_slots": 9,
                "confidence_bins": [2, 1, 0, 0, 0], "cycle_ms": 0.75
            })
        );
    }

    #[test]
    fn private_packet_withholds_pixels_and_ids_on_stale_fence_and_clears_at_lifecycle_edges() {
        let start = Instant::now();
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let strip = synthetic_strip(&geometry);
        let mut memory = DraftBarMemory::default();
        let assert_packet = |memory: &DraftBarMemory, epoch, now, enabled, expect_frame| {
            let packet = memory.packet(epoch, now, enabled).unwrap();
            let json_len = u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize;
            let (metadata, pixels) = packet[4..].split_at(json_len);
            let view: serde_json::Value = serde_json::from_slice(metadata).unwrap();
            assert_eq!(view["has_frame"], expect_frame);
            if expect_frame {
                assert_eq!(view["slots"][0]["hero_id"], "npc_dota_hero_synthetic_one");
                assert!(pixels == strip.as_slice());
            } else {
                assert!(view["slots"][0]["hero_id"].is_null());
                assert!(pixels.is_empty());
            }
        };
        memory.observe(Some(4), false, true);
        assert!(memory.needs_references());
        memory.install_references(synthetic_matcher());
        memory.accept(&strip, &geometry, start);
        assert_packet(&memory, Some(4), start, true, true);
        assert_packet(&memory, Some(5), start, true, false);
        assert_packet(&memory, Some(4), start, false, false);
        assert_packet(
            &memory,
            Some(4),
            start + Duration::from_secs(2),
            true,
            false,
        );
        memory.observe(Some(4), false, false);
        assert!(memory.pixels.is_empty());
        memory.observe(Some(4), false, true);
        assert!(!memory.needs_references());
        assert_packet(&memory, Some(4), start, true, false);
        memory.accept(&strip, &geometry, start);
        memory.observe(Some(5), false, true);
        assert!(memory.needs_references() && memory.pixels.is_empty());
        assert_packet(&memory, Some(5), start, true, false);
        memory.install_references(synthetic_matcher());
        memory.accept(&strip, &geometry, start);
        memory.observe(Some(5), true, false);
        assert!(memory.matcher.is_none() && memory.pixels.is_empty());
        assert_packet(&memory, Some(5), start, true, false);
    }

    #[test]
    fn hero_ids_are_discovered_from_a_bounded_roster_without_names() {
        let roster = br#"
            "npc_dota_hero_axe" { "Model" "npc_dota_hero_axe" }
            "npc_dota_hero_lion" { "Model" "npc_dota_hero_lion" }
            #base "npc_heroes/npc_dota_hero_mirana.txt"
            #base "npc_heroes/npc_dota_hero_bad-id.txt"
            // #base "npc_heroes/npc_dota_hero_ignored.txt"
        "#;
        assert_eq!(
            parse_hero_ids(roster),
            [
                "npc_dota_hero_axe",
                "npc_dota_hero_lion",
                "npc_dota_hero_mirana"
            ]
        );
        assert!(parse_hero_ids(&vec![b'a'; MAX_ROSTER_BYTES + 1]).is_empty());
    }
}
