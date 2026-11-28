use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ZoneId(&'static str);

impl ZoneId {
    pub(crate) fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, Copy)]
struct ReferenceCamp {
    x: i32,
    y: i32,
    zone: &'static str,
}

// Owner decision 2026-11-28; reference positions and camp grouping are from the 7.42 camp table
// supplied with replay-engine-rs 0.5.0 / bundle schema 3.3. Runtime camp ids are resolved by position.
const CAMPS_742: [ReferenceCamp; 28] = [
    ReferenceCamp { x: -8313, y: -553, zone: "R2" },
    ReferenceCamp { x: -8023, y: -1838, zone: "R2" },
    ReferenceCamp { x: -6408, y: 7816, zone: "D5" },
    ReferenceCamp { x: -5024, y: -96, zone: "R1" },
    ReferenceCamp { x: -4824, y: 3915, zone: "R1" },
    ReferenceCamp { x: -4228, y: 8140, zone: "D5" },
    ReferenceCamp { x: -4013, y: 992, zone: "R1" },
    ReferenceCamp { x: -3911, y: 4829, zone: "D5" },
    ReferenceCamp { x: -2596, y: 3850, zone: "D3" },
    ReferenceCamp { x: -2415, y: -8402, zone: "R4" },
    ReferenceCamp { x: -1983, y: -4815, zone: "R3" },
    ReferenceCamp { x: -1454, y: -3356, zone: "R3" },
    ReferenceCamp { x: -852, y: 4940, zone: "D3" },
    ReferenceCamp { x: -720, y: -7696, zone: "R4" },
    ReferenceCamp { x: 186, y: -5197, zone: "R3" },
    ReferenceCamp { x: 336, y: 7696, zone: "D4" },
    ReferenceCamp { x: 1064, y: 2580, zone: "D3" },
    ReferenceCamp { x: 1224, y: 4176, zone: "D3" },
    ReferenceCamp { x: 1922, y: -3975, zone: "R3" },
    ReferenceCamp { x: 2016, y: 7896, zone: "D4" },
    ReferenceCamp { x: 3392, y: -1408, zone: "D1" },
    ReferenceCamp { x: 3978, y: -5027, zone: "R5" },
    ReferenceCamp { x: 4352, y: 48, zone: "D1" },
    ReferenceCamp { x: 4520, y: -8336, zone: "R6" },
    ReferenceCamp { x: 4649, y: -3699, zone: "R5" },
    ReferenceCamp { x: 6712, y: -7632, zone: "R6" },
    ReferenceCamp { x: 7928, y: -120, zone: "D2" },
    ReferenceCamp { x: 8430, y: 1263, zone: "D2" },
];

pub(crate) fn zone_for_camp(camp_id: u64, bundle_camps: &Value) -> Option<ZoneId> {
    let camp = bundle_camps
        .as_array()?
        .iter()
        .find(|camp| camp.get("id").and_then(Value::as_u64) == Some(camp_id))?;
    let x = camp.get("x")?.as_i64()? as f64;
    let y = camp.get("y")?.as_i64()? as f64;
    let mut nearest = CAMPS_742
        .iter()
        .map(|reference| {
            let dx = x - f64::from(reference.x);
            let dy = y - f64::from(reference.y);
            (dx.hypot(dy), reference.zone)
        })
        .collect::<Vec<_>>();
    nearest.sort_by(|left, right| left.0.total_cmp(&right.0));
    let (distance, zone) = *nearest.first()?;
    let next_distance = nearest.get(1)?.0;
    (distance <= 600.0 && next_distance - distance > 600.0).then_some(ZoneId(zone))
}

pub(crate) fn zone_names() -> BTreeMap<String, String> {
    [
        ("R1", "ป่า Radiant 1"),
        ("R2", "ป่า Radiant 2"),
        ("R3", "ป่า Radiant 3"),
        ("R4", "ป่า Radiant 4"),
        ("R5", "ป่า Radiant 5"),
        ("R6", "ป่า Radiant 6"),
        ("D1", "ป่า Dire 1"),
        ("D2", "ป่า Dire 2"),
        ("D3", "ป่า Dire 3"),
        ("D4", "ป่า Dire 4"),
        ("D5", "ป่า Dire 5"),
    ]
    .into_iter()
    .map(|(id, name)| (id.to_owned(), format!("{name} ({id})")))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{zone_for_camp, zone_names};
    use serde_json::json;

    #[test]
    fn v742_reference_camps_map_to_the_owner_zones() {
        let camps = json!([
            {"id": 3, "x": -5024, "y": -96, "type": "small"},
            {"id": 20, "x": 3392, "y": -1408, "type": "medium"},
            {"id": 15, "x": 336, "y": 7696, "type": "large"}
        ]);
        assert_eq!(zone_for_camp(3, &camps).unwrap().as_str(), "R1");
        assert_eq!(zone_for_camp(20, &camps).unwrap().as_str(), "D1");
        assert_eq!(zone_for_camp(15, &camps).unwrap().as_str(), "D4");
        assert_eq!(zone_names()["R1"], "ป่า Radiant 1 (R1)");
    }

    #[test]
    fn camp_outside_the_600_unit_radius_is_unmapped() {
        let camps = json!([{"id": 5, "x": -4423, "y": -96, "type": "small"}]);
        assert!(zone_for_camp(5, &camps).is_none());
    }

    #[test]
    fn close_second_nearest_reference_is_ambiguous() {
        let camps = json!([{"id": 9, "x": -8181, "y": -1138, "type": "small"}]);
        assert!(zone_for_camp(9, &camps).is_none());
    }

    #[test]
    fn runtime_camp_ids_are_resolved_by_position_not_reference_id() {
        let camps = json!([
            {"id": 88, "x": 1224, "y": 4176, "type": "small"},
            {"id": 3, "x": 8000, "y": 5000, "type": "large"}
        ]);
        assert_eq!(zone_for_camp(88, &camps).unwrap().as_str(), "D3");
        assert!(zone_for_camp(3, &camps).is_none());
    }
}
