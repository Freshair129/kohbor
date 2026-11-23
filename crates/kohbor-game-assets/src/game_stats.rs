//! Read-only extraction and normalization of hero and item stat inputs from
//! the installed game's KeyValues scripts.

use crate::keyvalues::{self, KvObject, KvValue};
use crate::{Entry, Vpk};
use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

pub type ExtractResult<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug)]
pub struct SourceBlob {
    pub path: String,
    pub archive: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct HeroStats {
    pub id: String,
    pub primary_attribute: Option<String>,
    pub attack_type: Option<String>,
    pub values: BTreeMap<String, Option<f64>>,
    /// Ability names in their explicit in-game slot order. Missing ability
    /// definitions remain represented with unknown classification fields.
    pub abilities: Vec<HeroAbility>,
    pub sources: Vec<String>,
}

#[derive(Debug, Default)]
pub struct HeroAbility {
    pub slot: u8,
    pub name: String,
    pub is_ultimate: Option<bool>,
    pub behavior: Vec<String>,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Default)]
pub struct ItemStats {
    pub id: String,
    pub attributes: BTreeMap<String, Option<f64>>,
    pub effects: BTreeMap<String, Option<f64>>,
    pub unknown: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
pub struct StatsData {
    pub heroes: BTreeMap<String, HeroStats>,
    pub items: BTreeMap<String, ItemStats>,
    pub source_blobs: Vec<SourceBlob>,
}

#[derive(Debug)]
struct Script {
    object: KvObject,
    bytes: Vec<u8>,
    archive: PathBuf,
}

const HERO_FIELDS: &[(&str, &str)] = &[
    ("baseStrength", "AttributeBaseStrength"),
    ("strengthGain", "AttributeStrengthGain"),
    ("baseAgility", "AttributeBaseAgility"),
    ("agilityGain", "AttributeAgilityGain"),
    ("baseIntelligence", "AttributeBaseIntelligence"),
    ("intelligenceGain", "AttributeIntelligenceGain"),
    ("baseArmor", "ArmorPhysical"),
    ("baseAttackSpeed", "BaseAttackSpeed"),
    ("baseAttackTime", "AttackRate"),
    ("baseDamageMin", "AttackDamageMin"),
    ("baseDamageMax", "AttackDamageMax"),
    ("baseMoveSpeed", "MovementSpeed"),
    ("baseHealth", "StatusHealth"),
    ("baseHealthRegen", "StatusHealthRegen"),
    ("baseMana", "StatusMana"),
    ("baseManaRegen", "StatusManaRegen"),
    ("baseMagicResistance", "MagicalResistance"),
    ("attackRange", "AttackRange"),
];

const ITEM_ATTRIBUTE_FIELDS: &[&str] = &["strength", "agility", "intelligence", "all"];
const ITEM_EFFECT_FIELDS: &[&str] = &[
    "armor",
    "attackSpeed",
    "damage",
    "moveSpeedFlat",
    "moveSpeedPercent",
    "moveSpeedBoots",
    "moveSpeedBootsMelee",
    "moveSpeedBootsRanged",
    "magicResistance",
    "evasion",
    "statusResistance",
    "spellAmp",
    "lifesteal",
    "spellLifesteal",
    "healthRegen",
    "manaRegen",
];

/// Read the installed script entries. The returned bytes are exactly the
/// VPK payloads used to derive the JSON and can be hashed for provenance.
pub fn extract(vpk: &Vpk) -> ExtractResult<StatsData> {
    let mut data = StatsData::default();
    let hero_index = read_script(vpk, "scripts/npc/npc_heroes.txt")?;
    let base_path = "scripts/npc/heroes/npc_dota_hero_base.txt";
    let base = read_script(vpk, base_path)?;
    let items = read_script(vpk, "scripts/npc/items.txt")?;
    let ability_script = read_script(vpk, "scripts/npc/npc_abilities.txt")?;
    data.source_blobs.extend([
        source_blob("scripts/npc/npc_heroes.txt", &hero_index),
        source_blob(base_path, &base),
        source_blob("scripts/npc/items.txt", &items),
        source_blob("scripts/npc/npc_abilities.txt", &ability_script),
    ]);
    let ability_root = ability_script
        .object
        .object("DOTAAbilities")
        .unwrap_or(&ability_script.object);

    let mut hero_files = vpk.list_dir("scripts/npc/heroes", "txt")?;
    hero_files.retain(|entry| {
        entry.path().rsplit('/').next().is_some_and(|name| {
            name.starts_with("npc_dota_hero_")
                && name.ends_with(".txt")
                && name != "npc_dota_hero_base.txt"
        })
    });

    for entry in hero_files {
        let path = entry.path().to_owned();
        let Some(file_name) = path.rsplit('/').next() else {
            continue;
        };
        let Some(id) = file_name
            .strip_prefix("npc_dota_hero_")
            .and_then(|name| name.strip_suffix(".txt"))
        else {
            continue;
        };
        if id.is_empty() || !valid_game_id(id) {
            continue;
        }
        let id = id.to_owned();
        let hero_file = read_entry(vpk, &entry)?;
        let base_unit = find_unit(&base.object, "npc_dota_hero_base");
        let hero_unit = find_unit(&hero_file.object, file_name.trim_end_matches(".txt"));
        let hero_index_unit = find_unit(&hero_index.object, file_name.trim_end_matches(".txt"));
        let Some(hero_unit) = hero_unit else {
            continue;
        };
        data.source_blobs.push(source_blob(&path, &hero_file));
        let mut hero = HeroStats {
            id: id.clone(),
            ..HeroStats::default()
        };
        for &(field, source_key) in HERO_FIELDS {
            let (value, source_path) = scalar_from(hero_unit, source_key)
                .or_else(|| base_unit.and_then(|unit| scalar_from(unit, source_key)))
                .unwrap_or((None, None));
            hero.values
                .insert(field.to_owned(), value.and_then(parse_number));
            if let Some(source_path) = source_path {
                hero.sources.push(source_path.to_owned());
            }
        }
        hero.primary_attribute = primary_attribute_from(hero_unit, hero_index_unit, base_unit);
        hero.attack_type = scalar(hero_unit, "AttackCapabilities")
            .or_else(|| base_unit.and_then(|unit| scalar(unit, "AttackCapabilities")))
            .and_then(normalize_attack_type);
        hero.abilities = ability_slots(
            hero_index_unit,
            Some(hero_unit),
            ability_root,
            &hero_file.object,
        );
        hero.sources.push(path);
        hero.sources.push(base_path.to_owned());
        hero.sources.push("scripts/npc/npc_heroes.txt".to_owned());
        hero.sources.sort();
        hero.sources.dedup();
        data.heroes.insert(id, hero);
    }
    if data.heroes.is_empty() {
        return Err("npc_heroes.txt and hero scripts produced no hero records".into());
    }

    let item_root =
        find_object(&items.object, &["DOTAAbilities", "DOTAItems"]).unwrap_or(&items.object);
    for (key, value) in item_root.entries() {
        if !key.starts_with("item_") {
            continue;
        }
        let KvValue::Object(item_object) = value else {
            continue;
        };
        let mut item = ItemStats {
            id: key.clone(),
            ..ItemStats::default()
        };
        for field in ITEM_ATTRIBUTE_FIELDS {
            item.attributes.insert((*field).to_owned(), Some(0.0));
        }
        for field in ITEM_EFFECT_FIELDS {
            item.effects.insert((*field).to_owned(), Some(0.0));
        }

        let mut raw_values = BTreeMap::<String, String>::new();
        if let Some(values) = item_object.object("AbilityValues") {
            flatten_values(values, &mut raw_values);
        }
        if let Some(attrib) = item_object.object("attrib") {
            let mut attrib_values = BTreeMap::new();
            flatten_attrib(attrib, &mut attrib_values);
            for (key, value) in attrib_values {
                raw_values.entry(key).or_insert(value);
            }
        }
        normalize_item_values(&mut item, raw_values);
        if is_boots_item(&item.id) {
            let boots_speed = item.effects.get("moveSpeedFlat").copied().flatten();
            item.effects
                .insert("moveSpeedBoots".to_owned(), boots_speed);
            item.effects.insert("moveSpeedFlat".to_owned(), Some(0.0));
        }
        data.items.insert(item.id.clone(), item);
    }

    data.source_blobs.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(data)
}

fn ability_slots(
    primary: Option<&KvObject>,
    fallback: Option<&KvObject>,
    ability_root: &KvObject,
    hero_script_root: &KvObject,
) -> Vec<HeroAbility> {
    let mut slots = BTreeMap::<u8, String>::new();
    for hero in [primary, fallback].into_iter().flatten() {
        for (key, value) in hero.entries() {
            let Some(slot) = key
                .strip_prefix("Ability")
                .and_then(|slot| slot.parse::<u8>().ok())
            else {
                continue;
            };
            if slot == 0 || slot > 16 || slots.contains_key(&slot) {
                continue;
            }
            let KvValue::Scalar(name) = value else {
                continue;
            };
            let name = name.trim();
            if !name.is_empty() && name != "generic_hidden" && valid_game_id(name) {
                slots.insert(slot, name.to_owned());
            }
        }
    }
    slots
        .into_iter()
        .map(|(slot, name)| {
            let definition = find_named_object(ability_root, &name)
                .or_else(|| find_named_object(hero_script_root, &name));
            let is_ultimate = definition
                .and_then(|ability| scalar(ability, "AbilityType"))
                .map(|kind| kind.trim() == "ABILITY_TYPE_ULTIMATE");
            let behavior = definition
                .and_then(|ability| scalar(ability, "AbilityBehavior"))
                .map(split_flags)
                .unwrap_or_default();
            let modifiers = definition
                .and_then(|ability| ability.object("Modifiers"))
                .map(|modifiers| {
                    modifiers
                        .entries()
                        .iter()
                        .filter_map(|(name, value)| {
                            matches!(value, KvValue::Object(_)).then_some(name.clone())
                        })
                        .collect()
                })
                .unwrap_or_default();
            HeroAbility {
                slot,
                name,
                is_ultimate,
                behavior,
                modifiers,
            }
        })
        .collect()
}

fn split_flags(value: &str) -> Vec<String> {
    value
        .split('|')
        .map(str::trim)
        .filter(|flag| !flag.is_empty())
        .map(str::to_owned)
        .collect()
}

fn find_named_object<'a>(root: &'a KvObject, name: &str) -> Option<&'a KvObject> {
    if let Some(object) = root.object(name) {
        return Some(object);
    }
    root.entries().iter().find_map(|(_, value)| match value {
        KvValue::Object(object) => find_named_object(object, name),
        KvValue::Scalar(_) => None,
    })
}

fn source_blob(path: &str, script: &Script) -> SourceBlob {
    SourceBlob {
        path: path.to_owned(),
        archive: script.archive.clone(),
        bytes: script.bytes.clone(),
    }
}

fn read_script(vpk: &Vpk, path: &str) -> ExtractResult<Script> {
    let entry = vpk
        .find(path)
        .ok_or_else(|| format!("missing VPK entry: {path}"))?;
    read_entry(vpk, &entry)
}

fn read_entry(vpk: &Vpk, entry: &Entry) -> ExtractResult<Script> {
    let bytes = vpk.read(entry)?;
    let text = std::str::from_utf8(&bytes)?;
    Ok(Script {
        object: keyvalues::parse(text)?,
        bytes,
        archive: vpk.source_path(entry)?,
    })
}

fn find_object<'a>(root: &'a KvObject, names: &[&str]) -> Option<&'a KvObject> {
    for name in names {
        if let Some(object) = root.object(name) {
            return Some(object);
        }
    }
    root.entries().iter().find_map(|(_, value)| match value {
        KvValue::Object(object) => find_object(object, names),
        KvValue::Scalar(_) => None,
    })
}

fn find_unit<'a>(root: &'a KvObject, name: &str) -> Option<&'a KvObject> {
    if let Some(object) = root.object(name) {
        return Some(object);
    }
    if root.scalar("AttributeBaseStrength").is_some() || root.scalar("BaseClass").is_some() {
        return Some(root);
    }
    root.entries().iter().find_map(|(_, value)| match value {
        KvValue::Object(object) => find_unit(object, name),
        KvValue::Scalar(_) => None,
    })
}

fn scalar<'a>(root: &'a KvObject, key: &str) -> Option<&'a str> {
    root.scalar(key)
}

fn primary_attribute_from(
    hero: &KvObject,
    index: Option<&KvObject>,
    base: Option<&KvObject>,
) -> Option<String> {
    scalar(hero, "AttributePrimary")
        .or_else(|| index.and_then(|unit| scalar(unit, "AttributePrimary")))
        .or_else(|| base.and_then(|unit| scalar(unit, "AttributePrimary")))
        .and_then(normalize_primary_attribute)
}

fn scalar_from<'a>(root: &'a KvObject, key: &str) -> Option<(Option<&'a str>, Option<&'a str>)> {
    scalar(root, key).map(|value| (Some(value), None))
}

fn parse_number(raw: &str) -> Option<f64> {
    let cleaned = raw.trim().trim_end_matches('%').trim();
    let value = cleaned.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn normalize_primary_attribute(raw: &str) -> Option<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "dota_attribute_strength" | "strength" => Some("strength".to_owned()),
        "dota_attribute_agility" | "agility" => Some("agility".to_owned()),
        "dota_attribute_intellect" | "intellect" | "intelligence" => {
            Some("intelligence".to_owned())
        }
        "dota_attribute_all" | "dota_attribute_universal" | "all" | "universal" => {
            Some("universal".to_owned())
        }
        _ => None,
    }
}

fn normalize_attack_type(raw: &str) -> Option<String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "dota_unit_cap_melee_attack" | "melee" => Some("melee".to_owned()),
        "dota_unit_cap_ranged_attack" | "ranged" => Some("ranged".to_owned()),
        _ => None,
    }
}

fn valid_game_id(id: &str) -> bool {
    id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn is_boots_item(id: &str) -> bool {
    matches!(
        id,
        "item_arcane_boots"
            | "item_boots"
            | "item_boots_of_bearing"
            | "item_guardian_greaves"
            | "item_phase_boots"
            | "item_power_treads"
            | "item_tranquil_boots"
            | "item_travel_boots"
            | "item_travel_boots_2"
    )
}

fn flatten_values(object: &KvObject, output: &mut BTreeMap<String, String>) {
    for (key, value) in object.entries() {
        match value {
            KvValue::Scalar(value) => {
                output.insert(key.to_ascii_lowercase(), value.clone());
            }
            KvValue::Object(nested) => flatten_values(nested, output),
        }
    }
}

fn flatten_attrib(object: &KvObject, output: &mut BTreeMap<String, String>) {
    if let (Some(key), Some(value)) = (object.scalar("key"), object.scalar("value")) {
        output.insert(key.to_ascii_lowercase(), value.to_owned());
        return;
    }
    for (key, value) in object.entries() {
        match value {
            KvValue::Scalar(value) => {
                output.insert(key.to_ascii_lowercase(), value.clone());
            }
            KvValue::Object(nested) => flatten_attrib(nested, output),
        }
    }
}

fn normalize_item_values(item: &mut ItemStats, raw_values: BTreeMap<String, String>) {
    for (raw_key, raw_value) in raw_values {
        let key = normalize_attrib_label(&raw_key);
        if key == "bonus_stat" || key == "bonus_attribute" {
            for value in item.attributes.values_mut() {
                *value = None;
            }
            item.unknown.insert(raw_key, raw_value);
            continue;
        }
        let Some(field) = item_field_for_key(&key) else {
            match key.as_str() {
                "hp_regen_amp" | "aura_health_regen" => {
                    item.effects.insert("healthRegen".to_owned(), None);
                }
                "mana_regen_multiplier" => {
                    item.effects.insert("manaRegen".to_owned(), None);
                }
                _ => {}
            }
            item.unknown.insert(raw_key, raw_value);
            continue;
        };
        let Some(value) = parse_number(&raw_value) else {
            if let Some(value) = item.attributes.get_mut(field) {
                *value = None;
            } else if let Some(value) = item.effects.get_mut(field) {
                *value = None;
            }
            item.unknown.insert(raw_key, raw_value);
            continue;
        };
        let target = if item.attributes.contains_key(field) {
            item.attributes.get_mut(field)
        } else {
            item.effects.get_mut(field)
        };
        if let Some(target) = target {
            if let Some(current) = target {
                *current += value;
            }
        }
    }
}

fn normalize_attrib_label(raw: &str) -> String {
    let mut label = raw
        .trim()
        .trim_start_matches('+')
        .trim()
        .trim_end_matches('%')
        .trim()
        .to_ascii_lowercase();
    let prefix_len = label
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit() && ch != '.' && ch != ' ').then_some(index))
        .unwrap_or(label.len());
    if prefix_len > 0 {
        label.drain(..prefix_len);
        label = label.trim().to_owned();
    }
    label.trim_matches(['+', '%', ' ']).replace([' ', '-'], "_")
}

fn item_field_for_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "strength" | "bonus_strength" | "bonus_str" | "str" => "strength",
        "agility" | "bonus_agility" | "bonus_agi" | "agi" => "agility",
        "intellect" | "intelligence" | "bonus_intellect" | "bonus_intelligence" | "bonus_int"
        | "int" => "intelligence",
        "all_stats"
        | "bonus_all_stats"
        | "bonus_stats"
        | "all_attributes"
        | "bonus_all_attributes" => "all",
        "armor" | "bonus_armor" | "armor_bonus" => "armor",
        "attack_speed" | "bonus_attack_speed" | "attack_speed_bonus" => "attackSpeed",
        "damage" | "bonus_damage" | "damage_bonus" | "attack_damage" => "damage",
        "bonus_movement_speed"
        | "bonus_move_speed"
        | "bonus_movement"
        | "movement_speed"
        | "move_speed" => "moveSpeedFlat",
        "bonus_movement_speed_melee" => "moveSpeedBootsMelee",
        "bonus_movement_speed_ranged" => "moveSpeedBootsRanged",
        "bonus_movement_speed_pct"
        | "movement_speed_pct"
        | "move_speed_pct"
        | "movement_speed_percent_bonus"
        | "movespeed_pct" => "moveSpeedPercent",
        "armor_aura" | "aura_armor" => "armor",
        "bonus_magical_armor"
        | "bonus_magic_resistance"
        | "magic_resistance"
        | "magic_resist"
        | "bonus_spell_resist" => "magicResistance",
        "evasion" | "bonus_evasion" => "evasion",
        "status_resistance" | "bonus_status_resistance" => "statusResistance",
        "spell_amp" | "bonus_spell_amp" => "spellAmp",
        "lifesteal" | "bonus_lifesteal" | "lifesteal_percent" | "attack_lifesteal"
        | "lifesteal_aura" => "lifesteal",
        "spell_lifesteal" | "bonus_spell_lifesteal" => "spellLifesteal",
        "health_regen" | "bonus_health_regen" | "bonus_hp_regen" | "hp_regen" | "bonus_regen" => {
            "healthRegen"
        }
        "mana_regen" | "bonus_mana_regen" | "bonus_mp_regen" | "mp_regen" | "aura_mana_regen"
        | "mana_regen_aura" => "manaRegen",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::ItemStats;
    use super::{
        ability_slots, flatten_attrib, item_field_for_key, normalize_attrib_label,
        normalize_item_values,
    };
    use crate::keyvalues;
    use std::collections::BTreeMap;

    #[test]
    fn exports_hero_abilities_in_slot_order_with_ultimate_behavior_and_modifiers() {
        let hero = keyvalues::parse(
            r#""npc_dota_hero_test" { "Ability3" "test_ultimate" "Ability1" "test_stun" "Ability2" "generic_hidden" }"#,
        )
        .expect("hero fixture parses");
        let abilities = keyvalues::parse(
            r#""DOTAAbilities" {
                "test_ultimate" { "AbilityType" "ABILITY_TYPE_ULTIMATE" "AbilityBehavior" "DOTA_ABILITY_BEHAVIOR_NO_TARGET | DOTA_ABILITY_BEHAVIOR_AOE" }
                "test_stun" { "AbilityType" "ABILITY_TYPE_BASIC" "AbilityBehavior" "DOTA_ABILITY_BEHAVIOR_UNIT_TARGET" "Modifiers" { "modifier_test_stunned" { "Duration" "2" } } }
            }"#,
        )
        .expect("ability fixture parses");
        let hero_unit = hero.object("npc_dota_hero_test").unwrap();
        let ability_root = abilities.object("DOTAAbilities").unwrap();

        let rows = ability_slots(Some(hero_unit), None, ability_root, ability_root);
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].slot, rows[0].name.as_str()), (1, "test_stun"));
        assert_eq!(rows[0].is_ultimate, Some(false));
        assert_eq!(rows[0].behavior, ["DOTA_ABILITY_BEHAVIOR_UNIT_TARGET"]);
        assert_eq!(rows[0].modifiers, ["modifier_test_stunned"]);
        assert_eq!((rows[1].slot, rows[1].name.as_str()), (3, "test_ultimate"));
        assert_eq!(rows[1].is_ultimate, Some(true));
        assert_eq!(rows[1].behavior.len(), 2);
    }

    #[test]
    fn maps_supported_ability_value_names_to_formula_inputs() {
        assert_eq!(item_field_for_key("bonus_strength"), Some("strength"));
        assert_eq!(item_field_for_key("bonus_armor"), Some("armor"));
        assert_eq!(item_field_for_key("armor_aura"), Some("armor"));
        assert_eq!(item_field_for_key("movement_speed"), Some("moveSpeedFlat"));
        assert_eq!(item_field_for_key("move_speed"), Some("moveSpeedFlat"));
        assert_eq!(
            item_field_for_key("bonus_movement_speed_pct"),
            Some("moveSpeedPercent")
        );
        assert_eq!(
            item_field_for_key("movement_speed_percent_bonus"),
            Some("moveSpeedPercent")
        );
        assert_eq!(item_field_for_key("bonus_regen"), Some("healthRegen"));
        assert_eq!(item_field_for_key("aura_mana_regen"), Some("manaRegen"));
        assert_eq!(item_field_for_key("lifesteal_aura"), Some("lifesteal"));
        assert_eq!(
            item_field_for_key("bonus_movement_speed_melee"),
            Some("moveSpeedBootsMelee")
        );
        assert_eq!(
            item_field_for_key("bonus_movement_speed_ranged"),
            Some("moveSpeedBootsRanged")
        );
        assert_eq!(item_field_for_key("ability_duration"), None);
        assert!(super::is_boots_item("item_power_treads"));
        assert!(!super::is_boots_item("item_wind_lace"));
    }

    #[test]
    fn normalizes_hero_attack_capability_for_boot_speed_selection() {
        assert_eq!(
            super::normalize_attack_type("DOTA_UNIT_CAP_MELEE_ATTACK").as_deref(),
            Some("melee")
        );
        assert_eq!(
            super::normalize_attack_type("DOTA_UNIT_CAP_RANGED_ATTACK").as_deref(),
            Some("ranged")
        );
        assert_eq!(super::normalize_attack_type("unknown"), None);
    }

    #[test]
    fn reads_primary_attribute_from_hero_index_and_preserves_universal() {
        let hero = keyvalues::parse(r#""npc_dota_hero_test" { "AttackRange" "150" }"#)
            .expect("hero fixture parses");
        let index = keyvalues::parse(
            r#""DOTAUnits" { "npc_dota_hero_test" { "AttributePrimary" "DOTA_ATTRIBUTE_ALL" } }"#,
        )
        .expect("index fixture parses");
        let index_unit = super::find_unit(&index, "npc_dota_hero_test");
        assert_eq!(
            super::primary_attribute_from(&hero, index_unit, None).as_deref(),
            Some("universal")
        );

        let explicit = keyvalues::parse(
            r#""npc_dota_hero_test" { "AttributePrimary" "DOTA_ATTRIBUTE_AGILITY" }"#,
        )
        .expect("hero override fixture parses");
        let explicit_unit = super::find_unit(&explicit, "npc_dota_hero_test").unwrap();
        assert_eq!(
            super::primary_attribute_from(explicit_unit, index_unit, None).as_deref(),
            Some("agility")
        );
    }

    #[test]
    fn reads_attrib_key_value_entries_and_human_labels() {
        let fixture = keyvalues::parse(
            r#""item_test" { "attrib" { "0" { "key" "+10 Strength" "value" "10" } } }"#,
        )
        .expect("fixture parses");
        let attrib = fixture
            .object("item_test")
            .and_then(|item| item.object("attrib"))
            .unwrap();
        let mut values = BTreeMap::new();
        flatten_attrib(attrib, &mut values);
        assert_eq!(values.get("+10 strength").map(String::as_str), Some("10"));
        assert_eq!(normalize_attrib_label("+10 Strength"), "strength");
    }

    #[test]
    fn keeps_unmapped_values_and_marks_dynamic_attributes_unknown() {
        let mut item = ItemStats::default();
        item.attributes.insert("strength".to_owned(), Some(0.0));
        item.attributes.insert("agility".to_owned(), Some(0.0));
        item.attributes.insert("intelligence".to_owned(), Some(0.0));
        item.attributes.insert("all".to_owned(), Some(0.0));
        item.effects.insert("damage".to_owned(), Some(0.0));
        normalize_item_values(
            &mut item,
            BTreeMap::from([
                ("bonus_stat".to_owned(), "10".to_owned()),
                ("cleave_damage".to_owned(), "60".to_owned()),
            ]),
        );
        assert_eq!(item.attributes["strength"], None);
        assert_eq!(
            item.unknown.get("cleave_damage").map(String::as_str),
            Some("60")
        );
    }

    #[test]
    fn unknown_regen_amplifiers_keep_regeneration_unknown() {
        let mut item = ItemStats::default();
        item.effects.insert("healthRegen".to_owned(), Some(2.5));
        item.effects.insert("manaRegen".to_owned(), Some(1.0));
        normalize_item_values(
            &mut item,
            BTreeMap::from([
                ("aura_health_regen".to_owned(), "2.5".to_owned()),
                ("mana_regen_multiplier".to_owned(), "20".to_owned()),
            ]),
        );
        assert_eq!(item.effects["healthRegen"], None);
        assert_eq!(item.effects["manaRegen"], None);
        assert_eq!(item.unknown["aura_health_regen"], "2.5");
        assert_eq!(item.unknown["mana_regen_multiplier"], "20");
    }
}
