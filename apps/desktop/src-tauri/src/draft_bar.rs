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

// Remeasured 2026-11-23 against two owner STRATEGY_TIME frames at 1920x1080 (a050ae8, 6195c3c)
// by searching the box that best matches the installed reference portraits. The first
// measurement (one 1919x1072 hero-selection frame with a manual (1, 8) correction) put the Dire
// slots 10 px left and the card art 2 px low, so the Dire boxes cut into the next card.
const STRIP_X: i32 = 209;
const STRIP_Y: i32 = 0;
const STRIP_WIDTH: u32 = 1504;
const STRIP_HEIGHT: u32 = 125;
const SLOT_WIDTH: u32 = 119;
const SLOT_HEIGHT: u32 = 68;
const SLOT_Y: i32 = 6;
const SLOT_STEP: i32 = 124;
const RADIANT_X0: i32 = 209;
const DIRE_X0: i32 = 1098;
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
    /// Per-slot profiles for this match while the draft bar is active; `null` otherwise.
    roster: Option<&'a crate::draft_roster::SlotRoster>,
}

fn reason_code(reason: MatchReason) -> &'static str {
    match reason {
        MatchReason::Identified => "identified",
        MatchReason::EmptySlot => "empty_slot",
        MatchReason::LowContrast => "low_contrast",
        MatchReason::LowConfidence => "low_confidence",
        MatchReason::ReferencesUnavailable => "references_unavailable",
        MatchReason::AwaitingFrame => "awaiting_frame",
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            elapsed.as_millis().min(u128::from(u64::MAX)) as u64
        })
}

#[derive(Default)]
pub(crate) struct DraftBarMemory {
    epoch: Option<u64>,
    active: bool,
    detail: Option<String>,
    matcher: Option<DraftBarMatcher>,
    pixels: Vec<u8>,
    frame_at: Option<Instant>,
    /// Per-slot profiles for the current match; kept across short withdrawals, cleared with the
    /// session (DEC-DRAFT-BAR revision 2).
    roster: crate::draft_roster::SlotRoster,
}

impl DraftBarMemory {
    pub fn observe(&mut self, epoch: Option<u64>, match_ended: bool, active: bool) {
        let epoch = if match_ended { None } else { epoch };
        if should_clear_session(self.epoch, epoch, match_ended) {
            self.withdraw(None);
            self.matcher = None;
            self.roster.clear();
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

    /// Log-safe per-slot best match score in hundredths (0–100) and reason code, slot order
    /// Radiant 0–4 then Dire 5–9. No hero names. `None` before references are installed.
    pub(crate) fn slot_scores(&self) -> Option<Vec<(u8, MatchReason)>> {
        let matcher = self.matcher.as_ref()?;
        Some(
            matcher
                .deck_view(true)
                .slots
                .iter()
                .map(|slot| {
                    (
                        (slot.confidence.clamp(0.0, 1.0) * 100.0).round() as u8,
                        slot.reason,
                    )
                })
                .collect(),
        )
    }

    pub fn needs_references(&self) -> bool {
        self.active && self.matcher.is_none()
    }

    pub fn install_references(&mut self, matcher: DraftBarMatcher) {
        self.matcher = Some(matcher);
    }

    /// GSI own team and own hero key for `own_slot`; `None` when not admitted.
    pub(crate) fn set_own(
        &mut self,
        side: Option<crate::hero_stats::counters::Side>,
        hero: Option<String>,
    ) {
        self.roster.set_own(side, hero, unix_ms());
    }

    pub(crate) fn roster(&self) -> &crate::draft_roster::SlotRoster {
        &self.roster
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
            let view = matcher.deck_view(true);
            let readings: Vec<crate::draft_roster::HeroReading<'_>> = view
                .slots
                .iter()
                .map(|slot| crate::draft_roster::HeroReading {
                    hero_id: slot.hero_id.as_deref(),
                    confidence: slot.confidence,
                    reason: reason_code(slot.reason),
                })
                .collect();
            self.roster.observe_strip(
                &crate::draft_roster::StripPixels {
                    pixels,
                    width,
                    height: geometry.strip_rect.height,
                },
                geometry,
                &readings,
                &matcher.lane_icons,
                unix_ms(),
            );
        }
        self.pixels.resize(pixels.len(), 0);
        self.pixels.copy_from_slice(pixels);
        self.frame_at = Some(at);
        self.detail = None;
    }

    /// Binary control-window IPC only. Four little-endian bytes give the JSON
    /// metadata length, followed by metadata and optionally tight 1504x125 BGRA.
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
            roster: active.then_some(&self.roster),
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

/// Reference build outcome for the info log: counts and a failure code only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReferenceReport {
    pub roster: usize,
    pub built: usize,
    pub ycocg: usize,
    pub lane_icons: usize,
    pub missing: usize,
    pub unreadable: usize,
    pub decode_failed: usize,
    pub failure: Option<&'static str>,
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
    /// Lane role icons for the own-team labels (5e), built from the same installed files.
    lane_icons: crate::draft_roster::LaneIcons,
}

impl DraftBarMatcher {
    pub fn empty() -> Self {
        Self {
            references: Vec::new(),
            slots: std::array::from_fn(|_| None),
            lane_icons: crate::draft_roster::LaneIcons::default(),
        }
    }

    /// Read the hero roster and root hero images from the user's VPK into memory once.
    /// The root `heroes/` images are the landscape reference family for the top bar. The report
    /// carries log-safe counts only (no paths, no hero names).
    pub fn from_vpk_with_report(vpk: &Vpk) -> (Self, ReferenceReport) {
        let mut matcher = Self::empty();
        let mut report = ReferenceReport::default();
        let Some(roster_entry) = vpk.find(ROSTER_PATH) else {
            report.failure = Some("roster_missing");
            return (matcher, report);
        };
        if roster_entry.encoded_len() > MAX_ROSTER_BYTES as u64 {
            report.failure = Some("roster_too_large");
            return (matcher, report);
        }
        let Ok(roster) = vpk.read(&roster_entry) else {
            report.failure = Some("roster_unreadable");
            return (matcher, report);
        };
        if roster.len() > MAX_ROSTER_BYTES {
            report.failure = Some("roster_too_large");
            return (matcher, report);
        }
        let heroes = parse_hero_ids(&roster);
        report.roster = heroes.len();
        for hero_id in heroes {
            let Some(entry) = hero_portrait_path(&hero_id)
                .into_iter()
                .next()
                .and_then(|path| vpk.find(&path))
            else {
                report.missing += 1;
                continue;
            };
            let Ok(bytes) = vpk.read(&entry) else {
                report.unreadable += 1;
                continue;
            };
            if stored_as_ycocg(&bytes) {
                report.ycocg += 1;
            }
            let Some(image) = decode_reference(&bytes) else {
                report.decode_failed += 1;
                continue;
            };
            let Some(features) = rgba_features(&image) else {
                report.decode_failed += 1;
                continue;
            };
            matcher.references.push(ReferenceTemplate {
                hero_id: hero_id.into_bytes(),
                features,
            });
        }
        report.built = matcher.references.len();
        matcher.lane_icons = crate::draft_roster::LaneIcons::from_vpk(vpk);
        report.lane_icons = matcher.lane_icons.len();
        if report.built == 0 {
            report.failure = Some("no_reference_built");
        }
        matcher
            .references
            .sort_by(|left, right| left.hero_id.cmp(&right.hero_id));
        (matcher, report)
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

const YCOCG_MARKER: &[u8] = b"YCoCg Conv";
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

/// Whether a compiled texture was stored as scaled YCoCg in DXT5 (Source 2 "Image YCoCg
/// Conversion"). The landscape hero portraits are; their embedded-PNG siblings are not converted.
fn stored_as_ycocg(resource: &[u8]) -> bool {
    let Some(header) = resource.get(..4) else {
        return false;
    };
    let metadata_len = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let Some(metadata) = resource.get(..metadata_len) else {
        return false;
    };
    let payload = &resource[metadata_len..];
    !payload.starts_with(PNG_SIGNATURE)
        && metadata
            .windows(YCOCG_MARKER.len())
            .any(|window| window == YCOCG_MARKER)
}

/// Scaled YCoCg (Y in alpha, Co and Cg in red and green, scale in blue) to RGB.
fn ycocg_to_rgb(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let scale = f32::from(pixel[2] >> 3) + 1.0;
        let co = (f32::from(pixel[0]) - 128.0) / scale;
        let cg = (f32::from(pixel[1]) - 128.0) / scale;
        let y = f32::from(pixel[3]);
        pixel[0] = (y + co - cg).round().clamp(0.0, 255.0) as u8;
        pixel[1] = (y + cg).round().clamp(0.0, 255.0) as u8;
        pixel[2] = (y - co - cg).round().clamp(0.0, 255.0) as u8;
        pixel[3] = 255;
    }
}

/// Decode a reference portrait to straight RGBA. `decode_vtex_c` returns YCoCg-encoded DXT5
/// as raw channels, which made every reference unrecognisable (live test on 8d9f8c3: 0/10).
pub(crate) fn decode_reference(resource: &[u8]) -> Option<Image> {
    let mut image = decode_vtex_c(resource).ok()?;
    if stored_as_ycocg(resource) {
        ycocg_to_rgb(&mut image.rgba);
    }
    Some(image)
}

/// Percent of the card height compared, from the top. The live top bar overlays the rank banner,
/// medal and info icon on the lower part of every card; the reference art has none of them.
const CARD_TOP_PERCENT: usize = 58;

fn card_rows(height: usize) -> usize {
    (height * CARD_TOP_PERCENT / 100).max(1)
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
        let source_y = resample_coordinate(y, FEATURE_HEIGHT, card_rows(image.height as usize))?;
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
                + resample_coordinate(target_y, FEATURE_HEIGHT, card_rows(rect.height as usize))?;
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
            y: SLOT_Y,
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
            lane_icons: crate::draft_roster::LaneIcons::default(),
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
        // Like the live top bar: portrait art in the card top, a flat rank banner below it.
        let card = card_rows(height);
        for target_y in 0..height {
            for target_x in 0..width {
                let value = if target_y < card {
                    let feature_y = resample_coordinate(target_y, card, FEATURE_HEIGHT).unwrap();
                    let feature_x = resample_coordinate(target_x, width, FEATURE_WIDTH).unwrap();
                    (f32::from(features[feature_y * FEATURE_WIDTH + feature_x]) * dim)
                        .round()
                        .clamp(0.0, 255.0) as u8
                } else {
                    96
                };
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
    fn owner_frame_geometry_matches_the_remeasured_strategy_time_boxes() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        assert_eq!(
            geometry.strip_rect,
            Rect {
                x: 209,
                y: 0,
                width: 1504,
                height: 125
            }
        );
        // The strip carries the colour bar (y 0-5, 5d) and our lane labels (y 104-123, 5e).
        assert!(geometry.strip_rect.y <= crate::draft_roster::COLOUR_Y);
        assert!(
            crate::draft_roster::LABEL_Y + crate::draft_roster::LABEL_HEIGHT as i32
                <= geometry.strip_rect.y + geometry.strip_rect.height as i32
        );
        assert_eq!(
            geometry.slot_rects[0],
            Rect {
                x: 209,
                y: 6,
                width: 119,
                height: 68
            }
        );
        assert_eq!(geometry.slot_rects[4].x, 705);
        // Regression, live test on 8d9f8c3: Dire started at 1088, 10 px left of the cards.
        assert_eq!(geometry.slot_rects[5].x, 1098);
        assert_eq!(geometry.slot_rects[9].x, 1594);
        let strip_right = geometry.strip_rect.x + geometry.strip_rect.width as i32;
        assert!(geometry
            .slot_rects
            .iter()
            .all(|slot| slot.x + slot.width as i32 <= strip_right));
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

    /// DEC-DRAFT-BAR revision 2: the roster rides in the control-window packet while active, stays
    /// within the deck's 16 KiB metadata cap, carries no player name, and is cleared with the
    /// session.
    #[test]
    fn packet_carries_the_match_roster_and_clears_it_with_the_session() {
        let start = Instant::now();
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let strip = synthetic_strip(&geometry);
        let mut memory = DraftBarMemory::default();
        memory.observe(Some(4), false, true);
        memory.install_references(synthetic_matcher());
        memory.set_own(
            Some(crate::hero_stats::counters::Side::Radiant),
            Some("npc_dota_hero_synthetic_one".to_owned()),
        );
        memory.accept(&strip, &geometry, start);
        assert_eq!(memory.roster().own_slot.value, Some(0));
        let packet = memory.packet(Some(4), start, true).unwrap();
        let json_len = u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize;
        assert!(json_len <= 16 * 1024, "metadata {json_len} bytes");
        let view: serde_json::Value = serde_json::from_slice(&packet[4..4 + json_len]).unwrap();
        let roster = &view["roster"];
        assert_eq!(roster["slots"].as_array().unwrap().len(), SLOT_COUNT);
        assert_eq!(roster["own_slot"]["value"], 0);
        assert_eq!(
            roster["slots"][0]["hero"]["value"],
            "npc_dota_hero_synthetic_one"
        );
        assert_eq!(
            roster["slots"][0]["player_name"]["value"],
            serde_json::Value::Null
        );
        assert_eq!(roster["slots"][0]["player_name"]["reason"], "not_built");
        // A short withdrawal keeps the roster; a new session clears it.
        memory.withdraw(Some("no_game_window".to_owned()));
        assert_eq!(memory.roster().counts().heroes, 1);
        let inactive = memory.packet(Some(5), start, true).unwrap();
        let inactive_len = u32::from_le_bytes(inactive[..4].try_into().unwrap()) as usize;
        let inactive_view: serde_json::Value =
            serde_json::from_slice(&inactive[4..4 + inactive_len]).unwrap();
        assert!(
            inactive_view["roster"].is_null(),
            "no roster outside the admitted session"
        );
        memory.observe(Some(5), false, true);
        assert_eq!(
            memory.roster().counts(),
            crate::draft_roster::RosterCounts::default()
        );
        memory.observe(None, true, false);
        assert_eq!(memory.roster().counts().heroes, 0);
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

    #[test]
    fn scaled_ycocg_converts_back_to_rgb() {
        // RGB (200, 100, 50): Y 112.5, Co 75, Cg -12.5, stored at scale 1 (blue < 8).
        let mut pixel = [203_u8, 116, 0, 113];
        ycocg_to_rgb(&mut pixel);
        for (got, want) in pixel[..3].iter().zip([200_u8, 100, 50]) {
            assert!(got.abs_diff(want) <= 2, "{pixel:?}");
        }
        assert_eq!(pixel[3], 255);
        // Scale 2 (blue 8..15) stores chroma doubled: RGB (140, 120, 100) is Y 120, Co 20, Cg 0.
        let mut scaled = [128 + 40, 128, 8, 120];
        ycocg_to_rgb(&mut scaled);
        assert_eq!(&scaled[..3], &[140, 120, 100]);
    }

    /// Regression, live test on 8d9f8c3 (0/10 identified): the landscape portraits are DXT5
    /// with "Image YCoCg Conversion" and were matched as raw YCoCg channels.
    #[test]
    fn ycocg_marker_is_read_from_metadata_and_skips_embedded_png() {
        fn resource(marker: bool, png: bool) -> Vec<u8> {
            let mut bytes = vec![0_u8; 64];
            bytes[..4].copy_from_slice(&64_u32.to_le_bytes());
            if marker {
                bytes[20..30].copy_from_slice(b"YCoCg Conv");
            }
            if png {
                bytes.extend_from_slice(PNG_SIGNATURE);
            }
            bytes.extend_from_slice(&[0x0f, 0x18, 0x82, 0x94]);
            bytes
        }
        assert!(stored_as_ycocg(&resource(true, false)));
        assert!(
            !stored_as_ycocg(&resource(true, true)),
            "embedded PNG is already RGB"
        );
        assert!(!stored_as_ycocg(&resource(false, false)));
        let mut payload_only = resource(false, false);
        payload_only.extend_from_slice(b"YCoCg Conv");
        assert!(
            !stored_as_ycocg(&payload_only),
            "marker in pixel data does not count"
        );
        assert!(!stored_as_ycocg(&[1, 2]));
    }

    /// The rank banner, medal and info icon cover the lower part of each live card and are not
    /// in the reference art, so they must not change the features.
    #[test]
    fn features_ignore_the_rank_banner_below_the_card_top() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let strip = synthetic_strip(&geometry);
        let mut bannered = strip.clone();
        let slot = geometry.slot_rects[0];
        let stride = geometry.strip_rect.width as usize * 4;
        let local_y = (slot.y - geometry.strip_rect.y) as usize;
        let banner_from = local_y + card_rows(slot.height as usize);
        for y in banner_from..local_y + slot.height as usize {
            for x in 0..slot.width as usize {
                let at = y * stride + (slot.x - geometry.strip_rect.x) as usize * 4 + x * 4;
                bannered[at..at + 3].copy_from_slice(&[180, 140, 120]);
            }
        }
        let local = Rect {
            x: slot.x - geometry.strip_rect.x,
            y: slot.y - geometry.strip_rect.y,
            ..slot
        };
        let frame = |pixels: &[u8]| {
            BgraFrame {
                pixels,
                width: geometry.strip_rect.width,
                height: geometry.strip_rect.height,
                stride: geometry.strip_rect.width * 4,
            }
            .features(local)
            .unwrap()
        };
        assert_eq!(frame(&strip), frame(&bannered));
        assert_eq!(card_rows(68), 39);
    }

    /// Local-only (not run by default): score the owner's STRATEGY_TIME frames with the shipped
    /// geometry against references built from the installed game. `KOHBOR_5B_TOPS` lists raw
    /// 1920x130 BGRA top bands separated by `;`. They show player names and stay outside the repo.
    #[test]
    #[ignore]
    fn owner_frames_score_per_slot() {
        let Ok(paths) = std::env::var("KOHBOR_5B_TOPS") else {
            return;
        };
        let game = crate::intake::locate_game().expect("game located");
        let vpk = kohbor_game_assets::Vpk::open(game.join("game/dota/pak01_dir.vpk")).expect("vpk");
        let (mut matcher, report) = DraftBarMatcher::from_vpk_with_report(&vpk);
        eprintln!("PROBE report={report:?}");
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        for (index, path) in paths.split(';').enumerate() {
            let band = std::fs::read(path).unwrap();
            let strip_rect = geometry.strip_rect;
            let mut strip =
                Vec::with_capacity(strip_rect.width as usize * strip_rect.height as usize * 4);
            for y in strip_rect.y as usize..(strip_rect.y as u32 + strip_rect.height) as usize {
                let start = (y * 1920 + strip_rect.x as usize) * 4;
                strip.extend_from_slice(&band[start..start + strip_rect.width as usize * 4]);
            }
            let frame = BgraFrame {
                pixels: &strip,
                width: strip_rect.width,
                height: strip_rect.height,
                stride: strip_rect.width * 4,
            };
            for (slot, rect) in geometry.slot_rects.iter().enumerate() {
                let local = Rect {
                    x: rect.x - strip_rect.x,
                    y: rect.y - strip_rect.y,
                    ..*rect
                };
                let features = frame.features(local).unwrap();
                let mut scored: Vec<(f32, String)> = matcher
                    .references
                    .iter()
                    .filter_map(|r| {
                        tile_similarity(&r.features, &features).map(|s| {
                            (
                                s,
                                String::from_utf8_lossy(&r.hero_id)
                                    .trim_start_matches("npc_dota_hero_")
                                    .to_owned(),
                            )
                        })
                    })
                    .collect();
                scored.sort_by(|a, b| b.0.total_cmp(&a.0));
                eprintln!(
                    "PROBE top frame={index} slot={slot} contrast={:.1} best={:.3}:{} second={:.3}:{} margin={:.3}",
                    grayscale_contrast(&features),
                    scored[0].0,
                    scored[0].1,
                    scored[1].0,
                    scored[1].1,
                    scored[0].0 - scored[1].0
                );
            }
            matcher.identify_strip(
                &strip,
                strip_rect.width,
                strip_rect.height,
                strip_rect.width * 4,
                &geometry,
            );
            let view = matcher.deck_view(true);
            for slot in &view.slots {
                eprintln!(
                    "PROBE frame={index} slot={} score={:.3} reason={:?} hero={:?}",
                    slot.slot, slot.confidence, slot.reason, slot.hero_id
                );
            }
            if let Ok(owns) = std::env::var("KOHBOR_5B_OWN") {
                let own = owns
                    .split(',')
                    .nth(index)
                    .map(|hero| format!("npc_dota_hero_{hero}"));
                let mut roster = crate::draft_roster::SlotRoster::default();
                roster.set_own(Some(crate::hero_stats::counters::Side::Dire), own, 1);
                let view = matcher.deck_view(true);
                let readings: Vec<crate::draft_roster::HeroReading<'_>> = view
                    .slots
                    .iter()
                    .map(|slot| crate::draft_roster::HeroReading {
                        hero_id: slot.hero_id.as_deref(),
                        confidence: slot.confidence,
                        reason: reason_code(slot.reason),
                    })
                    .collect();
                roster.observe_strip(
                    &crate::draft_roster::StripPixels {
                        pixels: &strip,
                        width: strip_rect.width,
                        height: strip_rect.height,
                    },
                    &geometry,
                    &readings,
                    &matcher.lane_icons,
                    1,
                );
                for profile in &roster.slots {
                    eprintln!(
                        "PROBE roster frame={index} slot={} colour={:?} ({:.2}, {:?}) lane={:?} ({:.2}, {:?})",
                        profile.slot,
                        profile.player_colour.value.map(|c| c.name),
                        profile.player_colour.confidence,
                        profile.player_colour.reason,
                        profile.lane_role.value,
                        profile.lane_role.confidence,
                        profile.lane_role.reason
                    );
                }
                eprintln!(
                    "PROBE roster frame={index} own_slot={:?} reason={:?} counts={:?}",
                    roster.own_slot.value,
                    roster.own_slot.reason,
                    roster.counts()
                );
            }
            matcher.clear_slots();
        }
    }

    /// Local-only: dump decoded VPK images listed in `KOHBOR_VPK_DUMP_PATHS` (`;`-separated)
    /// into `KOHBOR_5B_DUMP` as raw RGBA named with their size.
    #[test]
    #[ignore]
    fn dump_vpk_images() {
        let (Ok(paths), Ok(dir)) = (
            std::env::var("KOHBOR_VPK_DUMP_PATHS"),
            std::env::var("KOHBOR_5B_DUMP"),
        ) else {
            return;
        };
        let game = crate::intake::locate_game().expect("game located");
        let vpk = kohbor_game_assets::Vpk::open(game.join("game/dota/pak01_dir.vpk")).expect("vpk");
        for path in paths.split(';') {
            let raw = vpk.read(&vpk.find(path).expect(path)).unwrap();
            let image = decode_reference(&raw).unwrap();
            let name = path.rsplit('/').next().unwrap().replace(".vtex_c", "");
            std::fs::write(
                format!("{dir}/{name}-{}x{}.rgba", image.width, image.height),
                &image.rgba,
            )
            .unwrap();
            eprintln!(
                "PROBE dumped {name} {}x{} ycocg={}",
                image.width,
                image.height,
                stored_as_ycocg(&raw)
            );
        }
    }
}
