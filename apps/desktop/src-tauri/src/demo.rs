pub fn demo_enabled(_env: Option<&str>) -> bool { unimplemented!() }
pub fn demo_event(_id: &str, _tier: Option<u8>) -> Result<kohbor_events::Event, &'static str> { unimplemented!() }
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn only_exact_one_enables_demo() {
        assert!(demo_enabled(Some("1")));
        for value in [None, Some(""), Some("0"), Some("true"), Some("1 ")] { assert!(!demo_enabled(value)); }
    }
    #[test] fn accepts_catalog_events_and_valid_tiers() {
        for id in ["first_blood","hero_kill","hero_death","level_milestone","health_low","mana_low","match_ended","match_started","hero_respawn","health_danger"] { assert!(demo_event(id,None).is_ok(), "{id}"); }
        for (id, tier) in [("multi_kill",2),("multi_kill",5),("streak",3),("streak",10)] { assert!(demo_event(id,Some(tier)).is_ok(), "{id} {tier}"); }
    }
    #[test] fn all_active_catalog_ids_are_representable() {
        let catalog: serde_json::Value = serde_json::from_str(include_str!("../../../../contracts/event-catalog.json")).unwrap();
        for event in catalog["events"].as_array().unwrap() {
            if event["status"] != "active" { continue; }
            let id = event["id"].as_str().unwrap();
            let tier = event.get("tier_range").and_then(|r| r["min"].as_u64()).map(|v| v as u8);
            let detectable = ["multi_kill","streak","first_blood","hero_kill","hero_death","level_milestone","health_low","mana_low","match_ended","match_started","hero_respawn","health_danger"].contains(&id);
            // Only ids the detector can emit (kohbor_events::EventId) are representable; the rest must be refused.
            assert_eq!(demo_event(id, tier).is_ok(), detectable, "catalog event {id}");
        }
    }
    #[test] fn rejects_unknown_and_invalid_tiers() {
        for (id,tier) in [("unknown",None),("hero_kill",Some(2)),("multi_kill",Some(1)),("multi_kill",Some(6)),("streak",Some(2)),("streak",Some(11))] { assert!(demo_event(id,tier).is_err(), "{id} {tier:?}"); }
    }
}

