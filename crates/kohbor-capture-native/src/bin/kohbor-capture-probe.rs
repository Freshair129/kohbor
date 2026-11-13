use std::{
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

use kohbor_capture_native::{profile_rect, Grab, MinimapGrabber};

struct ProbeOptions {
    seconds: u64,
    dump_dir: Option<PathBuf>,
    dump_count: u64,
}

#[derive(Default)]
struct IntervalStats {
    calls: u64,
    frames: u64,
    timeouts: u64,
    access_lost: u64,
    failures: u64,
    elapsed_us: u128,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = parse_options(std::env::args_os().skip(1))?;
    if let Some(path) = options.dump_dir.as_deref() {
        if path_has_git_ancestor(path) {
            return Err("refusing --dump-dir inside a Git worktree".to_owned());
        }
        eprintln!(
            "WARNING: dump files contain screen content; keep them local and do not share them."
        );
    }

    let mut grabber = MinimapGrabber::open().map_err(|error| error.to_string())?;
    grabber.set_diagnostic_callback(Some(diagnostic));
    let output = grabber.output();
    let (rect, profile_id) = profile_rect(&output)
        .ok_or_else(|| "no minimap profile is known for this output geometry and DPI".to_owned())?;
    eprintln!(
        "output={}x{} dpi={} profile={profile_id}",
        output.width, output.height, output.dpi
    );

    let deadline = Instant::now() + Duration::from_secs(options.seconds);
    let mut next_report = Instant::now() + Duration::from_secs(1);
    let mut stats = IntervalStats::default();
    let mut bytes = Vec::new();
    let mut total_frames = 0u64;
    let mut dump_remaining = options.dump_count;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let timeout_ms = remaining.as_millis().min(15) as u32;
        let started = Instant::now();
        let result = grabber.grab(rect, timeout_ms, &mut bytes);
        stats.calls += 1;
        stats.elapsed_us += started.elapsed().as_micros();

        match result {
            Grab::Frame {
                width,
                height,
                stride,
                ..
            } => {
                stats.frames += 1;
                total_frames += 1;
                if dump_remaining > 0 {
                    let expected_len = usize::try_from(stride)
                        .ok()
                        .and_then(|row| row.checked_mul(height as usize))
                        .ok_or_else(|| "frame byte length overflows".to_owned())?;
                    if bytes.len() != expected_len {
                        return Err("grabber returned a buffer with an unexpected size".to_owned());
                    }
                    let directory = options
                        .dump_dir
                        .as_deref()
                        .ok_or_else(|| "--dump requires --dump-dir".to_owned())?;
                    write_dump(directory, total_frames, width, height, stride, &bytes)?;
                    dump_remaining -= 1;
                }
            }
            Grab::Timeout => stats.timeouts += 1,
            Grab::AccessLost => stats.access_lost += 1,
            Grab::Failed(error) => {
                stats.failures += 1;
                eprintln!("grab failed: {error}");
            }
        }

        let now = Instant::now();
        if now >= next_report {
            print_stats(&stats, total_frames, &grabber);
            stats = IntervalStats::default();
            next_report += Duration::from_secs(1);
        }
    }

    if stats.calls > 0 {
        print_stats(&stats, total_frames, &grabber);
    }
    if dump_remaining > 0 {
        eprintln!("dump incomplete: {dump_remaining} requested frame(s) were not captured");
    }
    Ok(())
}

fn parse_options(args: impl IntoIterator<Item = OsString>) -> Result<ProbeOptions, String> {
    let mut args = args.into_iter();
    let mut seconds = None;
    let mut dump_dir = None;
    let mut dump_count = None;
    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--seconds" => {
                if seconds.is_some() {
                    return Err("--seconds may be provided once".to_owned());
                }
                let value = args
                    .next()
                    .ok_or_else(|| "--seconds requires a value".to_owned())?;
                seconds = Some(parse_positive(&value, "--seconds")?);
            }
            "--dump-dir" => {
                if dump_dir.is_some() {
                    return Err("--dump-dir may be provided once".to_owned());
                }
                let value = args
                    .next()
                    .ok_or_else(|| "--dump-dir requires a path".to_owned())?;
                dump_dir = Some(PathBuf::from(value));
            }
            "--dump" => {
                if dump_count.is_some() {
                    return Err("--dump may be provided once".to_owned());
                }
                let value = args
                    .next()
                    .ok_or_else(|| "--dump requires a count".to_owned())?;
                dump_count = Some(parse_positive(&value, "--dump")?);
            }
            unknown => return Err(format!("unknown option: {unknown}")),
        }
    }

    if dump_dir.is_some() != dump_count.is_some() {
        return Err("--dump-dir and --dump must be supplied together".to_owned());
    }
    let seconds =
        seconds.ok_or_else(|| "usage: --seconds N [--dump-dir PATH --dump N]".to_owned())?;
    Ok(ProbeOptions {
        seconds,
        dump_dir,
        dump_count: dump_count.unwrap_or(0),
    })
}

fn parse_positive(value: &OsString, option: &str) -> Result<u64, String> {
    let parsed = value
        .to_string_lossy()
        .parse::<u64>()
        .map_err(|_| format!("{option} requires a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{option} requires a positive integer"));
    }
    Ok(parsed)
}

fn print_stats(stats: &IntervalStats, total_frames: u64, grabber: &MinimapGrabber) {
    let mean_ms = if stats.calls == 0 {
        "unknown".to_owned()
    } else {
        format!(
            "{:.3}",
            stats.elapsed_us as f64 / stats.calls as f64 / 1000.0
        )
    };
    let gpu_bytes = grabber
        .gpu_local_bytes()
        .map(|bytes| bytes.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    println!(
        "frames={} timeouts={} access_lost={} failures={} total_frames={} mean_grab_ms={} gpu_local_bytes={}",
        stats.frames, stats.timeouts, stats.access_lost, stats.failures, total_frames, mean_ms, gpu_bytes
    );
}

fn diagnostic(message: &str) {
    eprintln!("capture diagnostic: {message}");
}

fn dump_header(width: u32, height: u32, stride: u32) -> String {
    format!("{width} {height} {stride}\n")
}

fn write_dump(
    directory: &Path,
    sequence: u64,
    width: u32,
    height: u32,
    stride: u32,
    bytes: &[u8],
) -> Result<(), String> {
    let directory = safe_dump_directory(directory)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create dump directory: {error}"))?;
    if path_has_git_ancestor(&directory) {
        return Err("refusing to write capture data inside a Git worktree".to_owned());
    }

    let pixels = directory.join(format!("{sequence}.bgra"));
    let header = directory.join(format!("{sequence}.txt"));
    fs::write(pixels, bytes).map_err(|error| format!("could not write BGRA crop: {error}"))?;
    fs::write(header, dump_header(width, height, stride))
        .map_err(|error| format!("could not write crop header: {error}"))?;
    Ok(())
}

fn path_has_git_ancestor(path: &Path) -> bool {
    let Ok(path) = canonical_candidate(path) else {
        return true;
    };
    let mut ancestor = Some(path.as_path());
    while let Some(current) = ancestor {
        match fs::symlink_metadata(current.join(".git")) {
            Ok(_) => return true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return true,
        }
        ancestor = current.parent();
    }
    false
}

fn safe_dump_directory(path: &Path) -> Result<PathBuf, String> {
    if path_has_git_ancestor(path) {
        return Err("refusing dump path inside a Git worktree or unresolved path".to_owned());
    }
    canonical_candidate(path)
}

fn canonical_candidate(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("could not resolve current directory: {error}"))?
            .join(path)
    };
    let normalized = normalize_path(&absolute);
    let mut existing = normalized.clone();
    let mut missing = Vec::new();

    loop {
        match fs::symlink_metadata(&existing) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    return Err("dump path ancestor is not a directory".to_owned());
                }
                let mut canonical = fs::canonicalize(&existing)
                    .map_err(|error| format!("could not resolve dump path: {error}"))?;
                for component in missing.into_iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let Some(name) = existing.file_name() else {
                    return Err("dump path has no existing ancestor".to_owned());
                };
                missing.push(name.to_os_string());
                if !existing.pop() {
                    return Err("dump path has no existing ancestor".to_owned());
                }
            }
            Err(error) => return Err(format!("could not inspect dump path: {error}")),
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn dump_path_inside_a_git_worktree_is_refused() {
        let current = std::env::current_dir().unwrap();
        assert!(path_has_git_ancestor(&current.join("captures")));
        assert!(!path_has_git_ancestor(Path::new(
            "F:/capture-probe-outside-repository"
        )));
    }

    #[test]
    fn dump_header_contains_width_height_and_tight_stride() {
        assert_eq!(dump_header(292, 292, 1168), "292 292 1168\n");
    }
}
