//! Per-slot draft profiles (DEC-DRAFT-BAR revision 2, owner direction 2026-11-23).
//!
//! Each top-bar slot (Radiant 0–4, Dire 0–4) is an independent profile: the hero (5b), the
//! player colour read from the colour strip (5d), our team's lane label (5e) and the player
//! name (5c, not built). A field that cannot be read stays unknown with a reason; one slot never
//! blocks another. The roster lives in memory for the current match only and is cleared at match
//! end like every Draft Bar output. Player names are never read here.

use crate::draft_bar::{DraftBarGeometry, SLOT_COUNT};
use crate::hero_stats::counters::Side;
use kohbor_game_assets::Vpk;
use serde::Serialize;

/// Colour strip sample box inside each slot, measured on three owner 1920x1080 draft frames
/// (pick phase 2026-11-19, strategy time a050ae8 and 6195c3c): the strip spans y 0–5 across the
/// slanted card top; x 25–84 of the slot box stays inside the slant on every card.
pub(crate) const COLOUR_Y: i32 = 1;
pub(crate) const COLOUR_HEIGHT: u32 = 4;
pub(crate) const COLOUR_X_OFFSET: i32 = 25;
pub(crate) const COLOUR_WIDTH: u32 = 60;
/// Lane label row under our team's names: icon and text fall within y 107–120 on all frames.
pub(crate) const LABEL_Y: i32 = 104;
pub(crate) const LABEL_HEIGHT: u32 = 20;
pub(crate) const LABEL_WIDTH: u32 = 124;

/// Dota 2 player colours as observed in the draft colour strip (median sRGB of three frames).
const PALETTE: [(&str, [u8; 3]); SLOT_COUNT] = [
    ("blue", [51, 117, 252]),
    ("teal", [102, 254, 191]),
    ("purple", [189, 1, 189]),
    ("yellow", [242, 240, 13]),
    ("orange", [253, 107, 1]),
    ("pink", [252, 133, 194]),
    ("olive", [161, 180, 72]),
    ("light_blue", [101, 216, 246]),
    ("dark_green", [1, 129, 33]),
    ("brown", [164, 104, 1]),
];
const MAX_COLOUR_DISTANCE: f32 = 48.0;
const MAX_COLOUR_SPREAD: u8 = 60;

const LANE_ICON_SIZE: usize = 16;
const LANE_ICON_COLUMNS: usize = 13;
const LABEL_MIN_PIXELS: usize = 20;
/// "Hard Support" is about 90 px wide at 1080p; "Support" is about 58 px.
const HARD_SUPPORT_MIN_WIDTH: usize = 78;
const MIN_LANE_SCORE: f32 = 0.45;
const MIN_LANE_MARGIN: f32 = 0.08;

const LANE_ICON_DIR: &str = "panorama/images/rank_tier_icons/handicap/";
const LANE_ICONS: [(LaneIcon, &str); 5] = [
    (LaneIcon::Safe, "safelaneicon_psd.vtex_c"),
    (LaneIcon::Mid, "midlaneicon_psd.vtex_c"),
    (LaneIcon::Off, "offlaneicon_psd.vtex_c"),
    (LaneIcon::SoftSupport, "softsupporticon_psd.vtex_c"),
    (LaneIcon::HardSupport, "hardsupporticon_psd.vtex_c"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LaneRole {
    SafeLane,
    MidLane,
    OffLane,
    Support,
    HardSupport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LaneIcon {
    Safe,
    Mid,
    Off,
    SoftSupport,
    HardSupport,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct PlayerColour {
    pub(crate) name: &'static str,
    pub(crate) rgb: [u8; 3],
}

/// One profile field: the value (or unknown), how sure, when, from which reader, and why not.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RosterField<T> {
    pub(crate) value: Option<T>,
    pub(crate) confidence: f32,
    pub(crate) observed_at_ms: Option<u64>,
    pub(crate) source: &'static str,
    pub(crate) reason: Option<&'static str>,
}

impl<T> RosterField<T> {
    fn unknown(source: &'static str, reason: &'static str) -> Self {
        Self {
            value: None,
            confidence: 0.0,
            observed_at_ms: None,
            source,
            reason: Some(reason),
        }
    }

    fn known(value: T, confidence: f32, at_ms: u64, source: &'static str) -> Self {
        Self {
            value: Some(value),
            confidence: confidence.clamp(0.0, 1.0),
            observed_at_ms: Some(at_ms),
            source,
            reason: None,
        }
    }

    /// A new reading replaces a known value only when it is itself known; an unknown reading
    /// never erases what this match already established (the strip leaves the screen later).
    fn observe(&mut self, next: Self) {
        if next.value.is_some() || self.value.is_none() {
            *self = next;
        }
    }

    pub(crate) fn is_known(&self) -> bool {
        self.value.is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SlotProfile {
    pub(crate) slot: usize,
    pub(crate) side: &'static str,
    pub(crate) index: usize,
    pub(crate) hero: RosterField<String>,
    pub(crate) player_colour: RosterField<PlayerColour>,
    pub(crate) lane_role: RosterField<LaneRole>,
    pub(crate) player_name: RosterField<String>,
}

impl SlotProfile {
    fn new(slot: usize) -> Self {
        Self {
            slot,
            side: if slot < 5 { "radiant" } else { "dire" },
            index: slot % 5,
            hero: RosterField::unknown("draft_bar_5b", "awaiting_frame"),
            player_colour: RosterField::unknown("draft_bar_5d", "awaiting_frame"),
            lane_role: RosterField::unknown("draft_bar_5e", "awaiting_frame"),
            player_name: RosterField::unknown("draft_bar_5c", "not_built"),
        }
    }

    fn slot_side(&self) -> Side {
        if self.slot < 5 {
            Side::Radiant
        } else {
            Side::Dire
        }
    }
}

/// Counts for the info log: no hero names, colours or player data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RosterCounts {
    pub(crate) heroes: usize,
    pub(crate) colours: usize,
    pub(crate) lanes: usize,
    pub(crate) own_slot_known: bool,
}

/// The match roster: ten independent slot profiles plus the derived own slot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct SlotRoster {
    pub(crate) slots: Vec<SlotProfile>,
    pub(crate) own_slot: RosterField<usize>,
    #[serde(skip)]
    own_side: Option<Side>,
    #[serde(skip)]
    own_hero: Option<String>,
}

impl Default for SlotRoster {
    fn default() -> Self {
        Self {
            slots: (0..SLOT_COUNT).map(SlotProfile::new).collect(),
            own_slot: RosterField::unknown("gsi_and_5b", "own_side_unknown"),
            own_side: None,
            own_hero: None,
        }
    }
}

/// One slot's hero reading from 5b, as the deck view reports it.
pub(crate) struct HeroReading<'a> {
    pub(crate) hero_id: Option<&'a str>,
    pub(crate) confidence: f32,
    pub(crate) reason: &'static str,
}

impl SlotRoster {
    /// Forget everything: match end or a new session (same lifetime as Draft Bar data).
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// GSI facts for the own slot: the own team and the own hero key (`npc_dota_hero_*`).
    pub(crate) fn set_own(&mut self, side: Option<Side>, hero: Option<String>, now_ms: u64) {
        if self.own_side != side || self.own_hero != hero {
            self.own_side = side;
            self.own_hero = hero;
            self.derive_own_slot(now_ms);
        }
    }

    /// Update every slot from one accepted strip. Each slot and field is read independently.
    pub(crate) fn observe_strip(
        &mut self,
        strip: &StripPixels<'_>,
        geometry: &DraftBarGeometry,
        heroes: &[HeroReading<'_>],
        lane_icons: &LaneIcons,
        now_ms: u64,
    ) {
        for (slot, profile) in self.slots.iter_mut().enumerate() {
            if let Some(reading) = heroes.get(slot) {
                profile.hero.observe(match reading.hero_id {
                    Some(hero) => RosterField::known(
                        hero.to_owned(),
                        reading.confidence,
                        now_ms,
                        "draft_bar_5b",
                    ),
                    None => RosterField::unknown("draft_bar_5b", reading.reason),
                });
            }
            let rect = geometry.slot_rects[slot];
            let local_x = rect.x - geometry.strip_rect.x;
            profile
                .player_colour
                .observe(read_colour(strip, local_x, now_ms));
            let lane = if self.own_side != Some(profile.slot_side()) {
                RosterField::unknown(
                    "draft_bar_5e",
                    if self.own_side.is_none() {
                        "own_side_unknown"
                    } else {
                        "enemy_team"
                    },
                )
            } else {
                let strip_right = geometry.strip_rect.width as i32;
                let width = (strip_right - local_x).clamp(0, LABEL_WIDTH as i32) as usize;
                read_lane(strip, local_x, width, lane_icons, now_ms)
            };
            profile.lane_role.observe(lane);
        }
        self.derive_own_slot(now_ms);
    }

    /// `own_slot` is the own-team slot whose 5b hero equals the GSI own hero, if exactly one
    /// does. It is never guessed from screen position.
    fn derive_own_slot(&mut self, now_ms: u64) {
        let Some(side) = self.own_side else {
            self.own_slot = RosterField::unknown("gsi_and_5b", "own_side_unknown");
            return;
        };
        let Some(hero) = self.own_hero.as_deref() else {
            self.own_slot = RosterField::unknown("gsi_and_5b", "own_hero_unknown");
            return;
        };
        let matches: Vec<&SlotProfile> = self
            .slots
            .iter()
            .filter(|profile| {
                profile.slot_side() == side && profile.hero.value.as_deref() == Some(hero)
            })
            .collect();
        self.own_slot = match matches.as_slice() {
            [profile] => {
                RosterField::known(profile.slot, profile.hero.confidence, now_ms, "gsi_and_5b")
            }
            [] => RosterField::unknown("gsi_and_5b", "own_hero_not_identified"),
            _ => RosterField::unknown("gsi_and_5b", "ambiguous"),
        };
    }

    pub(crate) fn counts(&self) -> RosterCounts {
        RosterCounts {
            heroes: self.slots.iter().filter(|p| p.hero.is_known()).count(),
            colours: self
                .slots
                .iter()
                .filter(|p| p.player_colour.is_known())
                .count(),
            lanes: self.slots.iter().filter(|p| p.lane_role.is_known()).count(),
            own_slot_known: self.own_slot.is_known(),
        }
    }
}

/// A borrowed BGRA strip.
pub(crate) struct StripPixels<'a> {
    pub(crate) pixels: &'a [u8],
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl StripPixels<'_> {
    fn rgb(&self, x: i32, y: i32) -> Option<[u8; 3]> {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 4;
        let pixel = self.pixels.get(at..at + 4)?;
        Some([pixel[2], pixel[1], pixel[0]])
    }
}

fn read_colour(strip: &StripPixels<'_>, slot_x: i32, now_ms: u64) -> RosterField<PlayerColour> {
    const SOURCE: &str = "draft_bar_5d";
    let mut channels: [Vec<u8>; 3] = Default::default();
    for y in COLOUR_Y..COLOUR_Y + COLOUR_HEIGHT as i32 {
        for x in slot_x + COLOUR_X_OFFSET..slot_x + COLOUR_X_OFFSET + COLOUR_WIDTH as i32 {
            let Some(rgb) = strip.rgb(x, y) else {
                return RosterField::unknown(SOURCE, "outside_strip");
            };
            for (channel, value) in channels.iter_mut().zip(rgb) {
                channel.push(value);
            }
        }
    }
    let mut median = [0_u8; 3];
    let mut spread = 0_u8;
    for (index, channel) in channels.iter_mut().enumerate() {
        channel.sort_unstable();
        let len = channel.len();
        median[index] = channel[len / 2];
        spread = spread.max(channel[len * 9 / 10] - channel[len / 10]);
    }
    if spread > MAX_COLOUR_SPREAD {
        return RosterField::unknown(SOURCE, "strip_not_uniform");
    }
    let distance = |reference: [u8; 3]| -> f32 {
        reference
            .iter()
            .zip(median)
            .map(|(a, b)| (f32::from(*a) - f32::from(b)).powi(2))
            .sum::<f32>()
            .sqrt()
    };
    let mut ranked: Vec<(f32, &(&str, [u8; 3]))> = PALETTE
        .iter()
        .map(|entry| (distance(entry.1), entry))
        .collect();
    ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (best, (name, _)) = ranked[0];
    if best > MAX_COLOUR_DISTANCE {
        return RosterField::unknown(SOURCE, "no_player_colour");
    }
    RosterField::known(
        PlayerColour { name, rgb: median },
        1.0 - best / MAX_COLOUR_DISTANCE,
        now_ms,
        SOURCE,
    )
}

/// Lane icon references built at runtime from the installed game files (read-only), resampled to
/// 16x16 alpha masks. Nothing is shipped.
#[derive(Default)]
pub(crate) struct LaneIcons {
    references: Vec<(LaneIcon, Vec<f32>)>,
}

impl LaneIcons {
    pub(crate) fn from_vpk(vpk: &Vpk) -> Self {
        let mut icons = Self::default();
        for (icon, file) in LANE_ICONS {
            let Some(entry) = vpk.find(&format!("{LANE_ICON_DIR}{file}")) else {
                continue;
            };
            let Some(image) = vpk
                .read(&entry)
                .ok()
                .and_then(|bytes| crate::draft_bar::decode_reference(&bytes))
            else {
                continue;
            };
            let alpha: Vec<f32> = image
                .rgba
                .chunks_exact(4)
                .map(|pixel| f32::from(pixel[3]) / 255.0)
                .collect();
            if let Some(mask) =
                crop_and_resample(&alpha, image.width as usize, image.height as usize, 0.25)
            {
                icons.references.push((icon, mask));
            }
        }
        icons
    }

    pub(crate) fn len(&self) -> usize {
        self.references.len()
    }

    #[cfg(test)]
    pub(crate) fn from_masks(masks: Vec<(usize, Vec<f32>)>) -> Self {
        let order = [
            LaneIcon::Safe,
            LaneIcon::Mid,
            LaneIcon::Off,
            LaneIcon::SoftSupport,
            LaneIcon::HardSupport,
        ];
        Self {
            references: masks
                .into_iter()
                .map(|(index, mask)| (order[index], mask))
                .collect(),
        }
    }
}

fn luma(rgb: [u8; 3]) -> f32 {
    (0.299 * f32::from(rgb[0]) + 0.587 * f32::from(rgb[1]) + 0.114 * f32::from(rgb[2])) / 255.0
}

fn is_label_pixel(rgb: [u8; 3]) -> bool {
    let max = *rgb.iter().max().unwrap_or(&0);
    let min = *rgb.iter().min().unwrap_or(&0);
    luma(rgb) > 0.59 && max - min < 70
}

/// Crop `values` (row-major) to the bounding box of entries above `threshold`, then
/// area-resample to a 16x16 mask.
fn crop_and_resample(
    values: &[f32],
    width: usize,
    height: usize,
    threshold: f32,
) -> Option<Vec<f32>> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if values[y * width + x] > threshold {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x == usize::MAX {
        return None;
    }
    let crop_w = max_x - min_x + 1;
    let crop_h = max_y - min_y + 1;
    let mut out = vec![0.0; LANE_ICON_SIZE * LANE_ICON_SIZE];
    for oy in 0..LANE_ICON_SIZE {
        let y0 = min_y + oy * crop_h / LANE_ICON_SIZE;
        let y1 = (min_y + ((oy + 1) * crop_h).div_ceil(LANE_ICON_SIZE)).max(y0 + 1);
        for ox in 0..LANE_ICON_SIZE {
            let x0 = min_x + ox * crop_w / LANE_ICON_SIZE;
            let x1 = (min_x + ((ox + 1) * crop_w).div_ceil(LANE_ICON_SIZE)).max(x0 + 1);
            let mut sum = 0.0;
            let mut count = 0.0;
            for y in y0..y1.min(height) {
                for x in x0..x1.min(width) {
                    sum += values[y * width + x];
                    count += 1.0;
                }
            }
            out[oy * LANE_ICON_SIZE + ox] = if count > 0.0 { sum / count } else { 0.0 };
        }
    }
    Some(out)
}

fn correlation(left: &[f32], right: &[f32]) -> f32 {
    let n = left.len() as f32;
    let (ml, mr) = (left.iter().sum::<f32>() / n, right.iter().sum::<f32>() / n);
    let (mut cov, mut vl, mut vr) = (0.0, 0.0, 0.0);
    for (a, b) in left.iter().zip(right) {
        cov += (a - ml) * (b - mr);
        vl += (a - ml).powi(2);
        vr += (b - mr).powi(2);
    }
    if vl <= f32::EPSILON || vr <= f32::EPSILON {
        return 0.0;
    }
    cov / (vl * vr).sqrt()
}

/// Our team's lane label under a slot: match the leading icon against the five role icons, then
/// tell Support from Hard Support by the label width. Unknown when unsure.
fn read_lane(
    strip: &StripPixels<'_>,
    slot_x: i32,
    width: usize,
    icons: &LaneIcons,
    now_ms: u64,
) -> RosterField<LaneRole> {
    const SOURCE: &str = "draft_bar_5e";
    if icons.references.is_empty() {
        return RosterField::unknown(SOURCE, "references_unavailable");
    }
    let height = LABEL_HEIGHT as usize;
    let mut bright = vec![false; width * height];
    let mut lumas = vec![0.0; width * height];
    let mut count = 0;
    for y in 0..height {
        for x in 0..width {
            let Some(rgb) = strip.rgb(slot_x + x as i32, LABEL_Y + y as i32) else {
                return RosterField::unknown(SOURCE, "outside_strip");
            };
            lumas[y * width + x] = luma(rgb);
            if is_label_pixel(rgb) {
                bright[y * width + x] = true;
                count += 1;
            }
        }
    }
    if count < LABEL_MIN_PIXELS {
        return RosterField::unknown(SOURCE, "no_label");
    }
    let columns: Vec<usize> = (0..width)
        .filter(|x| (0..height).any(|y| bright[y * width + x]))
        .collect();
    let (left, right) = (columns[0], columns[columns.len() - 1]);
    let label_width = right - left + 1;
    // The icon is the leading glyph; keep only its columns, then crop to its bright box.
    let icon_right = (left + LANE_ICON_COLUMNS).min(width);
    let icon: Vec<f32> = (0..height)
        .flat_map(|y| {
            let lumas = &lumas;
            let bright = &bright;
            (0..width).map(move |x| {
                if x >= left && x < icon_right && bright[y * width + x] {
                    lumas[y * width + x]
                } else {
                    0.0
                }
            })
        })
        .collect();
    let Some(mask) = crop_and_resample(&icon, width, height, 0.0) else {
        return RosterField::unknown(SOURCE, "no_label");
    };
    // Soft and hard support share a hand icon; score them as one class, split by width below.
    let mut classes: [(f32, LaneIcon); 4] = [
        (-1.0, LaneIcon::Safe),
        (-1.0, LaneIcon::Mid),
        (-1.0, LaneIcon::Off),
        (-1.0, LaneIcon::SoftSupport),
    ];
    for (icon, reference) in &icons.references {
        let class = match icon {
            LaneIcon::Safe => 0,
            LaneIcon::Mid => 1,
            LaneIcon::Off => 2,
            LaneIcon::SoftSupport | LaneIcon::HardSupport => 3,
        };
        classes[class].0 = classes[class].0.max(correlation(&mask, reference));
    }
    classes.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (best, class) = classes[0];
    if best < MIN_LANE_SCORE || best - classes[1].0 < MIN_LANE_MARGIN {
        return RosterField::unknown(SOURCE, "low_confidence");
    }
    let role = match class {
        LaneIcon::Safe => LaneRole::SafeLane,
        LaneIcon::Mid => LaneRole::MidLane,
        LaneIcon::Off => LaneRole::OffLane,
        LaneIcon::SoftSupport | LaneIcon::HardSupport => {
            if label_width >= HARD_SUPPORT_MIN_WIDTH {
                LaneRole::HardSupport
            } else {
                LaneRole::Support
            }
        }
    };
    RosterField::known(role, best, now_ms, SOURCE)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::draft_bar::draft_bar_geometry;

    fn blank(geometry: &DraftBarGeometry) -> Vec<u8> {
        let mut pixels =
            vec![
                0_u8;
                geometry.strip_rect.width as usize * geometry.strip_rect.height as usize * 4
            ];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[30, 24, 20, 255]);
        }
        pixels
    }

    fn fill(pixels: &mut [u8], width: u32, x: i32, y: i32, w: u32, h: u32, rgb: [u8; 3]) {
        for yy in y..y + h as i32 {
            for xx in x..x + w as i32 {
                let at = (yy as usize * width as usize + xx as usize) * 4;
                pixels[at..at + 4].copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
            }
        }
    }

    /// Five distinct 16x16 icon masks and the label pixels that draw them.
    fn icon_mask(index: usize) -> Vec<f32> {
        (0..LANE_ICON_SIZE * LANE_ICON_SIZE)
            .map(|i| {
                let (x, y) = (i % LANE_ICON_SIZE, i / LANE_ICON_SIZE);
                let on = match index {
                    0 => x == y || x + 1 == y,
                    1 => x < 3 || y < 3,
                    2 => (x as i32 - 8).abs() + (y as i32 - 8).abs() < 7,
                    _ => {
                        let d = (x as i32 - 8).pow(2) + (y as i32 - 8).pow(2);
                        (30..=64).contains(&d)
                    }
                };
                if on {
                    1.0
                } else {
                    0.0
                }
            })
            .collect()
    }

    fn icons() -> LaneIcons {
        LaneIcons::from_masks((0..5).map(|i| (i, icon_mask(i.min(3)))).collect())
    }

    fn draw_label(pixels: &mut [u8], width: u32, slot_x: i32, icon: usize, text_width: u32) {
        let mask = icon_mask(icon);
        for y in 0..LANE_ICON_SIZE {
            for x in 0..LANE_ICON_SIZE {
                if mask[y * LANE_ICON_SIZE + x] > 0.5 {
                    let px = slot_x + 20 + (x as i32 * 12 / 16);
                    let py = LABEL_Y + 3 + (y as i32 * 12 / 16);
                    fill(pixels, width, px, py, 1, 1, [210, 210, 205]);
                }
            }
        }
        fill(
            pixels,
            width,
            slot_x + 36,
            LABEL_Y + 6,
            text_width,
            7,
            [205, 205, 200],
        );
    }

    fn readings() -> Vec<HeroReading<'static>> {
        (0..SLOT_COUNT)
            .map(|slot| HeroReading {
                hero_id: match slot {
                    0 => Some("npc_dota_hero_axe"),
                    7 => Some("npc_dota_hero_lion"),
                    _ => None,
                },
                confidence: if slot == 0 || slot == 7 { 0.9 } else { 0.4 },
                reason: if slot == 0 || slot == 7 {
                    "identified"
                } else {
                    "low_confidence"
                },
            })
            .collect()
    }

    #[test]
    fn each_slot_reads_colour_hero_and_own_lane_independently() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let width = geometry.strip_rect.width;
        let mut pixels = blank(&geometry);
        for (slot, (rect, (_, rgb))) in geometry.slot_rects.iter().zip(PALETTE).enumerate() {
            if slot == 3 {
                continue; // no strip yet: this slot stays unknown, the others do not care
            }
            fill(
                &mut pixels,
                width,
                rect.x - geometry.strip_rect.x,
                0,
                119,
                6,
                rgb,
            );
        }
        let dire_x = |slot: usize| geometry.slot_rects[slot].x - geometry.strip_rect.x;
        draw_label(&mut pixels, width, dire_x(5), 2, 40); // off lane
        draw_label(&mut pixels, width, dire_x(6), 0, 45); // safe lane
        draw_label(&mut pixels, width, dire_x(7), 3, 40); // support (narrow)
        draw_label(&mut pixels, width, dire_x(8), 1, 45); // mid lane
        draw_label(&mut pixels, width, dire_x(9), 3, 72); // hard support (wide)
        let strip = StripPixels {
            pixels: &pixels,
            width,
            height: geometry.strip_rect.height,
        };
        let mut roster = SlotRoster::default();
        roster.set_own(Some(Side::Dire), Some("npc_dota_hero_lion".to_owned()), 1);
        roster.observe_strip(&strip, &geometry, &readings(), &icons(), 1_000);

        let colours: Vec<Option<&str>> = roster
            .slots
            .iter()
            .map(|p| p.player_colour.value.map(|c| c.name))
            .collect();
        assert_eq!(colours[0], Some("blue"));
        assert_eq!(colours[3], None, "missing strip stays unknown");
        assert_eq!(
            roster.slots[3].player_colour.reason,
            Some("no_player_colour")
        );
        assert_eq!(colours[9], Some("brown"));
        assert_eq!(colours.iter().filter(|c| c.is_some()).count(), 9);

        let lanes: Vec<Option<LaneRole>> = roster.slots.iter().map(|p| p.lane_role.value).collect();
        assert_eq!(
            &lanes[..5],
            &[None; 5],
            "lane labels are read for our team only"
        );
        assert_eq!(roster.slots[0].lane_role.reason, Some("enemy_team"));
        assert_eq!(
            &lanes[5..],
            &[
                Some(LaneRole::OffLane),
                Some(LaneRole::SafeLane),
                Some(LaneRole::Support),
                Some(LaneRole::MidLane),
                Some(LaneRole::HardSupport)
            ]
        );
        assert_eq!(roster.own_slot.value, Some(7));
        assert_eq!(roster.slots[7].player_name.reason, Some("not_built"));
        assert_eq!(
            roster.counts(),
            RosterCounts {
                heroes: 2,
                colours: 9,
                lanes: 5,
                own_slot_known: true
            }
        );
        let json = serde_json::to_string(&roster).unwrap();
        assert!(json.contains("\"own_slot\"") && !json.contains("own_hero"));
    }

    #[test]
    fn known_fields_survive_later_unknown_readings_until_cleared() {
        let geometry = draft_bar_geometry(1920, 1080, 100).unwrap();
        let width = geometry.strip_rect.width;
        let mut pixels = blank(&geometry);
        fill(&mut pixels, width, 0, 0, 119, 6, PALETTE[0].1);
        let mut roster = SlotRoster::default();
        let strip = StripPixels {
            pixels: &pixels,
            width,
            height: geometry.strip_rect.height,
        };
        roster.observe_strip(&strip, &geometry, &readings(), &icons(), 1);
        assert_eq!(
            roster.slots[0].player_colour.value.map(|c| c.name),
            Some("blue")
        );
        let empty = blank(&geometry);
        let strip = StripPixels {
            pixels: &empty,
            width,
            height: geometry.strip_rect.height,
        };
        let unknown: Vec<HeroReading<'static>> = (0..SLOT_COUNT)
            .map(|_| HeroReading {
                hero_id: None,
                confidence: 0.0,
                reason: "empty_slot",
            })
            .collect();
        roster.observe_strip(&strip, &geometry, &unknown, &icons(), 2);
        assert_eq!(
            roster.slots[0].player_colour.value.map(|c| c.name),
            Some("blue")
        );
        assert_eq!(
            roster.slots[0].hero.value.as_deref(),
            Some("npc_dota_hero_axe")
        );
        assert_eq!(roster.slots[0].player_colour.observed_at_ms, Some(1));
        roster.clear();
        assert_eq!(roster.counts(), RosterCounts::default());
    }

    #[test]
    fn own_slot_needs_side_hero_and_a_unique_match() {
        let mut roster = SlotRoster::default();
        assert_eq!(roster.own_slot.reason, Some("own_side_unknown"));
        roster.set_own(Some(Side::Radiant), None, 1);
        assert_eq!(roster.own_slot.reason, Some("own_hero_unknown"));
        roster.set_own(
            Some(Side::Radiant),
            Some("npc_dota_hero_lion".to_owned()),
            1,
        );
        assert_eq!(roster.own_slot.reason, Some("own_hero_not_identified"));
        for slot in [1, 2] {
            roster.slots[slot].hero =
                RosterField::known("npc_dota_hero_lion".to_owned(), 0.9, 1, "draft_bar_5b");
        }
        roster.derive_own_slot(2);
        assert_eq!(roster.own_slot.reason, Some("ambiguous"));
        roster.slots[2].hero = RosterField::unknown("draft_bar_5b", "low_confidence");
        // The same hero on the other team is not ours.
        roster.slots[6].hero =
            RosterField::known("npc_dota_hero_lion".to_owned(), 0.9, 1, "draft_bar_5b");
        roster.derive_own_slot(3);
        assert_eq!(roster.own_slot.value, Some(1));
    }
}
