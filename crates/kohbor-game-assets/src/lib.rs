mod texture;
mod vpk;

pub use texture::{decode_vtex_c, Image};
pub use vpk::{Entry, Error, Result, Vpk};

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
    use super::hero_portrait_path;

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
