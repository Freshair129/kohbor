//! The Lab's event catalog table (docs/design/KOHBOR-LAB.md section 12): every catalog event
//! with its Thai label, category, detection source, implementation status, G-Maiden equivalent
//! and tunables, plus the game events that are possible but NOT in the catalog.
//!
//! Facts come from `contracts/event-catalog.json` (embedded) and from what the code emits;
//! the G-Maiden column is a static id map (ids and semantics only, no code). Clip binding and
//! last-fired time are host facts and are merged in by the host.

use crate::profile::{catalog, editable_tunables};
use serde_json::{json, Value};

/// Events the GSI detector (`kohbor_events::EventId`) and the native risk engine
/// (`kohbor_risk::RiskEvent`) emit today. A test pins the count and the catalog membership.
const DETECTED_TODAY: [&str; 16] = [
    "multi_kill",
    "streak",
    "first_blood",
    "hero_kill",
    "hero_death",
    "level_milestone",
    "health_low",
    "mana_low",
    "match_ended",
    "match_started",
    "hero_respawn",
    "health_danger",
    "enemy_missing",
    "risk_alert",
    "risk_revision",
    "risk_unavailable",
];

/// G-Maiden ids for one of our events: (id, tier, relation). Relation `same` = obviously the
/// same moment; `approx` = same idea, semantics not verified; `tier` = one tier of our event.
type Equivalent = (&'static str, Option<u8>, &'static str);

fn g_maiden_for(id: &str) -> Vec<Equivalent> {
    match id {
        "hero_kill" => vec![("kill", None, "same")],
        "hero_death" => vec![("death", None, "same")],
        "hero_respawn" => vec![("respawn", None, "same")],
        "first_blood" => vec![("first_blood", None, "same")],
        "match_started" => vec![("match_start", None, "same")],
        "level_milestone" => vec![("levelUp", None, "approx")],
        "health_low" => vec![("hpLow", None, "approx")],
        "mana_low" => vec![("manaLow", None, "approx")],
        "health_danger" => vec![("danger", None, "approx")],
        "risk_alert" => vec![("gank", None, "approx")],
        "risk_revision" => vec![("revision", None, "approx")],
        "multi_kill" => vec![
            ("double_kill", Some(2), "tier"),
            ("triple_kill", Some(3), "tier"),
            ("ultra_kill", Some(4), "tier"),
            ("rampage", Some(5), "tier"),
        ],
        "streak" => vec![
            ("killing_spree", Some(3), "tier"),
            ("dominating", Some(4), "tier"),
            ("mega_kill", Some(5), "tier"),
            ("unstoppable", Some(6), "tier"),
            ("wicked_sick", Some(7), "tier"),
            ("monster_kill", Some(8), "tier"),
            ("godlike", Some(9), "tier"),
            ("beyond_godlike", Some(10), "tier"),
        ],
        // Ours only: match_ended, risk_unavailable, enemy_missing, clip_requested, latency_probe.
        _ => Vec::new(),
    }
}

fn source_of(id: &str, catalog_source: &str, status: &str) -> (&'static str, &'static str) {
    if status == "test_only" {
        return ("test_only", "ทดสอบเท่านั้น");
    }
    match (id, catalog_source) {
        ("enemy_missing", _) => ("gsi_cv", "GSI + กล้อง CV (ต้องมี capture)"),
        (_, "native") => ("native_risk", "native risk (GSI + CV)"),
        (_, "gsi") => ("gsi", "GSI (ข้อมูลเกม)"),
        (_, "manual") => ("manual", "กดเอง (manual)"),
        (_, "vision") => ("gsi_cv", "GSI + กล้อง CV (ต้องมี capture)"),
        _ => ("native_risk", "native"),
    }
}

fn status_of(id: &str, catalog_status: &str) -> (&'static str, &'static str) {
    if catalog_status == "test_only" {
        ("test_only", "เฉพาะทดสอบ")
    } else if DETECTED_TODAY.contains(&id) {
        ("detected_today", "ตรวจจับได้แล้ว")
    } else {
        ("catalog_only", "อยู่ในแคตตาล็อก ยังไม่ได้สร้างตัวตรวจจับ")
    }
}

/// Game events that may exist but are NOT in the catalog. Nothing here is verified against a
/// real GSI payload and nothing is built; `rune_spawn` is a design-only proposal.
const NOT_IN_CATALOG: [(&str, &str, &str, &str); 11] = [
    (
        "roshan_state",
        "สถานะ Roshan",
        "possible_unverified",
        "อาจอ่านได้จากข้อมูลเกม ยังไม่ได้ตรวจกับ payload จริงของเจ้าของ (อาจเป็นเฉพาะผู้ชม)",
    ),
    (
        "rune_spawn",
        "รูนเกิด",
        "proposed_not_built",
        "ออกแบบไว้เท่านั้น คำนวณจาก clock_time ของแผนที่ ยังไม่ได้สร้าง",
    ),
    (
        "day_night_change",
        "กลางวัน/กลางคืนเปลี่ยน",
        "possible_unverified",
        "อาจอ่านจากข้อมูลแผนที่ ยังไม่ได้ตรวจกับ payload จริง",
    ),
    (
        "buyback_ready",
        "Buyback พร้อม/คูลดาวน์",
        "possible_unverified",
        "อาจอ่านจากข้อมูลผู้เล่น/ฮีโร่ ยังไม่ได้ตรวจกับ payload จริง",
    ),
    (
        "ultimate_ready",
        "อัลติเมทพร้อม",
        "possible_unverified",
        "อาจคำนวณจากคูลดาวน์ในบล็อก abilities (ตอนนี้อ่านอยู่) ยังไม่ได้ตรวจเป็น event",
    ),
    (
        "item_ready",
        "ไอเทมพร้อมใช้",
        "possible_unverified",
        "อาจคำนวณจากคูลดาวน์ในบล็อก items (ตอนนี้อ่านอยู่) ยังไม่ได้ตรวจเป็น event",
    ),
    (
        "aegis",
        "Aegis",
        "possible_unverified",
        "ยังไม่ทราบว่า payload ให้ข้อมูลหรือไม่",
    ),
    (
        "gold_milestone",
        "ทองถึงเป้า",
        "possible_unverified",
        "อาจคำนวณจากทองของผู้เล่น ยังไม่ได้ตรวจเป็น event",
    ),
    (
        "xp_milestone",
        "XP ถึงเป้า",
        "possible_unverified",
        "อาจคำนวณจากค่าประสบการณ์ ยังไม่ได้ตรวจเป็น event",
    ),
    (
        "tower_destroyed",
        "ป้อมถูกทำลาย",
        "possible_unverified",
        "น่าจะมีเฉพาะโหมดผู้ชม ยังไม่ได้ตรวจ",
    ),
    (
        "barracks_destroyed",
        "ค่ายทหารถูกทำลาย",
        "possible_unverified",
        "น่าจะมีเฉพาะโหมดผู้ชม ยังไม่ได้ตรวจ",
    ),
];

fn equivalent_json(entries: Vec<Equivalent>) -> Vec<Value> {
    entries
        .into_iter()
        .map(|(id, tier, relation)| json!({"id": id, "tier": tier, "relation": relation}))
        .collect()
}

/// The whole table. Pure; the host adds `binding` and `last_fired` to each row.
pub fn table() -> Value {
    let editable = editable_tunables();
    let rows: Vec<Value> = catalog()["events"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|event| {
            let id = event["id"].as_str().unwrap_or_default();
            let (status, status_th) = status_of(id, event["status"].as_str().unwrap_or_default());
            let (source, source_th) =
                source_of(id, event["source"].as_str().unwrap_or_default(), status);
            let tunables = event["tunables"].as_object().map_or(0, |m| m.len());
            json!({
                "id": id,
                "label_th": event["labels"]["th"],
                "label_en": event["labels"]["en"],
                "category": event["category"],
                "source": source,
                "source_label_th": source_th,
                "status": status,
                "status_th": status_th,
                "detector_ref": event["detector_ref"],
                "milestone": event["detector_milestone"],
                "voiced_default": event["voiced_default"],
                "media_mappable": event["media_mappable"],
                "critical_media_required": event["critical_media_required"],
                "tier_range": event["tier_range"],
                "tunables": tunables,
                "tunables_editable": editable.iter().filter(|(e, _)| *e == id).count(),
                "g_maiden": equivalent_json(g_maiden_for(id)),
            })
        })
        .collect();

    let not_in_catalog: Vec<Value> = NOT_IN_CATALOG
        .iter()
        .map(|(id, label_th, status, note_th)| {
            json!({
                "id": id, "label_th": label_th, "status": status, "note_th": note_th,
                "verified": false, "built": false,
            })
        })
        .collect();

    json!({
        "events": rows,
        "not_in_catalog": not_in_catalog,
        "g_maiden_only": [{
            "id": "advice",
            "label_th": "คำแนะนำ (advice)",
            "note_th": "มีเฉพาะ G-Maiden ฝั่งเราเป็นความสามารถแยก (dynamic advice) ไม่ใช่ event ของแคตตาล็อก",
        }],
        "taxonomy_version": catalog()["taxonomy_version"],
        "note_th": "ข้อมูลตารางนี้มาจาก contracts/event-catalog.json และสิ่งที่โค้ดปล่อย event ได้จริง ส่วน G-Maiden เป็นแผนที่ id เท่านั้น ส่วน 'เกมอาจมีแต่ไม่อยู่ในแคตตาล็อก' ยังไม่ได้ตรวจกับ payload จริง",
    })
}
