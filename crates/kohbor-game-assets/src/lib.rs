pub mod game_stats;
pub mod keyvalues;
mod texture;
mod vpk;

pub use texture::{decode_vtex_c, decode_vtex_c_with_format, Image};
pub use vpk::{Entry, Error, Result, Vpk};

/// Game-owned stat icons found in the installed Dota VPK. Output names are
/// stable asset keys; source paths are exact, case-normalized VPK entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatIconSource {
    pub name: &'static str,
    pub source_path: &'static str,
}

pub const STAT_ICON_SOURCES: [StatIconSource; 13] = [
    StatIconSource {
        name: "primary_strength",
        source_path:
            "panorama/images/primary_attribute_icons/primary_attribute_icon_strength_psd.vtex_c",
    },
    StatIconSource {
        name: "primary_agility",
        source_path:
            "panorama/images/primary_attribute_icons/primary_attribute_icon_agility_psd.vtex_c",
    },
    StatIconSource {
        name: "primary_intelligence",
        source_path:
            "panorama/images/primary_attribute_icons/primary_attribute_icon_intelligence_psd.vtex_c",
    },
    StatIconSource {
        name: "primary_universal",
        source_path:
            "panorama/images/primary_attribute_icons/primary_attribute_icon_all_psd.vtex_c",
    },
    StatIconSource {
        name: "damage",
        source_path: "panorama/images/hud/reborn/icon_damage_psd.vtex_c",
    },
    StatIconSource {
        name: "health_regen",
        source_path: "panorama/images/hud/reborn/health_regen_png.vtex_c",
    },
    StatIconSource {
        name: "mana_regen",
        source_path: "panorama/images/hud/reborn/mana_regen_png.vtex_c",
    },
    StatIconSource {
        name: "armor",
        source_path: "panorama/images/hud/reborn/icon_armor_psd.vtex_c",
    },
    StatIconSource {
        name: "magic_resist",
        source_path: "panorama/images/hud/reborn/icon_magic_resist_psd.vtex_c",
    },
    StatIconSource {
        name: "attack_speed",
        source_path: "panorama/images/hud/reborn/icon_attack_speed_psd.vtex_c",
    },
    StatIconSource {
        name: "move_speed",
        source_path: "panorama/images/hud/reborn/icon_speed_psd.vtex_c",
    },
    StatIconSource {
        name: "evasion",
        source_path: "panorama/images/hud/reborn/icon_evasion_png.vtex_c",
    },
    StatIconSource {
        name: "lifesteal",
        source_path: "panorama/images/items/lifesteal_png.vtex_c",
    },
];

/// Kinds of card icon that the asset build tool exports from the installed game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    Hero,
    Ability,
    Item,
}

impl IconKind {
    pub const ALL: [Self; 3] = [Self::Hero, Self::Ability, Self::Item];

    /// VPK directory that holds this kind's icons, one file per internal name.
    pub fn vpk_dir(self) -> &'static str {
        match self {
            Self::Hero => "panorama/images/heroes",
            Self::Ability => "panorama/images/spellicons",
            Self::Item => "panorama/images/items",
        }
    }

    /// Output folder name, matching the Match view's icon key directories.
    pub fn out_dir(self) -> &'static str {
        match self {
            Self::Hero => "heroes",
            Self::Ability => "abilities",
            Self::Item => "items",
        }
    }
}

/// Internal name for an icon resource path, in the form the Match view keys
/// icons by: heroes without `npc_dota_hero_`, items without `item_`, abilities
/// as is. Returns `None` for anything that is not a direct `<name>_png.vtex_c`
/// child of the kind's directory or whose name is not `[a-z0-9_]{1,64}`.
pub fn icon_name(kind: IconKind, path: &str) -> Option<String> {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    let file = path.strip_prefix(kind.vpk_dir())?.strip_prefix('/')?;
    let mut name = file.strip_suffix("_png.vtex_c")?;
    name = match kind {
        IconKind::Hero => name.strip_prefix("npc_dota_hero_")?,
        IconKind::Item => name.strip_prefix("item_").unwrap_or(name),
        IconKind::Ability => name,
    };
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    valid.then(|| name.to_owned())
}

/// Return the installed-game resources that can provide a hero portrait.
///
/// The game ID may be either `lion` or `npc_dota_hero_lion`. Invalid IDs produce
/// no candidates rather than being interpolated into an archive path.
pub fn hero_portrait_path(hero_id: &str) -> Vec<String> {
    let mut id = hero_id.trim().to_ascii_lowercase();
    if let Some(stripped) = id.strip_prefix("npc_dota_hero_") {
        id = stripped.to_owned();
    }
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Vec::new();
    }

    let file = format!("npc_dota_hero_{id}_png.vtex_c");
    ["", "selection/", "icons/"]
        .into_iter()
        .map(|folder| format!("panorama/images/heroes/{folder}{file}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{hero_portrait_path, icon_name, IconKind};

    #[test]
    fn icon_names_follow_the_match_view_keys() {
        assert_eq!(
            icon_name(
                IconKind::Hero,
                "panorama/images/heroes/npc_dota_hero_dark_seer_png.vtex_c"
            )
            .as_deref(),
            Some("dark_seer")
        );
        assert_eq!(
            icon_name(
                IconKind::Ability,
                "PANORAMA\\IMAGES\\SPELLICONS\\earthshaker_echo_slam_png.vtex_c"
            )
            .as_deref(),
            Some("earthshaker_echo_slam")
        );
        assert_eq!(
            icon_name(IconKind::Item, "panorama/images/items/blink_png.vtex_c").as_deref(),
            Some("blink")
        );
        assert_eq!(
            icon_name(
                IconKind::Item,
                "panorama/images/items/item_aegis_png.vtex_c"
            )
            .as_deref(),
            Some("aegis")
        );
        // Subfolders, other kinds' folders, other suffixes and odd names are refused.
        assert!(icon_name(
            IconKind::Hero,
            "panorama/images/heroes/selection/npc_dota_hero_lion_png.vtex_c"
        )
        .is_none());
        assert!(icon_name(IconKind::Hero, "panorama/images/heroes/lion_png.vtex_c").is_none());
        assert!(icon_name(
            IconKind::Item,
            "panorama/images/spellicons/blink_png.vtex_c"
        )
        .is_none());
        assert!(icon_name(IconKind::Item, "panorama/images/items/blink.vtex_c").is_none());
        assert!(icon_name(IconKind::Item, "panorama/images/items/bl-ink_png.vtex_c").is_none());
        assert!(icon_name(IconKind::Ability, "panorama/images/spellicons/_png.vtex_c").is_none());
        assert_eq!(IconKind::Ability.out_dir(), "abilities");
    }

    #[test]
    fn hero_paths_cover_the_observed_portrait_folders() {
        assert_eq!(
            hero_portrait_path("lion"),
            [
                "panorama/images/heroes/npc_dota_hero_lion_png.vtex_c",
                "panorama/images/heroes/selection/npc_dota_hero_lion_png.vtex_c",
                "panorama/images/heroes/icons/npc_dota_hero_lion_png.vtex_c",
            ]
        );
        assert_eq!(
            hero_portrait_path("npc_dota_hero_AXE")[0],
            "panorama/images/heroes/npc_dota_hero_axe_png.vtex_c"
        );
        assert!(hero_portrait_path("../lion").is_empty());
        assert!(hero_portrait_path("").is_empty());
    }
}
