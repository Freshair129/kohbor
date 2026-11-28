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
//!   kohbor-asset-export --out <dir> [--game <dota 2 beta dir>] [--map-style detailed|simple] [--map-entry <vpk path>]
//!   kohbor-asset-export --map-only --out <dir> [--game <dota 2 beta dir>] [--map-style detailed|simple] [--map-entry <vpk path>]
//!   kohbor-asset-export --preview <vpk path> <out.png> [--game <dir>]
//!   kohbor-asset-export --list-prefix <vpk path prefix> <extension> <path fragment> [--game <dir>]
//!   kohbor-asset-export --stat-icon-formats [--game <dir>]
//!   kohbor-asset-export --refresh-stat-icons --out <dir> [--game <dir>]

use flate2::write::ZlibEncoder;
use flate2::Compression;
use kohbor_game_assets::{
    decode_vtex_c_for_display, decode_vtex_c_for_display_with_format, game_stats, icon_name,
    IconKind, Image, TextureDecodeFlags, Vpk, STAT_ICON_SOURCES,
};
use kohbor_game_assets::keyvalues::{self, KvObject, KvValue};
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{collections::BTreeSet, error::Error};

const MAP_SIZE: u32 = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MapStyle {
    Detailed,
    Simple,
}

impl MapStyle {
    fn as_str(self) -> &'static str {
        match self {
            Self::Detailed => "detailed",
            Self::Simple => "simple",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct OverviewConfig {
    material: String,
    simple_material: String,
    pos_x: f64,
    pos_y: f64,
    scale: f64,
}

#[derive(Clone, Debug)]
struct MapOverview {
    style: MapStyle,
    source_path: String,
    material_logical_path: String,
    simple_material_logical_path: String,
    material_path: String,
    texture_path: String,
    pos_x: f64,
    pos_y: f64,
    scale: f64,
    source_texture_bytes: u64,
    source_texture_sha256: String,
}

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
    map_style: MapStyle,
    preview: Option<(String, PathBuf)>,
    stats_only: bool,
    stat_icons_only: bool,
    refresh_stat_icons: bool,
    stat_icon_formats: bool,
    map_only: bool,
    list_prefix: Option<(String, String, String)>,
}

struct Written {
    rel: String,
    source_path: Option<String>,
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

    if let Some((prefix, extension, fragment)) = args.list_prefix {
        let fragments: Vec<String> = fragment
            .split('|')
            .map(|part| part.to_ascii_lowercase())
            .filter(|part| !part.is_empty())
            .collect();
        for entry in vpk.list_prefix(&prefix, &extension)? {
            if fragments.iter().any(|part| entry.path().contains(part)) {
                println!("{}", entry.path());
            }
        }
        return Ok(());
    }

    if args.stat_icon_formats {
        for source in &STAT_ICON_SOURCES {
            let entry = vpk.find(source.source_path).ok_or_else(|| {
                format!("{} is absent from the installed VPK", source.source_path)
            })?;
            let bytes = vpk.read(&entry)?;
            let (image, format, flags) = decode_vtex_c_for_display_with_format(&bytes)?;
            println!(
                "stat_icon={} source_path={} format={} ({}) flags={} dimensions={}x{}",
                source.name,
                entry.path(),
                format,
                image_format_name(format),
                format_decode_flags(flags),
                image.width,
                image.height,
            );
        }
        return Ok(());
    }

    if let Some((entry, out)) = args.preview {
        refuse_inside(&out, &game)?;
        let (image, format, flags) = load_with_format(&vpk, &entry)?;
        println!(
            "{entry}: format={format} flags={} {}x{}",
            format_decode_flags(flags),
            image.width,
            image.height
        );
        write_new(&out, &encode_png(&image)?)?;
        return Ok(());
    }

    let out_root = args.out.ok_or("--out is required")?;
    let build = game_build(&game).ok_or("could not read ClientVersion from steam.inf")?;
    fs::create_dir_all(&out_root)?;
    let out_root = fs::canonicalize(&out_root)?;
    refuse_inside(&out_root, &game)?;
    let out = out_root.join(&build);
    if args.stat_icons_only {
        if out.exists() {
            return Err(
                format!("{} already exists; refusing to mix exports", out.display()).into(),
            );
        }
        fs::create_dir_all(&out)?;
        let out = fs::canonicalize(&out)?;
        refuse_inside(&out, &game)?;
        let written = export_stat_icons(&vpk, &out, false)?;
        let provenance = stat_icons_provenance_json(&build, &game, &written);
        write_new(&out.join("provenance.json"), provenance.as_bytes())?;
        return Ok(());
    }
    if args.refresh_stat_icons {
        if !out.exists() {
            return Err(format!(
                "{} does not exist; refusing to create a refresh target",
                out.display()
            )
            .into());
        }
        validate_stat_icons_refresh_target(&out)?;
        let out = fs::canonicalize(&out)?;
        refuse_inside(&out, &game)?;
        let written = export_stat_icons(&vpk, &out, true)?;
        let provenance = stat_icons_provenance_json(&build, &game, &written);
        write_replace(&out.join("provenance.json"), provenance.as_bytes())?;
        return Ok(());
    }
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

    let overview = match resolve_map_overview(&vpk, args.map_style) {
        Ok(overview) => Some(overview),
        Err(error) if args.map_entry.is_some() => {
            eprintln!("overview metadata unavailable for explicit --map-entry: {error}");
            None
        }
        Err(error) => return Err(error),
    };
    let map_entry = args
        .map_entry
        .clone()
        .or_else(|| overview.as_ref().map(|overview| overview.texture_path.clone()));
    if args.map_only {
        let entry = map_entry.ok_or("no minimap texture resolved")?;
        if out.exists() {
            return Err(format!("{} already exists; refusing to mix exports", out.display()).into());
        }
        fs::create_dir_all(&out)?;
        let out = fs::canonicalize(&out)?;
        refuse_inside(&out, &game)?;
        let (image, source_bytes) = load_map(&vpk, &entry)?;
        let png = encode_png(&map_export_image(&image))?;
        let file = out.join("map.png");
        let written = record_from_source(&out, &file, &png, &entry)?;
        write_new(&file, &png)?;
        let provenance = map_provenance_json(
            &build,
            &game,
            overview.as_ref(),
            args.map_entry.as_deref(),
            &entry,
            source_bytes.len() as u64,
            &hex(&sha256(&source_bytes)),
            &image,
            &written,
        );
        write_new(&out.join("provenance.json"), provenance.as_bytes())?;
        println!("map: {entry} ({}x{})", image.width, image.height);
        println!("map.png: {} bytes sha256={}", written.bytes, written.sha256);
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
            let image = match vpk
                .read(&entry)
                .and_then(|bytes| decode_vtex_c_for_display(&bytes))
            {
                Ok(image) => image,
                Err(error) => {
                    failures.push(format!("{}: {error}", entry.path()));
                    continue;
                }
            };
            let png = encode_png(&fit(&image, w, h, ResampleFilter::Nearest))?;
            written.push(record_from_source(&out, &file, &png, entry.path())?);
            write_new(&file, &png)?;
            count += 1;
        }
        println!("{}: {count}", kind.out_dir());
        counts.push((kind.out_dir(), count));
    }

    let candidates: Vec<String> = map_entry.into_iter().collect();
    let mut map_source = None;
    let mut map_source_bytes = None;
    let mut map_source_sha256 = None;
    for entry in &candidates {
        match load_map(&vpk, entry) {
            Ok((image, source_bytes)) => {
                let png = encode_png(&map_export_image(&image))?;
                let file = out.join("map.png");
                written.push(record_from_source(&out, &file, &png, entry)?);
                write_new(&file, &png)?;
                println!("map: {entry} ({}x{})", image.width, image.height);
                map_source = Some(entry.clone());
                map_source_bytes = Some(source_bytes.len() as u64);
                map_source_sha256 = Some(hex(&sha256(&source_bytes)));
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
        overview.as_ref(),
        map_source_bytes,
        map_source_sha256.as_deref(),
    );
    write_new(&out.join("provenance.json"), provenance.as_bytes())?;
    Ok(())
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        out: None,
        game: None,
        map_entry: None,
        map_style: MapStyle::Detailed,
        preview: None,
        stats_only: false,
        stat_icons_only: false,
        refresh_stat_icons: false,
        stat_icon_formats: false,
        map_only: false,
        list_prefix: None,
    };
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--out" => parsed.out = Some(PathBuf::from(value("--out")?)),
            "--game" => parsed.game = Some(PathBuf::from(value("--game")?)),
            "--map-entry" => parsed.map_entry = Some(value("--map-entry")?),
            "--map-style" => {
                parsed.map_style = match value("--map-style")?.as_str() {
                    "detailed" => MapStyle::Detailed,
                    "simple" => MapStyle::Simple,
                    other => return Err(format!("--map-style must be detailed or simple, got {other}")),
                };
            }
            "--stats-only" => parsed.stats_only = true,
            "--stat-icons-only" => parsed.stat_icons_only = true,
            "--refresh-stat-icons" => parsed.refresh_stat_icons = true,
            "--stat-icon-formats" => parsed.stat_icon_formats = true,
            "--map-only" => parsed.map_only = true,
            "--list-prefix" => {
                let prefix = value("--list-prefix")?;
                let extension = value("--list-prefix")?;
                let fragment = value("--list-prefix")?;
                parsed.list_prefix = Some((prefix, extension, fragment));
            }
            "--preview" => {
                let entry = value("--preview")?;
                let out = PathBuf::from(value("--preview")?);
                parsed.preview = Some((entry, out));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let export_modes = [
        parsed.stats_only,
        parsed.stat_icons_only,
        parsed.refresh_stat_icons,
        parsed.map_only,
    ]
    .into_iter()
    .filter(|selected| *selected)
    .count();
    if export_modes > 1 || (parsed.stat_icon_formats && export_modes > 0) {
        return Err(
            "--stats-only, --stat-icons-only, --refresh-stat-icons, --map-only and --stat-icon-formats are mutually exclusive"
                .into(),
        );
    }
    Ok(parsed)
}

fn load_map(vpk: &Vpk, path: &str) -> Result<(Image, Vec<u8>), Box<dyn std::error::Error>> {
    let entry = vpk.find(path).ok_or_else(|| format!("{path} not found"))?;
    let bytes = vpk.read(&entry)?;
    let image = decode_vtex_c_for_display(&bytes)?;
    Ok((image, bytes))
}

fn parse_overview_text(text: &str) -> Result<OverviewConfig, String> {
    fn find(object: &KvObject) -> Option<OverviewConfig> {
        let config = OverviewConfig {
            material: object.scalar("material")?.trim().to_owned(),
            simple_material: object.scalar("simple_material")?.trim().to_owned(),
            pos_x: object.scalar("pos_x")?.parse().ok()?,
            pos_y: object.scalar("pos_y")?.parse().ok()?,
            scale: object.scalar("scale")?.parse().ok()?,
        };
        [config.pos_x, config.pos_y, config.scale]
            .into_iter()
            .all(f64::is_finite)
            .then_some(config)
    }
    fn visit(object: &KvObject) -> Option<OverviewConfig> {
        find(object).or_else(|| object.entries().iter().find_map(|(_, value)| match value {
            KvValue::Object(child) => visit(child),
            KvValue::Scalar(_) => None,
        }))
    }

    let root = keyvalues::parse(text).map_err(|error| error.to_string())?;
    visit(&root).ok_or_else(|| "overview has no material, simple_material and calibration".to_owned())
}

fn overview_material(config: &OverviewConfig, style: MapStyle) -> &str {
    match style {
        MapStyle::Detailed => &config.material,
        MapStyle::Simple => &config.simple_material,
    }
}

fn compiled_material_path(path: &str) -> Result<String, String> {
    let path = path.trim().replace('\\', "/");
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".vmat_c") {
        return Ok(path);
    }
    if lower.ends_with(".vmat") {
        return Ok(format!("{}_c", path));
    }
    Err(format!("overview material is not a .vmat path: {path}"))
}

fn extract_vmat_texture_reference(bytes: &[u8]) -> Result<String, String> {
    fn normalize_token(token: &str) -> Option<String> {
        let token = token.replace('\\', "/");
        let lower = token.to_ascii_lowercase();
        if lower.ends_with(".vtex_c") {
            Some(token)
        } else if lower.ends_with(".vtex") {
            Some(format!("{}.vtex_c", &token[..token.len() - ".vtex".len()]))
        } else {
            None
        }
    }

    let text = String::from_utf8_lossy(bytes);
    let mut candidates = BTreeSet::new();
    let mut token = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '\\') {
            token.push(ch);
        } else {
            if let Some(path) = normalize_token(&token) {
                candidates.insert(path);
            }
            token.clear();
        }
    }
    match candidates.len() {
        1 => Ok(candidates.into_iter().next().unwrap()),
        0 => Err("compiled material contains no .vtex_c reference".to_owned()),
        _ => Err(format!("compiled material contains ambiguous texture references: {}", candidates.into_iter().collect::<Vec<_>>().join(", "))),
    }
}

fn resolve_map_overview(vpk: &Vpk, style: MapStyle) -> Result<MapOverview, Box<dyn std::error::Error>> {
    let overview_entry = vpk
        .find("resource/overviews/dota.txt")
        .ok_or("resource/overviews/dota.txt is absent from the installed VPK")?;
    let source_path = overview_entry.path().to_owned();
    let overview_text = String::from_utf8(vpk.read(&overview_entry)?)?;
    let config = parse_overview_text(&overview_text)?;
    let selected_material = overview_material(&config, style);
    let material_path = compiled_material_path(selected_material)?;
    let material_entry = vpk
        .find(&material_path)
        .ok_or_else(|| format!("{} is absent from the installed VPK", material_path))?;
    let material_path = material_entry.path().to_owned();
    let material_bytes = vpk.read(&material_entry)?;
    let texture_path = extract_vmat_texture_reference(&material_bytes)?;
    let texture_entry = vpk
        .find(&texture_path)
        .ok_or_else(|| format!("{} referenced by the {} material is absent from the installed VPK", texture_path, style.as_str()))?;
    let texture_path = texture_entry.path().to_owned();
    let texture_bytes = vpk.read(&texture_entry)?;
    Ok(MapOverview {
        style,
        source_path,
        material_logical_path: config.material,
        simple_material_logical_path: config.simple_material,
        material_path,
        texture_path,
        pos_x: config.pos_x,
        pos_y: config.pos_y,
        scale: config.scale,
        source_texture_bytes: texture_bytes.len() as u64,
        source_texture_sha256: hex(&sha256(&texture_bytes)),
    })
}

fn load_with_format(
    vpk: &Vpk,
    path: &str,
) -> Result<(Image, u8, TextureDecodeFlags), Box<dyn std::error::Error>> {
    let entry = vpk.find(path).ok_or_else(|| format!("{path} not found"))?;
    Ok(decode_vtex_c_for_display_with_format(&vpk.read(&entry)?)?)
}

fn record(root: &Path, file: &Path, bytes: &[u8]) -> io::Result<Written> {
    let rel = file
        .strip_prefix(root)
        .map_err(|_| io::Error::other("output escaped its root"))?
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Written {
        rel,
        source_path: None,
        bytes: bytes.len() as u64,
        sha256: hex(&sha256(bytes)),
    })
}

fn record_from_source(
    root: &Path,
    file: &Path,
    bytes: &[u8],
    source_path: &str,
) -> io::Result<Written> {
    let mut written = record(root, file, bytes)?;
    written.source_path = Some(source_path.to_owned());
    Ok(written)
}

fn export_stat_icons(
    vpk: &Vpk,
    out: &Path,
    replace: bool,
) -> Result<Vec<Written>, Box<dyn std::error::Error>> {
    let directory = out.join("stat-icons");
    fs::create_dir_all(&directory)?;
    let mut written = Vec::with_capacity(STAT_ICON_SOURCES.len());
    for source in &STAT_ICON_SOURCES {
        let entry = vpk
            .find(source.source_path)
            .ok_or_else(|| format!("{} is absent from the installed VPK", source.source_path))?;
        let (image, format, flags) = decode_vtex_c_for_display_with_format(&vpk.read(&entry)?)?;
        let png = encode_png(&image)?;
        let file = directory.join(format!("{}.png", source.name));
        let item = record_from_source(out, &file, &png, entry.path())?;
        if replace {
            write_replace(&file, &png)?;
        } else {
            write_new(&file, &png)?;
        }
        let pixel_check = match source.name {
            "primary_strength" => dominant_pixel(&image, 0),
            "primary_agility" => dominant_pixel(&image, 1),
            "primary_intelligence" => dominant_pixel(&image, 2),
            _ => None,
        };
        let pixel_log = pixel_check
            .map(|(x, y, rgba)| {
                format!(
                    " dominant_pixel=({x},{y}) rgba=[{},{},{},{}]",
                    rgba[0], rgba[1], rgba[2], rgba[3]
                )
            })
            .unwrap_or_default();
        println!(
            "stat_icon={} source_path={} format={} ({}) flags={} out={} dimensions={}x{} bytes={} sha256={}{}",
            source.name,
            entry.path(),
            format,
            image_format_name(format),
            format_decode_flags(flags),
            item.rel,
            image.width,
            image.height,
            item.bytes,
            item.sha256,
            pixel_log,
        );
        written.push(item);
    }
    Ok(written)
}

fn image_format_name(format: u8) -> &'static str {
    match format {
        1 => "DXT1",
        2 => "DXT5",
        4 => "RGBA8888",
        16 | 18 => "PNG-encoded",
        28 => "BGRA8888",
        _ => "unknown",
    }
}

fn format_decode_flags(flags: TextureDecodeFlags) -> String {
    let mut names = Vec::new();
    if flags.ycocg_conversion {
        names.push("ycocg");
    }
    if flags.normalize {
        names.push("normalize");
    }
    if flags.hemi_oct {
        names.push("hemi-oct");
    }
    if flags.invert {
        names.push("invert");
    }
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(",")
    }
}

fn dominant_pixel(image: &Image, channel: usize) -> Option<(u32, u32, [u8; 4])> {
    let mut best = None;
    let mut best_margin = 0_i16;
    for (index, pixel) in image.rgba.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let other_max = (0..3)
            .filter(|&other| other != channel)
            .map(|other| pixel[other])
            .max()?;
        let margin = i16::from(pixel[channel]) - i16::from(other_max);
        if margin > best_margin {
            best_margin = margin;
            let pixel_index = u32::try_from(index).ok()?;
            best = Some((
                pixel_index % image.width,
                pixel_index / image.width,
                [pixel[0], pixel[1], pixel[2], pixel[3]],
            ));
        }
    }
    best
}

fn validate_stat_icons_refresh_target(out: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let root_type = fs::symlink_metadata(out)?.file_type();
    if !root_type.is_dir() || root_type.is_symlink() {
        return Err(format!("{} is not a plain directory", out.display()).into());
    }

    let expected_root = BTreeSet::from(["provenance.json".to_owned(), "stat-icons".to_owned()]);
    let actual_root = fs::read_dir(out)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if !expected_root.is_subset(&actual_root) {
        return Err(format!("{} is missing provenance.json or stat-icons", out.display()).into());
    }
    let provenance_type = fs::symlink_metadata(out.join("provenance.json"))?.file_type();
    let icon_dir = out.join("stat-icons");
    let icon_dir_type = fs::symlink_metadata(&icon_dir)?.file_type();
    if !provenance_type.is_file()
        || provenance_type.is_symlink()
        || !icon_dir_type.is_dir()
        || icon_dir_type.is_symlink()
    {
        return Err("stat icon refresh target contains a non-regular path".into());
    }

    let expected_files = STAT_ICON_SOURCES
        .iter()
        .map(|source| format!("{}.png", source.name))
        .collect::<BTreeSet<_>>();
    let mut actual_files = BTreeSet::new();
    for entry in fs::read_dir(&icon_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() || file_type.is_symlink() {
            return Err(format!("{} is not a plain file", entry.path().display()).into());
        }
        actual_files.insert(entry.file_name().to_string_lossy().into_owned());
    }
    if actual_files != expected_files {
        return Err(format!(
            "{} does not contain exactly the 13 registered stat icons",
            icon_dir.display()
        )
        .into());
    }
    Ok(())
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

fn write_replace(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to replace a non-regular file",
        ));
    }
    let mut file = OpenOptions::new().write(true).truncate(true).open(path)?;
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
    map_overview: Option<&MapOverview>,
    map_source_bytes: Option<u64>,
    map_source_sha256: Option<&str>,
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
        "    \"map\": {},",
        map_source.map_or_else(|| "null".to_owned(), json_str)
    );
    let _ = writeln!(s, "    \"overview\": {},", overview_provenance_json(map_overview));
    let source_texture = match (map_source, map_source_bytes, map_source_sha256) {
        (Some(path), Some(bytes), Some(hash)) => format!(
            "{{ \"sourcePath\": {}, \"bytes\": {}, \"sha256\": {} }}",
            json_str(path), bytes, json_str(hash)
        ),
        _ => "null".to_owned(),
    };
    let _ = writeln!(s, "    \"mapSourceTexture\": {source_texture}");
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
            let source_path = w
                .source_path
                .as_ref()
                .map(|path| format!(", \"sourcePath\": {}", json_str(path)))
                .unwrap_or_default();
            format!(
                "    {{ \"path\": {}{}, \"bytes\": {}, \"sha256\": \"{}\" }}",
                json_str(&w.rel),
                source_path,
                w.bytes,
                w.sha256
            )
        })
        .collect();
    let _ = writeln!(s, "  \"files\": [\n{}\n  ]", files.join(",\n"));
    let _ = writeln!(s, "}}");
    s
}

fn overview_provenance_json(overview: Option<&MapOverview>) -> String {
    let Some(overview) = overview else {
        return "null".to_owned();
    };
    format!(
        "{{ \"sourcePath\": {}, \"style\": {}, \"materialLogicalPath\": {}, \"simpleMaterialLogicalPath\": {}, \"selectedMaterialCompiledPath\": {}, \"selectedTexturePath\": {}, \"textureReference\": {{ \"sourcePath\": {}, \"bytes\": {}, \"sha256\": {} }}, \"pos_x\": {}, \"pos_y\": {}, \"scale\": {} }}",
        json_str(&overview.source_path),
        json_str(overview.style.as_str()),
        json_str(&overview.material_logical_path),
        json_str(&overview.simple_material_logical_path),
        json_str(&overview.material_path),
        json_str(&overview.texture_path),
        json_str(&overview.texture_path),
        overview.source_texture_bytes,
        json_str(&overview.source_texture_sha256),
        overview.pos_x,
        overview.pos_y,
        overview.scale,
    )
}

fn map_provenance_json(
    build: &str,
    game: &Path,
    overview: Option<&MapOverview>,
    map_entry_override: Option<&str>,
    selected_texture: &str,
    selected_source_bytes: u64,
    selected_source_sha256: &str,
    source_image: &Image,
    written: &Written,
) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{{");
    let _ = writeln!(s, "  \"schema\": \"kohbor.dota-overview-map/1\",");
    let _ = writeln!(s, "  \"gameBuild\": {},", json_str(build));
    let vpk = relative_game_path(game, &game.join("game/dota/pak01_dir.vpk"))
        .unwrap_or_else(|_| "game/dota/pak01_dir.vpk".to_owned());
    let _ = writeln!(s, "  \"vpk\": {},", json_str(&vpk));
    let _ = writeln!(s, "  \"overview\": {},", overview_provenance_json(overview));
    let override_value = map_entry_override.map_or_else(|| "null".to_owned(), json_str);
    let _ = writeln!(s, "  \"mapEntryOverride\": {override_value},");
    let _ = writeln!(s, "  \"selectedTexture\": {{ \"sourcePath\": {}, \"bytes\": {}, \"sha256\": {}, \"dimensions\": [{}, {}] }},", json_str(selected_texture), selected_source_bytes, json_str(selected_source_sha256), source_image.width, source_image.height);
    let upsample_note = map_resampling_note(source_image);
    let _ = writeln!(s, "  \"map\": {{ \"path\": {}, \"bytes\": {}, \"sha256\": {}, \"sourcePath\": {}, \"dimensions\": [{}, {}], \"resampling\": {} }},", json_str(&written.rel), written.bytes, json_str(&written.sha256), json_str(selected_texture), MAP_SIZE, MAP_SIZE, json_str(upsample_note));
    let _ = writeln!(s, "  \"rights\": \"Valve game art. Rights review required before public release.\"");
    let _ = writeln!(s, "}}");
    s
}

fn stat_icons_provenance_json(build: &str, game: &Path, written: &[Written]) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{{");
    let _ = writeln!(s, "  \"schema\": \"kohbor.dota-stat-icons/1\",");
    let _ = writeln!(s, "  \"gameBuild\": {},", json_str(build));
    let _ = writeln!(s, "  \"source\": {{");
    let _ = writeln!(
        s,
        "    \"install\": \"installed Dota 2 on the owner's machine, read only, no download\","
    );
    let vpk = relative_game_path(game, &game.join("game/dota/pak01_dir.vpk"))
        .unwrap_or_else(|_| "game/dota/pak01_dir.vpk".to_owned());
    let _ = writeln!(s, "    \"vpk\": {}", json_str(&vpk));
    let _ = writeln!(s, "  }},");
    let _ = writeln!(
        s,
        "  \"tool\": {{ \"name\": \"kohbor-asset-export\", \"commit\": {} }},",
        json_str(&git_head())
    );
    let _ = writeln!(s, "  \"date\": {},", json_str(&utc_date()));
    let _ = writeln!(s, "  \"authority\": \"DEC-REPLAY-INTEGRATION item 3\",");
    let _ = writeln!(
        s,
        "  \"rights\": \"Valve game art. Rights review required before public release.\","
    );
    let total: u64 = written.iter().map(|item| item.bytes).sum();
    let _ = writeln!(s, "  \"totalBytes\": {total},");
    let files: Vec<String> = written
        .iter()
        .map(|item| {
            format!(
                "    {{ \"path\": {}, \"sourcePath\": {}, \"bytes\": {}, \"sha256\": \"{}\" }}",
                json_str(&item.rel),
                json_str(item.source_path.as_deref().unwrap_or("")),
                item.bytes,
                item.sha256
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResampleFilter {
    Nearest,
    Smooth,
}

/// Centre-crop to the target aspect ratio, area-average down, and select the
/// requested upsampling filter. Premultiplied alpha avoids dark fringes.
fn fit(image: &Image, width: u32, height: u32, filter: ResampleFilter) -> Image {
    let (sw, sh) = (image.width as f64, image.height as f64);
    let target = width as f64 / height as f64;
    let (cw, ch) = if sw / sh > target {
        (sh * target, sh)
    } else {
        (sw, sw / target)
    };
    let (cx, cy) = ((sw - cw) / 2.0, (sh - ch) / 2.0);
    let xs = weights(cx, cw, image.width, width, filter);
    let ys = weights(cy, ch, image.height, height, filter);
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

fn map_export_image(image: &Image) -> Image {
    if image.width == MAP_SIZE && image.height == MAP_SIZE {
        image.clone()
    } else {
        fit(image, MAP_SIZE, MAP_SIZE, ResampleFilter::Smooth)
    }
}

fn map_resampling_note(image: &Image) -> &'static str {
    if image.width == MAP_SIZE && image.height == MAP_SIZE {
        "source 1024 px, native-size export with no upscale or resampling"
    } else if image.width == 512 && image.height == 512 && MAP_SIZE == 1024 {
        "source 512 px, upscaled to 1024 with smooth bilinear filtering"
    } else if image.width < MAP_SIZE || image.height < MAP_SIZE {
        "source texture upscaled to 1024 with smooth bilinear filtering"
    } else {
        "source texture fitted to 1024 using smooth filtering"
    }
}

/// For each output pixel, the source pixels it covers and their overlap.
fn weights(
    start: f64,
    span: f64,
    source: u32,
    out: u32,
    filter: ResampleFilter,
) -> Vec<Vec<(u32, f64)>> {
    let scale = span / f64::from(out);
    (0..out)
        .map(|i| {
            let (a, b) = (
                start + f64::from(i) * scale,
                start + f64::from(i + 1) * scale,
            );
            let mut taps = Vec::new();
            if scale <= 1.0 {
                match filter {
                    ResampleFilter::Nearest => {
                        let s = ((a + b) / 2.0).floor().clamp(0.0, f64::from(source - 1)) as u32;
                        taps.push((s, 1.0));
                    }
                    ResampleFilter::Smooth => {
                        let center = (a + b) / 2.0 - 0.5;
                        let lower = center.floor();
                        let fraction = center - lower;
                        let first = lower.clamp(0.0, f64::from(source - 1)) as u32;
                        let second = (lower + 1.0).clamp(0.0, f64::from(source - 1)) as u32;
                        taps.push((first, 1.0 - fraction));
                        if second == first {
                            taps[0].1 += fraction;
                        } else {
                            taps.push((second, fraction));
                        }
                    }
                }
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
    fn overview_parser_reads_both_materials_and_official_calibration() {
        let parsed = parse_overview_text(
            r#"
            "Overview" {
                "material" "materials/overviews/dota.vmat"
                "simple_material" "materials/overviews/dota_minimal.vmat"
                "pos_x" "-9472"
                "pos_y" "9472"
                "scale" "18.5"
            }
            "Other" { "value" "ignored" }
            "#,
        )
        .expect("synthetic overview");
        assert_eq!(parsed.material, "materials/overviews/dota.vmat");
        assert_eq!(parsed.simple_material, "materials/overviews/dota_minimal.vmat");
        assert_eq!((parsed.pos_x, parsed.pos_y, parsed.scale), (-9472.0, 9472.0, 18.5));
        assert_eq!(overview_material(&parsed, MapStyle::Detailed), "materials/overviews/dota.vmat");
        assert_eq!(overview_material(&parsed, MapStyle::Simple), "materials/overviews/dota_minimal.vmat");
        assert_eq!(
            compiled_material_path(overview_material(&parsed, MapStyle::Detailed)).unwrap(),
            "materials/overviews/dota.vmat_c"
        );
        assert_eq!(
            compiled_material_path(overview_material(&parsed, MapStyle::Simple)).unwrap(),
            "materials/overviews/dota_minimal.vmat_c"
        );
    }

    #[test]
    fn detailed_map_style_is_default_and_both_styles_can_be_selected() {
        assert_eq!(parse_args(Vec::<String>::new().into_iter()).unwrap().map_style, MapStyle::Detailed);
        assert_eq!(
            parse_args(["--map-style", "simple"].map(str::to_owned).into_iter()).unwrap().map_style,
            MapStyle::Simple
        );
        assert_eq!(
            parse_args(["--map-style", "detailed"].map(str::to_owned).into_iter()).unwrap().map_style,
            MapStyle::Detailed
        );
        assert!(parse_args(["--map-style", "fast"].map(str::to_owned).into_iter()).is_err());
    }

    #[test]
    fn overview_provenance_records_both_logical_materials_and_selected_texture() {
        let overview = MapOverview {
            style: MapStyle::Detailed,
            source_path: "resource/overviews/dota.txt".into(),
            material_logical_path: "materials/overviews/dota.vmat".into(),
            simple_material_logical_path: "materials/overviews/dota_minimal.vmat".into(),
            material_path: "materials/overviews/dota.vmat_c".into(),
            texture_path: "materials/overviews/dota_tga_d8178876.vtex_c".into(),
            pos_x: -9472.0,
            pos_y: 9472.0,
            scale: 18.5,
            source_texture_bytes: 12,
            source_texture_sha256: "abc".into(),
        };
        let json = overview_provenance_json(Some(&overview));
        assert!(json.contains("\"style\": \"detailed\""));
        assert!(json.contains("materials/overviews/dota.vmat"));
        assert!(json.contains("materials/overviews/dota_minimal.vmat"));
        assert!(json.contains("materials/overviews/dota_tga_d8178876.vtex_c"));
    }

    #[test]
    fn map_entry_override_is_recorded_without_losing_dota_txt_calibration() {
        let args = parse_args(
            [
                "--map-only",
                "--out",
                "out",
                "--map-style",
                "detailed",
                "--map-entry",
                "materials/overviews/dota_psd_8b4a9409.vtex_c",
            ]
            .map(str::to_owned)
            .into_iter(),
        )
        .expect("explicit map entry with detailed overview selection");
        assert_eq!(args.map_entry.as_deref(), Some("materials/overviews/dota_psd_8b4a9409.vtex_c"));
        assert_eq!(args.map_style, MapStyle::Detailed);

        let overview = MapOverview {
            style: MapStyle::Detailed,
            source_path: "resource/overviews/dota.txt".into(),
            material_logical_path: "materials/overviews/dota.vmat".into(),
            simple_material_logical_path: "materials/overviews/dota_minimal.vmat".into(),
            material_path: "materials/overviews/dota.vmat_c".into(),
            texture_path: "materials/overviews/dota_tga_d8178876.vtex_c".into(),
            pos_x: -9472.0,
            pos_y: 9472.0,
            scale: 18.5,
            source_texture_bytes: 12,
            source_texture_sha256: "abc".into(),
        };
        let override_path = "materials/overviews/dota_psd_8b4a9409.vtex_c";
        let image = Image { width: 1024, height: 1024, rgba: Vec::new() };
        let written = Written {
            rel: "map.png".into(),
            source_path: Some(override_path.into()),
            bytes: 123,
            sha256: "def".into(),
        };
        let json = map_provenance_json(
            "6997",
            Path::new("G:/steam/steamapps/common/dota 2 beta"),
            Some(&overview),
            args.map_entry.as_deref(),
            override_path,
            456,
            "ghi",
            &image,
            &written,
        );
        assert!(json.contains("\"mapEntryOverride\": \"materials/overviews/dota_psd_8b4a9409.vtex_c\""));
        assert!(json.contains("\"pos_x\": -9472, \"pos_y\": 9472, \"scale\": 18.5"));
        assert!(json.contains("\"selectedTexture\": { \"sourcePath\": \"materials/overviews/dota_psd_8b4a9409.vtex_c\""));
        assert!(json.contains("\"resampling\": \"source 1024 px, native-size export with no upscale or resampling\""));
    }

    #[test]
    fn native_1024_map_export_preserves_pixels_and_reports_no_upscale() {
        let image = Image {
            width: MAP_SIZE,
            height: MAP_SIZE,
            rgba: vec![17, 23, 31, 255].repeat((MAP_SIZE * MAP_SIZE) as usize),
        };
        assert_eq!(map_export_image(&image), image);
        assert_eq!(map_resampling_note(&image), "source 1024 px, native-size export with no upscale or resampling");
    }

    #[test]
    fn compiled_material_texture_reference_extracts_a_vtex_c_path() {
        for (material, texture) in [
            ("dota.vmat_c", "dota_tga_d8178876.vtex_c"),
            ("dota_minimal.vmat_c", "dota_minimal_psd_f4e53729.vtex_c"),
        ] {
            let bytes = format!("\x01\0g_tColor\0materials/overviews/{texture}\0");
            let path = extract_vmat_texture_reference(bytes.as_bytes()).expect("one texture reference");
            assert_eq!(path, format!("materials/overviews/{texture}"), "{material}");
        }
        assert_eq!(
            extract_vmat_texture_reference(b"textures/dota_minimal.vtex\0").unwrap(),
            "textures/dota_minimal.vtex_c"
        );
    }

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
        let half = fit(&image, 2, 1, ResampleFilter::Nearest);
        assert_eq!(half.rgba, [128, 128, 128, 255, 128, 128, 128, 255]);
        // 4x2 to a square keeps the centre 2x2.
        let square = fit(&image, 2, 2, ResampleFilter::Nearest);
        assert_eq!((square.width, square.height), (2, 2));
        assert_eq!(&square.rgba[..4], &[255, 255, 255, 255]);
        // A transparent pixel does not darken its opaque neighbour's colour.
        let edge = Image {
            width: 2,
            height: 2,
            rgba: vec![200, 100, 50, 255, 0, 0, 0, 0, 200, 100, 50, 255, 0, 0, 0, 0],
        };
        assert_eq!(fit(&edge, 1, 1, ResampleFilter::Nearest).rgba, [200, 100, 50, 128]);
    }

    #[test]
    fn smooth_filter_interpolates_when_upscaling_the_detailed_overview() {
        let image = Image {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 255, 255, 255, 255, 255],
        };
        let upscaled = fit(&image, 4, 1, ResampleFilter::Smooth);
        assert_eq!((upscaled.width, upscaled.height), (4, 1));
        assert_eq!(upscaled.rgba[4], 64);
        assert_eq!(upscaled.rgba[8], 191);
        assert_eq!(upscaled.rgba[12], 255);
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
    fn unflagged_texture_png_output_is_byte_identical_for_supported_formats() {
        let samples = [
            (1, vec![0x00, 0xf8, 0, 0, 0, 0, 0, 0]),
            (
                2,
                vec![113, 113, 0, 0, 0, 0, 0, 0, 0x00, 0xf8, 0, 0, 0, 0, 0, 0],
            ),
            (4, vec![10, 20, 30, 40]),
            (28, vec![30, 20, 10, 40]),
        ];

        for (format, payload) in samples {
            let resource = make_export_test_vtex(format, &payload);
            let legacy = kohbor_game_assets::decode_vtex_c(&resource).expect("legacy decode");
            let display = decode_vtex_c_for_display(&resource).expect("display decode");

            assert_eq!(display.rgba, legacy.rgba, "format {format} pixels changed");
            assert_eq!(
                encode_png(&display).expect("display PNG"),
                encode_png(&legacy).expect("legacy PNG"),
                "format {format} PNG bytes changed"
            );
        }
    }

    fn make_export_test_vtex(format: u8, payload: &[u8]) -> Vec<u8> {
        let data_start = 44_usize;
        let metadata_len = data_start + 28;
        let mut bytes = vec![0_u8; metadata_len];
        bytes[0..4].copy_from_slice(&(metadata_len as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&0x0001_000c_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&8_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(b"DATA");
        bytes[20..24].copy_from_slice(&24_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&28_u32.to_le_bytes());
        bytes[data_start..data_start + 2].copy_from_slice(&1_u16.to_le_bytes());
        bytes[data_start + 20..data_start + 22].copy_from_slice(&1_u16.to_le_bytes());
        bytes[data_start + 22..data_start + 24].copy_from_slice(&1_u16.to_le_bytes());
        bytes[data_start + 24..data_start + 26].copy_from_slice(&1_u16.to_le_bytes());
        bytes[data_start + 26] = format;
        bytes[data_start + 27] = 1;
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn output_inside_the_game_folder_is_refused() {
        let game = Path::new("C:/Games/dota 2 beta");
        assert!(refuse_inside(Path::new("C:/Games/dota 2 beta/out"), game).is_err());
        assert!(refuse_inside(Path::new("C:/Games/DOTA 2 BETA/x.png"), game).is_err());
        assert!(refuse_inside(Path::new("F:/kohbor/apps/desktop/src/assets/dota"), game).is_ok());
    }
}
