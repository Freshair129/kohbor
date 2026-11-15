use kohbor_lab::catalog::table;
use serde_json::Value;
use std::collections::BTreeSet;

fn contract() -> Value {
    let path = format!(
        "{}/../../contracts/event-catalog.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn row<'a>(table: &'a Value, id: &str) -> &'a Value {
    table["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no row for {id}"))
}

#[test]
fn one_row_per_catalog_event_in_catalog_order() {
    let table = table();
    let expected: Vec<String> = contract()["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect();
    let actual: Vec<String> = table["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 18);
    assert_eq!(
        actual.iter().collect::<BTreeSet<_>>().len(),
        18,
        "no id twice"
    );
}

#[test]
fn every_row_is_fully_classified_with_thai_text() {
    let categories = [
        "ordinary",
        "critical",
        "notice",
        "correction",
        "system",
        "recording",
        "test",
    ];
    let statuses = ["detected_today", "catalog_only", "test_only"];
    for r in table()["events"].as_array().unwrap() {
        let id = r["id"].as_str().unwrap();
        assert!(
            categories.contains(&r["category"].as_str().unwrap()),
            "{id} category"
        );
        assert!(
            statuses.contains(&r["status"].as_str().unwrap()),
            "{id} status"
        );
        for key in [
            "label_th",
            "label_en",
            "source_label_th",
            "status_th",
            "detector_ref",
        ] {
            assert!(
                !r[key].as_str().unwrap_or("").is_empty(),
                "{id}.{key} is empty"
            );
        }
        assert!(r["g_maiden"].is_array(), "{id} g_maiden");
    }
}

#[test]
fn implementation_status_matches_what_the_code_emits() {
    let table = table();
    let detected: Vec<&str> = table["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["status"] == "detected_today")
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        detected.len(),
        16,
        "12 GSI detector events + 4 native risk events: {detected:?}"
    );
    assert_eq!(row(&table, "clip_requested")["status"], "catalog_only");
    assert_eq!(row(&table, "latency_probe")["status"], "test_only");
    assert_eq!(row(&table, "health_danger")["status"], "detected_today");
}

#[test]
fn detection_source_distinguishes_gsi_cv_and_native_risk() {
    let table = table();
    assert_eq!(row(&table, "hero_kill")["source"], "gsi");
    assert_eq!(row(&table, "enemy_missing")["source"], "gsi_cv");
    assert_eq!(row(&table, "risk_alert")["source"], "native_risk");
    assert_eq!(row(&table, "clip_requested")["source"], "manual");
    assert_eq!(row(&table, "latency_probe")["source"], "test_only");
}

#[test]
fn tunable_counts_come_from_the_catalog_and_the_profile() {
    let table = table();
    assert_eq!(row(&table, "health_danger")["tunables"], 5);
    assert_eq!(row(&table, "health_danger")["tunables_editable"], 5);
    assert_eq!(row(&table, "enemy_missing")["tunables"], 2);
    assert_eq!(row(&table, "enemy_missing")["tunables_editable"], 0);
    assert_eq!(row(&table, "hero_kill")["tunables"], 0);
    let editable: u64 = table["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["tunables_editable"].as_u64().unwrap())
        .sum();
    assert_eq!(
        editable, 13,
        "all 13 profile parameters are reachable from a row"
    );
}

#[test]
fn g_maiden_equivalence_covers_every_event_id_of_its_taxonomy() {
    let table = table();
    let mut seen = BTreeSet::new();
    for r in table["events"].as_array().unwrap() {
        for name in r["g_maiden"].as_array().unwrap() {
            assert!(
                seen.insert(name["id"].as_str().unwrap().to_owned()),
                "G-Maiden id twice: {name}"
            );
        }
    }
    for only in table["g_maiden_only"].as_array().unwrap() {
        assert!(seen.insert(only["id"].as_str().unwrap().to_owned()));
    }
    let taxonomy = [
        "danger",
        "gank",
        "revision",
        "match_start",
        "first_blood",
        "kill",
        "double_kill",
        "triple_kill",
        "ultra_kill",
        "rampage",
        "killing_spree",
        "dominating",
        "mega_kill",
        "unstoppable",
        "wicked_sick",
        "monster_kill",
        "godlike",
        "beyond_godlike",
        "death",
        "respawn",
        "levelUp",
        "hpLow",
        "manaLow",
        "advice",
    ];
    let expected: BTreeSet<String> = taxonomy.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(
        seen, expected,
        "24 G-Maiden ids (the 25th, 'unused', is a placeholder)"
    );
}

#[test]
fn g_maiden_tier_names_line_up_with_our_tiers() {
    let table = table();
    let tiers = |id: &str| -> Vec<u64> {
        row(&table, id)["g_maiden"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["tier"].as_u64().unwrap())
            .collect()
    };
    assert_eq!(tiers("multi_kill"), vec![2, 3, 4, 5]);
    assert_eq!(tiers("streak"), vec![3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(row(&table, "hero_kill")["g_maiden"][0]["id"], "kill");
    assert_eq!(
        row(&table, "health_danger")["g_maiden"][0]["relation"],
        "approx"
    );
    assert_eq!(row(&table, "hero_death")["g_maiden"][0]["relation"], "same");
    assert!(
        row(&table, "match_ended")["g_maiden"]
            .as_array()
            .unwrap()
            .is_empty(),
        "ours-only"
    );
    assert_eq!(table["g_maiden_only"][0]["id"], "advice");
}

#[test]
fn game_possible_events_are_never_presented_as_built_or_verified() {
    let table = table();
    let catalog_ids: BTreeSet<String> = contract()["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_owned())
        .collect();
    let rows = table["not_in_catalog"].as_array().unwrap();
    assert!(rows.len() >= 8);
    for r in rows {
        let id = r["id"].as_str().unwrap();
        assert!(!catalog_ids.contains(id), "{id} is already a catalog event");
        let status = r["status"].as_str().unwrap();
        assert!(
            ["possible_unverified", "proposed_not_built"].contains(&status),
            "{id}: {status}"
        );
        assert_eq!(r["verified"], false);
        assert_eq!(r["built"], false);
        assert!(!r["note_th"].as_str().unwrap().is_empty());
    }
    let rune = rows.iter().find(|r| r["id"] == "rune_spawn").unwrap();
    assert_eq!(rune["status"], "proposed_not_built");
}
