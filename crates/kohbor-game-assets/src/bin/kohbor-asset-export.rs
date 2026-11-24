//! Developer tool: export the Match view's card images and stats from the owner's
//! installed Dota 2 into Kohbor's source tree. Not shipped in the app.
//!
//! Authority: DEC-REPLAY-INTEGRATION item 3 (hero, ability and item images may
//! be bundled, built from the installed game with no download; rights review
//! required before any public release).
//!
//! The game folder is only read. Output goes to `<out>/<game build>/`, which
//! must not exist yet, so a rerun never mixes two builds or two tool versions.
//!
//! Usage:
//!   kohbor-asset-export --out <dir> [--game <dota 2 beta dir>] [--map-entry <vpk path>]
//!   kohbor-asset-export --preview <vpk path> <out.png> [--game <dir>]

use flate2::write::ZlibEncoder;
use flate2::Compression;
use kohbor_game_assets::{decode_vtex_c, game_stats, icon_name, IconKind, Image, Vpk};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{collections::BTreeSet, error::Error};

/// Minimap overview. The Match view's prototype calibration (matchTables.mjs
/// `MAP`) was fitted on a 1024 px 7.41 minimap whose frame and art match this
/// 7.37 overview within a few pixels; `dota_psd_8b4a9409` shows the newer
/// corner features but at a different scale, so it would need its own fit.
const MAP_ENTRIES: [&str; 1] = ["materials/overviews/dota_737_psd_28d44696.vtex_c"];
const MAP_SIZE: u32 = 1024;

fn card_size(kind: IconKind) -> (u32, u32) {
    match kind {
        IconKind::Hero => (128, 72),
        IconKind::Ability => (64, 64),
        IconKind::Item => (88, 64),
    }
}

struct Args {
    out: Option<PathBuf>,
    game: Option<PathBuf>,
    map_entry: Option<String>,
    preview: Option<(String, PathBuf)>,
    stats_only: bool,
}

struct Written {
    rel: String,
    bytes: u64,
    sha256: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("asset export failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args(std::env::args().skip(1))?;
    let game = match args.game.clone() {
        Some(game) => game,
        None => locate_game().ok_or("Dota 2 install not found; pass --game")?,
    };
    let game = fs::canonicalize(&game)?;
    let vpk_path = game.join("game/dota/pak01_dir.vpk");
    let vpk = Vpk::open(&vpk_path)?;

    if let Some((entry, out)) = args.preview {
        refuse_inside(&out, &game)?;
        let image = load(&vpk, &entry)?;
        println!("{entry}: {}x{}", image.width, image.height);
        write_new(&out, &encode_png(&image)?)?;
        return Ok(());
    }

    let out_root = args.out.ok_or("--out is required")?;
    let build = game_build(&game).ok_or("could not read ClientVersion from steam.inf")?;
    fs::create_dir_all(&out_root)?;
    let out_root = fs::canonicalize(&out_root)?;
    refuse_inside(&out_root, &game)?;
    let out = out_root.join(&build);
    if args.stats_only {
        fs::create_dir_all(&out)?;
        let out = fs::canonicalize(&out)?;
        refuse_inside(&out, &game)?;
        let stats_path = out.join("stats.json");
        let json = stats_json(&vpk, &game, &build)?;
        write_new(&stats_path, json.as_bytes())?;
        println!("stats.json: {} bytes", json.len());
        println!("sha256: {}", hex(&sha256(json.as_bytes())));
        return Ok(());
    }
    if out.exists() {
        return Err(format!("{} already exists; remove it first", out.display()).into());
    }

    let mut written: Vec<Written> = Vec::new();
    let mut counts = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for kind in IconKind::ALL {
        let dir = out.join(kind.out_dir());
        fs::create_dir_all(&dir)?;
        let (w, h) = card_size(kind);
        let mut count = 0_usize;
        for entry in vpk.list_dir(kind.vpk_dir(), "vtex_c")? {
            let Some(name) = icon_name(kind, entry.path()) else {
                continue;
            };
            let file = dir.join(format!("{name}.png"));
            if file.exists() {
                // `item_x` and `x` both map to `x`; keep the first (sorted) one.
                continue;
            }
            let image = match vpk.read(&entry).and_then(|bytes| decode_vtex_c(&bytes)) {
                Ok(image) => image,
                Err(error) => {
                    failures.push(format!("{}: {error}", entry.path()));
                    continue;
                }
            };
            let png = encode_png(&fit(&image, w, h))?;
            written.push(record(&out, &file, &png)?);
            write_new(&file, &png)?;
            count += 1;
        }
        println!("{}: {count}", kind.out_dir());
        counts.push((kind.out_dir(), count));
    }

    let candidates: Vec<String> = match args.map_entry {
        Some(entry) => vec![entry],
        None => MAP_ENTRIES.iter().map(|s| (*s).to_owned()).collect(),
    };
    let mut map_source = None;
    for entry in &candidates {
        match load(&vpk, entry) {
            Ok(image) => {
                let png = encode_png(&fit(&image, MAP_SIZE, MAP_SIZE))?;
                let file = out.join("map.png");
                written.push(record(&out, &file, &png)?);
                write_new(&file, &png)?;
                println!("map: {entry} ({}x{})", image.width, image.height);
                map_source = Some(entry.clone());
                break;
            }
            Err(error) => failures.push(format!("{entry}: {error}")),
        }
    }

    for failure in &failures {
        eprintln!("skipped {failure}");
    }
    let total: u64 = written.iter().map(|w| w.bytes).sum();
    println!("files={} total_bytes={total}", written.len());

    let provenance = provenance_json(
        &build,
        &game,
        &counts,
        map_source.as_deref(),
        &failures,
        &written,
        total,
    );
    write_new(&out.join("provenance.json"), provenance.as_bytes())?;
    Ok(())
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        out: None,
        game: None,
        map_entry: None,
        preview: None,
        stats_only: false,
    };
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--out" => parsed.out = Some(PathBuf::from(value("--out")?)),
            "--game" => parsed.game = Some(PathBuf::from(value("--game")?)),
            "--map-entry" => parsed.map_entry = Some(value("--map-entry")?),
            "--stats-only" => parsed.stats_only = true,
            "--preview" => {
                let entry = value("--preview")?;
                let out = PathBuf::from(value("--preview")?);
                parsed.preview = Some((entry, out));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(parsed)
}

fn load(vpk: &Vpk, path: &str) -> Result<Image, Box<dyn std::error::Error>> {
    let entry = vpk.find(path).ok_or_else(|| format!("{path} not found"))?;
    Ok(decode_vtex_c(&vpk.read(&entry)?)?)
}

fn record(root: &Path, file: &Path, bytes: &[u8]) -> io::Result<Written> {
    let rel = file
        .strip_prefix(root)
        .map_err(|_| io::Error::other("output escaped its root"))?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Written {
        rel,
        bytes: bytes.len() as u64,
        sha256: hex(&sha256(bytes)),
    })
}

fn refuse_inside(path: &Path, game: &Path) -> Result<(), String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(path)
    };
    let lower = |p: &Path| p.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    let (a, g) = (lower(&absolute), lower(game));
    let g = g.trim_start_matches("//?/").to_owned();
    if a.trim_start_matches("//?/").starts_with(&g) {
        return Err("refusing to write inside the game folder".into());
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

/* ---------------- install discovery (same rules as the desktop host) ---------------- */

fn locate_game() -> Option<PathBuf> {
    let steam = steam_path()?;
    let mut libraries = vec![steam.clone()];
    if let Ok(text) = fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("\"path\"") {
                let value = rest.trim().trim_matches('"').replace("\\\\", "\\");
                if !value.is_empty() {
                    libraries.push(PathBuf::from(value));
                }
            }
        }
    }
    libraries.into_iter().find_map(|library| {
        let text = fs::read_to_string(library.join("steamapps/appmanifest_570.acf")).ok()?;
        let dir = installdir(&text)?;
        let game = library.join("steamapps/common").join(dir);
        game.join("game/dota/pak01_dir.vpk")
            .is_file()
            .then_some(game)
    })
}

fn installdir(manifest: &str) -> Option<String> {
    let dir = manifest.lines().find_map(|line| {
        line.trim()
            .strip_prefix("\"installdir\"")
            .map(|rest| rest.trim().trim_matches('"').to_owned())
    })?;
    let single = !dir.is_empty() && dir != "." && dir != ".." && !dir.contains(['/', '\\', ':']);
    single.then_some(dir)
}

#[cfg(windows)]
fn steam_path() -> Option<PathBuf> {
    // Reads HKCU\Software\Valve\Steam\SteamPath with the system `reg` tool,
    // so this dev tool needs no registry crate.
    let output = Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines().find_map(|line| {
        let (_, value) = line.split_once("REG_SZ")?;
        let value = value.trim();
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

#[cfg(not(windows))]
fn steam_path() -> Option<PathBuf> {
    None
}

fn game_build(game: &Path) -> Option<String> {
    let text = fs::read_to_string(game.join("game/dota/steam.inf")).ok()?;
    steam_inf_build(&text)
}

fn steam_inf_build(text: &str) -> Option<String> {
    let value = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("ClientVersion="))?
        .trim();
    (!value.is_empty() && value.len() <= 12 && value.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.to_owned())
}

fn git_head() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".to_owned())
}

fn git_dirty() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| !output.stdout.is_empty())
}

/* ---------------- provenance ---------------- */

fn provenance_json(
    build: &str,
    game: &Path,
    counts: &[(&str, usize)],
    map_source: Option<&str>,
    failures: &[String],
    written: &[Written],
    total: u64,
) -> String {
    let mut s = String::new();
    let date = utc_date();
    let _ = writeln!(s, "{{");
    let _ = writeln!(s, "  \"schema\": \"kohbor.dota-card-assets/1\",");
    let _ = writeln!(s, "  \"gameBuild\": {},", json_str(build));
    let _ = writeln!(s, "  \"source\": {{");
    let _ = writeln!(
        s,
        "    \"install\": \"installed Dota 2 on the owner's machine, read only, no download\","
    );
    let _ = writeln!(
        s,
        "    \"vpk\": {},",
        json_str(
            &game
                .join("game/dota/pak01_dir.vpk")
                .file_name()
                .map_or_else(
                    || "pak01_dir.vpk".to_owned(),
                    |n| format!("game/dota/{}", n.to_string_lossy())
                )
        )
    );
    let _ = writeln!(s, "    \"directories\": {{");
    let dirs: Vec<String> = IconKind::ALL
        .iter()
        .map(|k| format!("      {}: {}", json_str(k.out_dir()), json_str(k.vpk_dir())))
        .collect();
    let _ = writeln!(s, "{}", dirs.join(",\n"));
    let _ = writeln!(s, "    }},");
    let _ = writeln!(
        s,
        "    \"map\": {}",
        map_source.map_or_else(|| "null".to_owned(), json_str)
    );
    let _ = writeln!(s, "  }},");
    let _ = writeln!(s, "  \"sizes\": {{ \"heroes\": [128, 72], \"abilities\": [64, 64], \"items\": [88, 64], \"map\": [{MAP_SIZE}, {MAP_SIZE}] }},");
    let count_text: Vec<String> = counts
        .iter()
        .map(|(k, n)| format!("{}: {n}", json_str(k)))
        .chain(std::iter::once(format!(
            "\"map\": {}",
            usize::from(map_source.is_some())
        )))
        .collect();
    let _ = writeln!(s, "  \"counts\": {{ {} }},", count_text.join(", "));
    let _ = writeln!(s, "  \"totalBytes\": {total},");
    let _ = writeln!(
        s,
        "  \"tool\": {{ \"name\": \"kohbor-asset-export\", \"commit\": {} }},",
        json_str(&git_head())
    );
    let _ = writeln!(s, "  \"date\": {},", json_str(&date));
    let _ = writeln!(s, "  \"authority\": \"DEC-REPLAY-INTEGRATION item 3\",");
    let _ = writeln!(
        s,
        "  \"rights\": \"Valve game art. Rights review required before public release.\","
    );
    let skipped: Vec<String> = failures
        .iter()
        .map(|f| format!("    {}", json_str(f)))
        .collect();
    if skipped.is_empty() {
        let _ = writeln!(s, "  \"skipped\": [],");
    } else {
        let _ = writeln!(s, "  \"skipped\": [\n{}\n  ],", skipped.join(",\n"));
    }
    let files: Vec<String> = written
        .iter()
        .map(|w| {
            format!(
                "    {{ \"path\": {}, \"bytes\": {}, \"sha256\": \"{}\" }}",
                json_str(&w.rel),
                w.bytes,
                w.sha256
            )
        })
        .collect();
    let _ = writeln!(s, "  \"files\": [\n{}\n  ]", files.join(",\n"));
    let _ = writeln!(s, "}}");
    s
}

fn json_str(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn stats_json(vpk: &Vpk, game: &Path, build: &str) -> Result<String, Box<dyn Error>> {
    let data = game_stats::extract(vpk)?;
    let mut physical_vpks = BTreeSet::new();
    physical_vpks.insert(game.join("game/dota/pak01_dir.vpk"));
    for source in &data.source_blobs {
        physical_vpks.insert(source.archive.clone());
    }

    let mut s = String::new();
    let _ = writeln!(s, "{{");
    let _ = writeln!(s, "  \"schema\": \"kohbor.dota-derived-stats/1\",");
    let build_number = build.parse::<u32>()?;
    let _ = writeln!(s, "  \"build\": {build_number},");
    let _ = writeln!(s, "  \"patch\": \"unverified\",");
    let _ = writeln!(s, "  \"provenance\": {{");
    let _ = writeln!(
        s,
        "    \"install\": \"installed Dota 2; read only; no download\","
    );
    let _ = writeln!(
        s,
        "    \"patchEvidence\": \"no explicit 7.41f or 7.42 string found in inspected game files\","
    );
    let _ = writeln!(s, "    \"exportedOn\": {},", json_str(&utc_date()));
    let _ = writeln!(
        s,
        "    \"authority\": \"DEC-REPLAY-INTEGRATION item 3; owner decision for local extraction\","
    );
    let _ = writeln!(
        s,
        "    \"rights\": \"public redistribution rights are not established; rights review required before public release\","
    );
    let _ = writeln!(s, "    \"vpk\": [");
    let mut vpk_records = Vec::new();
    for path in physical_vpks {
        let bytes = fs::read(&path)?;
        let relative = relative_game_path(game, &path)?;
        vpk_records.push(format!(
            "      {{ \"path\": {}, \"bytes\": {}, \"sha256\": \"{}\" }}",
            json_str(&relative),
            bytes.len(),
            hex(&sha256(&bytes))
        ));
    }
    let _ = writeln!(s, "{}", vpk_records.join(",\n"));
    let _ = writeln!(s, "    ],");
    let _ = writeln!(s, "    \"scripts\": [");
    let mut script_records = Vec::new();
    for source in &data.source_blobs {
        let relative_vpk = relative_game_path(game, &source.archive)?;
        script_records.push(format!(
            "      {{ \"path\": {}, \"vpk\": {}, \"bytes\": {}, \"sha256\": \"{}\" }}",
            json_str(&source.path),
            json_str(&relative_vpk),
            source.bytes.len(),
            hex(&sha256(&source.bytes))
        ));
    }
    let _ = writeln!(s, "{}", script_records.join(",\n"));
    let _ = writeln!(s, "    ],");
    let _ = writeln!(
        s,
        "    \"tool\": {{ \"name\": \"kohbor-asset-export\", \"commit\": {}, \"workingTree\": {} }}",
        json_str(&git_head()),
        json_str(if git_dirty() { "dirty" } else { "clean" })
    );
    let _ = writeln!(s, "  }},");
    let _ = writeln!(s, "  \"heroes\": {{");
    let hero_records: Vec<String> = data
        .heroes
        .iter()
        .map(|(id, hero)| {
            let v = &hero.values;
            let source_list = hero
                .sources
                .iter()
                .map(|source| json_str(source))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                concat!(
                    "    {}: {{ ",
                    "\"primaryAttribute\": {}, ",
                    "\"attributes\": {{ ",
                    "\"strength\": {{ \"base\": {}, \"gain\": {} }}, ",
                    "\"agility\": {{ \"base\": {}, \"gain\": {} }}, ",
                    "\"intelligence\": {{ \"base\": {}, \"gain\": {} }} }}, ",
                    "\"armor\": {}, ",
                    "\"attack\": {{ \"type\": {}, \"baseAttackSpeed\": {}, \"baseAttackTime\": {}, ",
                    "\"damageMin\": {}, \"damageMax\": {}, \"range\": {} }}, ",
                    "\"moveSpeed\": {}, ",
                    "\"health\": {{ \"base\": {}, \"regen\": {} }}, ",
                    "\"mana\": {{ \"base\": {}, \"regen\": {} }}, ",
                    "\"magicResistance\": {}, \"abilities\": [{}], \"sources\": [{}] }}"
                ),
                json_str(id),
                hero.primary_attribute
                    .as_deref()
                    .map_or_else(|| "null".to_owned(), json_str),
                number_value(v, "baseStrength"),
                number_value(v, "strengthGain"),
                number_value(v, "baseAgility"),
                number_value(v, "agilityGain"),
                number_value(v, "baseIntelligence"),
                number_value(v, "intelligenceGain"),
                number_value(v, "baseArmor"),
                hero.attack_type
                    .as_deref()
                    .map_or_else(|| "null".to_owned(), json_str),
                number_value(v, "baseAttackSpeed"),
                number_value(v, "baseAttackTime"),
                number_value(v, "baseDamageMin"),
                number_value(v, "baseDamageMax"),
                number_value(v, "attackRange"),
                number_value(v, "baseMoveSpeed"),
                number_value(v, "baseHealth"),
                number_value(v, "baseHealthRegen"),
                number_value(v, "baseMana"),
                number_value(v, "baseManaRegen"),
                number_value(v, "baseMagicResistance"),
                hero_abilities_json(&hero.abilities),
                source_list
            )
        })
        .collect();
    let _ = writeln!(s, "{}", hero_records.join(",\n"));
    let _ = writeln!(s, "  }},");
    let _ = writeln!(s, "  \"items\": {{");
    let item_records: Vec<String> = data
        .items
        .iter()
        .map(|(id, item)| {
            let attributes = json_number_map(&item.attributes);
            let effects = json_number_map(&item.effects);
            let unknown = item
                .unknown
                .iter()
                .map(|(key, value)| format!("{}: {}", json_str(key), json_str(value)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                concat!(
                    "    {}: {{ \"attributes\": {{ {} }}, \"effects\": {{ {} }}, ",
                    "\"unknown\": {{ {} }} }}"
                ),
                json_str(id),
                attributes,
                effects,
                unknown
            )
        })
        .collect();
    let _ = writeln!(s, "{}", item_records.join(",\n"));
    let _ = writeln!(s, "  }}");
    let _ = writeln!(s, "}}");
    Ok(s)
}

fn number_value(values: &std::collections::BTreeMap<String, Option<f64>>, key: &str) -> String {
    number_json(values.get(key).copied().flatten())
}

fn number_json(value: Option<f64>) -> String {
    value
        .filter(|value| value.is_finite())
        .map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn json_number_map(values: &std::collections::BTreeMap<String, Option<f64>>) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{}: {}", json_str(key), number_json(*value)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_string_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_str(value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn hero_abilities_json(abilities: &[game_stats::HeroAbility]) -> String {
    abilities
        .iter()
        .map(|ability| {
            let ultimate = ability
                .is_ultimate
                .map_or_else(|| "null".to_owned(), |value| value.to_string());
            format!(
                "{{ \"slot\": {}, \"name\": {}, \"ultimate\": {}, \"behavior\": [{}], \"modifiers\": [{}] }}",
                ability.slot,
                json_str(&ability.name),
                ultimate,
                json_string_array(&ability.behavior),
                json_string_array(&ability.modifiers),
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn relative_game_path(game: &Path, file: &Path) -> io::Result<String> {
    Ok(file
        .strip_prefix(game)
        .map_err(|_| io::Error::other("source file is outside the game directory"))?
        .to_string_lossy()
        .replace('\\', "/"))
}

fn utc_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to a proleptic Gregorian date (H. Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/* ---------------- image fitting ---------------- */

/// Centre-crop to the target aspect ratio, then area-average down (or
/// up-sample by the same weights) with premultiplied alpha so transparent
/// edges do not bleed dark fringes.
fn fit(image: &Image, width: u32, height: u32) -> Image {
    let (sw, sh) = (image.width as f64, image.height as f64);
    let target = width as f64 / height as f64;
    let (cw, ch) = if sw / sh > target {
        (sh * target, sh)
    } else {
        (sw, sw / target)
    };
    let (cx, cy) = ((sw - cw) / 2.0, (sh - ch) / 2.0);
    let xs = weights(cx, cw, image.width, width);
    let ys = weights(cy, ch, image.height, height);
    let premul: Vec<[f64; 4]> = image
        .rgba
        .chunks_exact(4)
        .map(|p| {
            let a = f64::from(p[3]) / 255.0;
            [
                f64::from(p[0]) * a,
                f64::from(p[1]) * a,
                f64::from(p[2]) * a,
                f64::from(p[3]),
            ]
        })
        .collect();
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in &ys {
        for col in &xs {
            let mut acc = [0.0_f64; 4];
            let mut total = 0.0;
            for &(sy, wy) in row {
                for &(sx, wx) in col {
                    let p = premul[sy as usize * image.width as usize + sx as usize];
                    let w = wy * wx;
                    for c in 0..4 {
                        acc[c] += p[c] * w;
                    }
                    total += w;
                }
            }
            let a = if total > 0.0 { acc[3] / total } else { 0.0 };
            for channel in &acc[..3] {
                let v = if a > 0.0 {
                    channel / total / (a / 255.0)
                } else {
                    0.0
                };
                rgba.push(v.round().clamp(0.0, 255.0) as u8);
            }
            rgba.push(a.round().clamp(0.0, 255.0) as u8);
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

/// For each output pixel, the source pixels it covers and their overlap.
fn weights(start: f64, span: f64, source: u32, out: u32) -> Vec<Vec<(u32, f64)>> {
    let scale = span / f64::from(out);
    (0..out)
        .map(|i| {
            let (a, b) = (
                start + f64::from(i) * scale,
                start + f64::from(i + 1) * scale,
            );
            let mut taps = Vec::new();
            if scale <= 1.0 {
                // Up-sampling or same size: nearest source pixel.
                let s = ((a + b) / 2.0).floor().clamp(0.0, f64::from(source - 1)) as u32;
                taps.push((s, 1.0));
            } else {
                let mut s = a.floor();
                while s < b {
                    let overlap = (s + 1.0).min(b) - s.max(a);
                    if overlap > 0.0 {
                        taps.push((s.clamp(0.0, f64::from(source - 1)) as u32, overlap));
                    }
                    s += 1.0;
                }
            }
            taps
        })
        .collect()
}

/* ---------------- PNG writer (zlib via flate2, no png crate) ---------------- */

/// RGB when every pixel is opaque, otherwise RGBA. Each row uses the filter
/// with the smallest sum of absolute values, then zlib at best compression.
fn encode_png(image: &Image) -> io::Result<Vec<u8>> {
    let (w, h) = (image.width as usize, image.height as usize);
    if w == 0 || h == 0 || image.rgba.len() != w * h * 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "bad image size",
        ));
    }
    let opaque = image.rgba.chunks_exact(4).all(|p| p[3] == 255);
    let bpp = if opaque { 3 } else { 4 };
    let pixels: Vec<u8> = if opaque {
        image
            .rgba
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect()
    } else {
        image.rgba.clone()
    };
    let stride = w * bpp;
    let mut raw = Vec::with_capacity((stride + 1) * h);
    let zero = vec![0_u8; stride];
    let mut candidate = vec![0_u8; stride];
    let mut best = vec![0_u8; stride];
    for y in 0..h {
        let row = &pixels[y * stride..(y + 1) * stride];
        let prev = if y == 0 {
            &zero[..]
        } else {
            &pixels[(y - 1) * stride..y * stride]
        };
        let mut best_filter = 0_u8;
        let mut best_score = u64::MAX;
        for filter in 0..5_u8 {
            for i in 0..stride {
                let left = if i >= bpp { row[i - bpp] } else { 0 };
                let up = prev[i];
                let up_left = if i >= bpp { prev[i - bpp] } else { 0 };
                let predicted = match filter {
                    0 => 0,
                    1 => left,
                    2 => up,
                    3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                    _ => paeth(left, up, up_left),
                };
                candidate[i] = row[i].wrapping_sub(predicted);
            }
            let score: u64 = candidate
                .iter()
                .map(|&b| u64::from((b as i8).unsigned_abs()))
                .sum();
            if score < best_score {
                best_score = score;
                best_filter = filter;
                best.copy_from_slice(&candidate);
            }
        }
        raw.push(best_filter);
        raw.extend_from_slice(&best);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&raw)?;
    let compressed = encoder.finish()?;

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&image.width.to_be_bytes());
    ihdr.extend_from_slice(&image.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, if opaque { 2 } else { 6 }, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &compressed);
    chunk(&mut png, b"IEND", &[]);
    Ok(png)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(data);
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    png.extend_from_slice(&crc.sum().to_be_bytes());
}

/* ---------------- SHA-256 (FIPS 180-4) ---------------- */

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut w = [0_u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (a, b) in h.iter_mut().zip(v) {
            *a = a.wrapping_add(b);
        }
    }
    let mut out = [0_u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_fips_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn steam_inf_build_and_installdir_are_validated() {
        assert_eq!(
            steam_inf_build("ClientVersion=6989\nServerVersion=6989\n").as_deref(),
            Some("6989")
        );
        assert!(steam_inf_build("ClientVersion=../x\n").is_none());
        assert!(steam_inf_build("PatchVersion=7.41\n").is_none());
        assert_eq!(
            installdir("\"installdir\"\t\t\"dota 2 beta\"").as_deref(),
            Some("dota 2 beta")
        );
        assert!(installdir("\"installdir\" \"..\\\\x\"").is_none());
    }

    #[test]
    fn fit_halves_by_area_average_and_crops_to_the_target_aspect() {
        let image = Image {
            width: 4,
            height: 2,
            rgba: [[0, 0, 0, 255], [255, 255, 255, 255]]
                .iter()
                .cycle()
                .take(8)
                .flatten()
                .copied()
                .collect(),
        };
        let half = fit(&image, 2, 1);
        assert_eq!(half.rgba, [128, 128, 128, 255, 128, 128, 128, 255]);
        // 4x2 to a square keeps the centre 2x2.
        let square = fit(&image, 2, 2);
        assert_eq!((square.width, square.height), (2, 2));
        assert_eq!(&square.rgba[..4], &[255, 255, 255, 255]);
        // A transparent pixel does not darken its opaque neighbour's colour.
        let edge = Image {
            width: 2,
            height: 2,
            rgba: vec![200, 100, 50, 255, 0, 0, 0, 0, 200, 100, 50, 255, 0, 0, 0, 0],
        };
        assert_eq!(fit(&edge, 1, 1).rgba, [200, 100, 50, 128]);
    }

    #[test]
    fn png_is_rgb_when_opaque_and_rgba_otherwise_with_valid_crcs() {
        let opaque = Image {
            width: 2,
            height: 2,
            rgba: vec![9; 16]
                .into_iter()
                .enumerate()
                .map(|(i, v)| if i % 4 == 3 { 255 } else { v })
                .collect(),
        };
        let png = encode_png(&opaque).expect("encode");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(png[25], 2);
        let crc_end = 8 + 4 + 4 + 13;
        let mut crc = flate2::Crc::new();
        crc.update(&png[12..crc_end]);
        assert_eq!(&png[crc_end..crc_end + 4], &crc.sum().to_be_bytes());
        let alpha = Image {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 4],
        };
        assert_eq!(encode_png(&alpha).expect("encode")[25], 6);
    }

    #[test]
    fn output_inside_the_game_folder_is_refused() {
        let game = Path::new("C:/Games/dota 2 beta");
        assert!(refuse_inside(Path::new("C:/Games/dota 2 beta/out"), game).is_err());
        assert!(refuse_inside(Path::new("C:/Games/DOTA 2 BETA/x.png"), game).is_err());
        assert!(refuse_inside(Path::new("F:/kohbor/apps/desktop/src/assets/dota"), game).is_ok());
    }
}
