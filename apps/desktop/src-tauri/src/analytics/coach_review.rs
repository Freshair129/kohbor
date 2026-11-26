use super::lane_expectation;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) struct ReviewHero {
    pub(crate) hero: String,
    pub(crate) role: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReviewCandidate {
    pub(crate) bundle_hash: String,
    pub(crate) lane: String,
    pub(crate) patch: Option<String>,
    pub(crate) shape: String,
    pub(crate) radiant: Vec<ReviewHero>,
    pub(crate) dire: Vec<ReviewHero>,
    pub(crate) raw: Value,
    pub(crate) expectation: Value,
    pub(crate) table_tier: f64,
    pub(crate) model_verdict: String,
    pub(crate) constants_version: u32,
    pub(crate) constants_hash: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Blind,
    HalfBlind,
    Revealed,
}

#[derive(Clone, Debug)]
struct ReviewItem {
    candidate: ReviewCandidate,
    stage: Stage,
    coach_tier: Option<i32>,
    coach_verdict: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct CoachReviewSession {
    items: Vec<ReviewItem>,
    current: usize,
    constants_version: u32,
    constants_hash: String,
}

impl CoachReviewSession {
    pub(crate) fn start(
        candidates: Vec<ReviewCandidate>,
        required: usize,
    ) -> Result<Self, &'static str> {
        if required == 0 || candidates.len() < required {
            return Err("coach_review_insufficient_candidates");
        }
        let selected = candidates.into_iter().take(required).collect::<Vec<_>>();
        let first = selected
            .first()
            .ok_or("coach_review_insufficient_candidates")?;
        if selected.iter().any(|candidate| {
            candidate.constants_version != first.constants_version
                || candidate.constants_hash != first.constants_hash
        }) {
            return Err("coach_review_constants_mismatch");
        }
        let constants_version = first.constants_version;
        let constants_hash = first.constants_hash.clone();
        let items = selected
            .into_iter()
            .map(|candidate| ReviewItem {
                candidate,
                stage: Stage::Blind,
                coach_tier: None,
                coach_verdict: None,
            })
            .collect();
        Ok(Self {
            items,
            current: 0,
            constants_version,
            constants_hash,
        })
    }

    pub(crate) fn current(&self) -> Value {
        let Some(item) = self.items.get(self.current) else {
            return json!({"complete":true,"completed":self.items.len(),"total":self.items.len()});
        };
        let candidate = &item.candidate;
        let heroes = json!({
            "radiant":candidate.radiant.iter().map(hero_json).collect::<Vec<_>>(),
            "dire":candidate.dire.iter().map(hero_json).collect::<Vec<_>>()
        });
        match item.stage {
            Stage::Blind => json!({
                "complete":false,"stage":"blind","index":self.current,"total":self.items.len(),
                "lane":candidate.lane,"patch":candidate.patch,"shape":candidate.shape,"heroes":heroes,
                "tier_min":lane_expectation::config_signed_integer("/expectation/tier_min"),
                "tier_max":lane_expectation::config_signed_integer("/expectation/tier_max"),
                "prompt":"ให้คะแนนความได้เปรียบจากความรู้ของโค้ช ตั้งแต่ -2 ถึง +2 โดยมุม Radiant"
            }),
            Stage::HalfBlind => json!({
                "complete":false,"stage":"half_blind","index":self.current,"total":self.items.len(),
                "lane":candidate.lane,"patch":candidate.patch,"shape":candidate.shape,"heroes":heroes,
                "raw":candidate.raw,
                "prompt":"จากตัวเลขดิบ เลือก เกินคาด / ตามคาด / ต่ำกว่าคาด โดยยังไม่แสดงค่าคาดหมาย"
            }),
            Stage::Revealed => json!({
                "complete":false,"stage":"revealed","index":self.current,"total":self.items.len(),
                "lane":candidate.lane,"patch":candidate.patch,"shape":candidate.shape,"heroes":heroes,
                "raw":candidate.raw,"expectation":candidate.expectation,"table_tier":candidate.table_tier,
                "coach_tier":item.coach_tier,"coach_verdict":item.coach_verdict,
                "model_verdict":candidate.model_verdict,
                "prompt":"ตรวจผลแล้ว กดถัดไปเพื่อเปิดเลนต่อไป"
            }),
        }
    }

    pub(crate) fn submit_blind(&mut self, tier: i32) -> Result<Value, &'static str> {
        let minimum = lane_expectation::config_signed_integer("/expectation/tier_min");
        let maximum = lane_expectation::config_signed_integer("/expectation/tier_max");
        if !(minimum..=maximum).contains(&i64::from(tier)) {
            return Err("coach_tier_invalid");
        }
        let item = self
            .items
            .get_mut(self.current)
            .ok_or("coach_review_complete")?;
        if item.stage != Stage::Blind {
            return Err("coach_review_order_invalid");
        }
        item.coach_tier = Some(tier);
        item.stage = Stage::HalfBlind;
        Ok(self.current())
    }

    pub(crate) fn submit_half_blind(&mut self, verdict: &str) -> Result<Value, &'static str> {
        if !matches!(verdict, "above" | "as" | "below") {
            return Err("coach_verdict_invalid");
        }
        let item = self
            .items
            .get_mut(self.current)
            .ok_or("coach_review_complete")?;
        if item.stage != Stage::HalfBlind {
            return Err("coach_review_order_invalid");
        }
        item.coach_verdict = Some(verdict.to_owned());
        item.stage = Stage::Revealed;
        Ok(self.current())
    }

    pub(crate) fn next(&mut self) -> Result<Value, &'static str> {
        let item = self
            .items
            .get(self.current)
            .ok_or("coach_review_complete")?;
        if item.stage != Stage::Revealed {
            return Err("coach_review_order_invalid");
        }
        self.current += 1;
        Ok(self.current())
    }

    pub(crate) fn export(&self) -> Result<Value, &'static str> {
        if self.current < self.items.len() {
            return Err("coach_review_incomplete");
        }
        let bundle_hashes = self
            .items
            .iter()
            .map(|item| item.candidate.bundle_hash.clone())
            .collect::<BTreeSet<_>>();
        let rows = self.items.iter().map(|item| {
            let candidate = &item.candidate;
            json!({
                "bundle_hash":candidate.bundle_hash,
                "lane":candidate.lane,
                "patch":candidate.patch,
                "shape":candidate.shape,
                "heroes":{"radiant":candidate.radiant.iter().map(hero_json).collect::<Vec<_>>(),"dire":candidate.dire.iter().map(hero_json).collect::<Vec<_>>()},
                "raw":candidate.raw,
                "expectation":candidate.expectation,
                "table_tier":candidate.table_tier,
                "coach_tier":item.coach_tier,
                "coach_verdict":item.coach_verdict,
                "model_verdict":candidate.model_verdict,
                "opposite_direction":opposite(item.coach_verdict.as_deref(),Some(&candidate.model_verdict)),
                "absolute_tier_difference":item.coach_tier.map(|tier|(f64::from(tier)-candidate.table_tier).abs())
            })
        }).collect::<Vec<_>>();
        let same = self
            .items
            .iter()
            .filter(|item| {
                item.coach_verdict.as_deref() == Some(item.candidate.model_verdict.as_str())
            })
            .count();
        let opposite_pairs = self
            .items
            .iter()
            .filter(|item| {
                opposite(
                    item.coach_verdict.as_deref(),
                    Some(&item.candidate.model_verdict),
                )
            })
            .count();
        let mean_abs_tier_difference = self
            .items
            .iter()
            .filter_map(|item| {
                item.coach_tier
                    .map(|tier| (f64::from(tier) - item.candidate.table_tier).abs())
            })
            .sum::<f64>()
            / self.items.len().max(1) as f64;
        let mut confusion = BTreeMap::<String, usize>::new();
        for item in &self.items {
            if let Some(coach) = item.coach_verdict.as_deref() {
                *confusion
                    .entry(format!("{}:{}", coach, item.candidate.model_verdict))
                    .or_default() += 1;
            }
        }
        Ok(json!({
            "schema":1,
            "constants_version":self.constants_version,
            "constants_hash":self.constants_hash,
            "bundle_hashes":bundle_hashes,
            "reviewed_lane_count":self.items.len(),
            "same_verdict_count":same,
            "same_verdict_pct":lane_expectation::config_number("/score/percent_scale")*same as f64/self.items.len().max(1) as f64,
            "opposite_direction_pairs":opposite_pairs,
            "mean_absolute_tier_difference":mean_abs_tier_difference,
            "verdict_matrix":confusion,
            "rows":rows,
            "contains_personal_data":false
        }))
    }
}

fn hero_json(hero: &ReviewHero) -> Value {
    json!({"hero":hero.hero,"role":hero.role})
}

fn opposite(coach: Option<&str>, model: Option<&String>) -> bool {
    matches!(
        (coach, model.map(String::as_str)),
        (Some("above"), Some("below")) | (Some("below"), Some("above"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(index: usize) -> ReviewCandidate {
        ReviewCandidate {
            bundle_hash: format!("{:064x}", index),
            lane: "mid".into(),
            patch: Some("7.42".into()),
            shape: "1v1".into(),
            radiant: vec![ReviewHero {
                hero: "Ember Spirit".into(),
                role: Some("mid".into()),
            }],
            dire: vec![ReviewHero {
                hero: "Earthshaker".into(),
                role: Some("mid".into()),
            }],
            raw: json!({"score":-7.3,"components":{"nw":-8.6,"xp":-12.6}}),
            expectation: json!({"value":1.0,"source_label":"ตารางโค้ช · 7.42 · 2026-11-25"}),
            table_tier: 0.2,
            model_verdict: "below".into(),
            constants_version: 0,
            constants_hash: "constants-hash".into(),
        }
    }

    fn session() -> CoachReviewSession {
        CoachReviewSession::start((0..15).map(candidate).collect(), 15).unwrap()
    }

    #[test]
    fn blind_order_hides_raw_expectation_and_verdict_until_submitted() {
        let mut session = session();
        let blind = session.current();
        assert_eq!(blind["stage"], "blind");
        assert!(blind.get("raw").is_none());
        assert!(blind.get("expectation").is_none());
        assert!(blind.get("model_verdict").is_none());
        assert_eq!(
            session.submit_half_blind("above").err(),
            Some("coach_review_order_invalid")
        );
        let half = session.submit_blind(1).unwrap();
        assert_eq!(half["stage"], "half_blind");
        assert!(half.get("raw").is_some());
        assert!(half.get("expectation").is_none());
        assert!(half.get("model_verdict").is_none());
        assert_eq!(session.next().err(), Some("coach_review_order_invalid"));
        let reveal = session.submit_half_blind("as").unwrap();
        assert_eq!(reveal["stage"], "revealed");
        assert!(reveal.get("expectation").is_some());
        assert_eq!(reveal["model_verdict"], "below");
    }

    #[test]
    fn export_requires_all_rows_and_contains_hashes_without_personal_data() {
        let mut session = session();
        assert_eq!(session.export().err(), Some("coach_review_incomplete"));
        for _ in 0..15 {
            session.submit_blind(0).unwrap();
            session.submit_half_blind("below").unwrap();
            session.next().unwrap();
        }
        let export = session.export().unwrap();
        assert_eq!(export["reviewed_lane_count"], 15);
        assert_eq!(export["opposite_direction_pairs"], 0);
        assert_eq!(export["constants_version"], 0);
        assert_eq!(export["bundle_hashes"].as_array().unwrap().len(), 15);
        let text = serde_json::to_string(&export).unwrap();
        for forbidden in [
            "match_id",
            "player_name",
            "steamid",
            "account_id",
            "bundle_path",
        ] {
            assert!(!text.contains(forbidden));
        }
    }

    #[test]
    fn candidate_count_and_blind_tier_are_bounded() {
        assert_eq!(
            CoachReviewSession::start(vec![candidate(0)], 15).err(),
            Some("coach_review_insufficient_candidates")
        );
        let mut session = session();
        assert_eq!(session.submit_blind(3).err(), Some("coach_tier_invalid"));
    }
}
