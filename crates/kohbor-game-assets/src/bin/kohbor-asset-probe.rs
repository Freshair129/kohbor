use flate2::write::ZlibEncoder;
use flate2::Compression;
use kohbor_game_assets::{decode_vtex_c, hero_portrait_path, Vpk};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Request {
    hero: String,
    output: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortraitDir {
    Root,
    Selection,
    Icons,
}

impl PortraitDir {
    fn candidate_index(self) -> usize {
        match self {
            Self::Root => 0,
            Self::Selection => 1,
            Self::Icons => 2,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("asset probe failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (game_dir, portrait_dir, mut requests) = parse_args(std::env::args_os().skip(1))?;
    let resolved_game_dir = fs::canonicalize(&game_dir)?;
    for request in &mut requests {
        request.output = output_outside_protected_directories(&request.output, &resolved_game_dir)?;
    }
    for (index, request) in requests.iter().enumerate() {
        if requests[..index]
            .iter()
            .any(|other| other.output == request.output)
        {
            return Err("each portrait needs a distinct output path".into());
        }
    }
    let directory_vpk = game_dir.join("game/dota/pak01_dir.vpk");
    let open_started = Instant::now();
    let vpk = Vpk::open(&directory_vpk)?;
    let open_ms = open_started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "open_vpk_ms={open_ms:.3} tree_bytes={} indexed_portraits={}",
        vpk.directory_tree_bytes(),
        vpk.indexed_portrait_count()
    );
    if let Some(bytes) = peak_working_set_bytes() {
        println!("peak_working_set_bytes_after_open={bytes}");
    }

    for request in requests {
        let request_started = Instant::now();
        let find_started = Instant::now();
        let entry = hero_portrait_path(&request.hero)
            .get(portrait_dir.candidate_index())
            .and_then(|candidate| vpk.find(candidate));
        let find_ms = find_started.elapsed().as_secs_f64() * 1000.0;
        let entry =
            entry.ok_or_else(|| format!("no installed portrait found for {}", request.hero))?;

        let read_started = Instant::now();
        let bytes = vpk.read(&entry)?;
        let read_ms = read_started.elapsed().as_secs_f64() * 1000.0;

        let decode_started = Instant::now();
        let image = decode_vtex_c(&bytes)?;
        let decode_ms = decode_started.elapsed().as_secs_f64() * 1000.0;

        let png_started = Instant::now();
        write_png(&request.output, &image)?;
        let png_ms = png_started.elapsed().as_secs_f64() * 1000.0;
        let png_bytes = fs::metadata(&request.output)?.len();
        println!(
            "hero={} path={} dimensions={}x{} entry_bytes={} source_bytes={} rgba_bytes={} png_bytes={} find_ms={:.3} read_ms={:.3} decode_ms={:.3} png_ms={:.3} total_ms={:.3} out={}",
            request.hero,
            entry.path(),
            image.width,
            image.height,
            entry.encoded_len(),
            bytes.len(),
            image.rgba.len(),
            png_bytes,
            find_ms,
            read_ms,
            decode_ms,
            png_ms,
            request_started.elapsed().as_secs_f64() * 1000.0,
            request.output.display(),
        );
    }
    match peak_working_set_bytes() {
        Some(bytes) => println!("peak_working_set_bytes={bytes}"),
        None => println!("peak_working_set_bytes=unavailable"),
    }
    Ok(())
}

fn parse_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(PathBuf, PortraitDir, Vec<Request>), Box<dyn std::error::Error>> {
    let mut args = args.into_iter();
    let mut game_dir = None;
    let mut portrait_dir = None;
    let mut hero: Option<String> = None;
    let mut requests = Vec::new();
    while let Some(flag) = args.next() {
        match flag.to_str() {
            Some("--portrait-dir") => {
                if portrait_dir.is_some() {
                    return Err("--portrait-dir may only be provided once".into());
                }
                portrait_dir = Some(
                    match args.next().as_deref().and_then(|value| value.to_str()) {
                        Some("root") => PortraitDir::Root,
                        Some("selection") => PortraitDir::Selection,
                        Some("icons") => PortraitDir::Icons,
                        _ => return Err("--portrait-dir requires root, selection, or icons".into()),
                    },
                );
            }
            Some("--game-dir") => {
                if game_dir.is_some() {
                    return Err("--game-dir may only be provided once".into());
                }
                game_dir = args.next().map(PathBuf::from);
                if game_dir.is_none() {
                    return Err("--game-dir requires a path".into());
                }
            }
            Some("--hero") => {
                if hero.is_some() {
                    return Err("provide --out before another --hero".into());
                }
                hero = Some(
                    args.next()
                        .ok_or("--hero requires an ID")?
                        .into_string()
                        .map_err(|_| "hero ID is not valid Unicode")?,
                );
            }
            Some("--out") => {
                let current_hero = hero.take().ok_or("--out must follow --hero")?;
                let output = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or("--out requires a path")?;
                requests.push(Request {
                    hero: current_hero,
                    output,
                });
            }
            Some("--help") | Some("-h") => {
                println!("Usage: kohbor-asset-probe --game-dir <Dota install root> [--portrait-dir root|selection|icons] --hero <id> --out <PNG path> [--hero <id> --out <PNG path> ...]");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument {:?}", flag).into()),
        }
    }
    if hero.is_some() {
        return Err("--hero requires a matching --out".into());
    }
    let game_dir = game_dir.ok_or("--game-dir is required")?;
    if requests.is_empty() {
        return Err("at least one --hero/--out pair is required".into());
    }
    Ok((
        game_dir,
        portrait_dir.unwrap_or(PortraitDir::Root),
        requests,
    ))
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy().to_lowercase();
        let root = root.to_string_lossy().to_lowercase();
        path == root || path.starts_with(&(root.trim_end_matches('\\').to_owned() + "\\"))
    }
    #[cfg(not(windows))]
    path.starts_with(root)
}

fn output_outside_git_worktree(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let file_name = path.file_name().ok_or("--out must include a file name")?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let canonical_parent = fs::canonicalize(parent)?;
    let requested = canonical_parent.join(file_name);
    // Reject all existing directory entries, including dangling symlinks.
    match fs::symlink_metadata(&requested) {
        Ok(_) => {
            return Err(
                io::Error::new(io::ErrorKind::AlreadyExists, "output already exists").into(),
            )
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    for ancestor in canonical_parent.ancestors() {
        match fs::symlink_metadata(ancestor.join(".git")) {
            Ok(_) => {
                return Err(format!(
                    "refusing output inside Git worktree: {}",
                    requested.display()
                )
                .into())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(requested)
}

fn output_outside_protected_directories(
    path: &Path,
    resolved_game_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let resolved = output_outside_git_worktree(path)?;
    let parent = resolved.parent().ok_or("output has no parent")?;
    if path_is_within(parent, resolved_game_dir) {
        return Err("refusing output inside the game installation".into());
    }
    Ok(resolved)
}

fn write_png(path: &Path, image: &kohbor_game_assets::Image) -> io::Result<()> {
    let row_bytes = usize::try_from(image.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PNG row size overflow"))?;
    let expected = row_bytes
        .checked_mul(image.height as usize)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PNG size overflow"))?;
    if expected != image.rgba.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "RGBA byte count does not match image dimensions",
        ));
    }
    let filtered_row = row_bytes
        .checked_add(1)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PNG row size overflow"))?;
    let filtered_len = filtered_row
        .checked_mul(image.height as usize)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "PNG size overflow"))?;
    let mut scanlines = Vec::with_capacity(filtered_len);
    for row in image.rgba.chunks_exact(row_bytes) {
        scanlines.push(0);
        scanlines.extend_from_slice(row);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&scanlines)?;
    let compressed = encoder.finish()?;

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&image.width.to_be_bytes());
    ihdr.extend_from_slice(&image.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    push_chunk(&mut png, b"IHDR", &ihdr)?;
    push_chunk(&mut png, b"IDAT", &compressed)?;
    push_chunk(&mut png, b"IEND", &[])?;
    // This atomic creation, not the preflight check, prevents a racing writer
    // from having an existing file truncated or replaced.
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&png)?;
    file.flush()?;
    Ok(())
}

fn push_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> io::Result<()> {
    let length = u32::try_from(data.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "PNG chunk is too large"))?;
    png.extend_from_slice(&length.to_be_bytes());
    let crc_start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let crc = png_crc32(&png[crc_start..]);
    png.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

fn png_crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(windows)]
fn peak_working_set_bytes() -> Option<usize> {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Default)]
    #[allow(dead_code)]
    struct ProcessMemoryCountersEx {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
        private_usage: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        #[link_name = "GetCurrentProcess"]
        fn get_current_process() -> *mut c_void;
    }
    #[link(name = "psapi")]
    extern "system" {
        #[link_name = "GetProcessMemoryInfo"]
        fn get_process_memory_info(
            process: *mut c_void,
            counters: *mut ProcessMemoryCountersEx,
            size: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCountersEx {
        cb: u32::try_from(std::mem::size_of::<ProcessMemoryCountersEx>()).ok()?,
        ..Default::default()
    };
    let counters_size = counters.cb;
    let result =
        unsafe { get_process_memory_info(get_current_process(), &mut counters, counters_size) };
    if result == 0 {
        None
    } else {
        Some(counters.peak_working_set_size)
    }
}

#[cfg(not(windows))]
fn peak_working_set_bytes() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::{output_outside_git_worktree, parse_args};
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos();
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "kohbor-probe-test-{}-{nonce}-{unique}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("owned synthetic directory");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn output_guard_refuses_existing_file() {
        let temp = TempDir::new();
        let path = temp.0.join("existing.png");
        fs::write(&path, b"synthetic sentinel").expect("synthetic existing output");
        assert!(output_outside_git_worktree(&path).is_err());
        assert!(fs::read(path).expect("sentinel read") == b"synthetic sentinel");
    }

    #[test]
    fn output_create_new_preserves_file_created_after_preflight() {
        let temp = TempDir::new();
        let path =
            output_outside_git_worktree(&temp.0.join("raced.png")).expect("unused output path");
        fs::write(&path, b"synthetic sentinel").expect("simulate another writer winning the race");
        let image = kohbor_game_assets::Image {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        };
        let error =
            super::write_png(&path, &image).expect_err("existing output must not be truncated");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(fs::read(path).expect("sentinel read") == b"synthetic sentinel");
    }

    #[test]
    fn output_guard_rejects_resolved_game_and_git_parents() {
        let temp = TempDir::new();
        let game = temp.0.join("synthetic-game");
        let repo = temp.0.join("synthetic-repo");
        fs::create_dir_all(game.join("nested")).expect("synthetic game directories");
        fs::create_dir_all(repo.join("nested")).expect("synthetic git directories");
        fs::write(repo.join(".git"), b"synthetic git marker").expect("synthetic git marker");
        let game = fs::canonicalize(game).expect("resolved synthetic game");
        assert!(super::output_outside_protected_directories(
            &game.join("nested/../image.png"),
            &game
        )
        .is_err());
        assert!(super::output_outside_protected_directories(
            &repo.join("nested/../image.png"),
            &game
        )
        .is_err());
        assert!(
            super::output_outside_protected_directories(&temp.0.join("unused.png"), &game).is_ok()
        );
    }

    #[test]
    fn output_png_is_created_once_without_replacing_it() {
        let temp = TempDir::new();
        let path = temp.0.join("once.png");
        let image = kohbor_game_assets::Image {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 255],
        };
        super::write_png(&path, &image).expect("new synthetic PNG");
        let before = fs::read(&path).expect("synthetic PNG read");
        assert!(before.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            super::write_png(&path, &image)
                .expect_err("second creation refused")
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert!(fs::read(path).expect("unchanged synthetic PNG") == before);
    }

    #[test]
    fn probe_refuses_output_under_the_repository_worktree() {
        let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("probe-test.png");
        assert!(output_outside_git_worktree(&output).is_err());
    }

    #[test]
    fn probe_accepts_selection_directory_and_two_hero_requests() {
        let args = [
            "--game-dir",
            "synthetic-install",
            "--portrait-dir",
            "selection",
            "--hero",
            "lion",
            "--out",
            "lion.png",
            "--hero",
            "axe",
            "--out",
            "axe.png",
        ];
        let (_, directory, requests) =
            parse_args(args.into_iter().map(OsString::from)).expect("synthetic arguments");
        assert_eq!(directory, super::PortraitDir::Selection);
        assert_eq!(requests.len(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn game_output_boundary_is_case_insensitive_and_component_bounded() {
        use std::path::Path;
        let root = Path::new(r"F:\synthetic-game");
        assert!(super::path_is_within(
            Path::new(r"f:\SYNTHETIC-game\portrait.png"),
            root
        ));
        assert!(!super::path_is_within(
            Path::new(r"F:\synthetic-game-other\portrait.png"),
            root
        ));
    }
}
