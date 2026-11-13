use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kohbor_minimap_cv::{analyze, Team};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut directory = None;
    let mut team = None;
    let mut arguments = std::env::args_os().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == OsStr::new("--help") || argument == OsStr::new("-h") {
            print_usage();
            return Ok(());
        }
        if argument == OsStr::new("--team") {
            let value = arguments
                .next()
                .ok_or_else(|| "--team requires radiant or dire".to_owned())?;
            team = Some(parse_team(&value.to_string_lossy())?);
        } else if argument.to_string_lossy().starts_with('-') {
            return Err(format!("unknown option: {}", argument.to_string_lossy()));
        } else if directory.is_none() {
            directory = Some(PathBuf::from(argument));
        } else {
            return Err(format!(
                "unexpected argument: {}",
                argument.to_string_lossy()
            ));
        }
    }

    let Some(directory) = directory else {
        print_usage();
        return Err("a dump directory is required".to_owned());
    };
    let mut dumps = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| {
        format!(
            "cannot read dump directory {}: {error}",
            directory.display()
        )
    })? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                eprintln!("cannot inspect a directory entry: {error}");
                continue;
            }
        };
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        if entry
            .path()
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bgra"))
        {
            dumps.push(entry.path());
        }
    }
    dumps.sort_by(|left, right| left.file_stem().cmp(&right.file_stem()));

    for bgra_path in dumps {
        let sequence = bgra_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "unknown".to_owned());
        let sidecar_path = bgra_path.with_extension("txt");
        let result = read_pair(&bgra_path, &sidecar_path, team);
        match result {
            Ok((structural_ok, count)) => {
                println!("{}", format_row(&sequence, structural_ok, count))
            }
            Err(error) => {
                println!("{}", format_row(&sequence, false, None));
                eprintln!("{sequence}: {error}");
            }
        }
    }
    Ok(())
}

fn print_usage() {
    println!(
        "Usage: kohbor-minimap-replay DIR [--team radiant|dire]\nEach .bgra file is paired with a same-stem .txt sidecar containing width, height, and stride as named fields or one plain integer triplet. --team selects the counted enemy ring colour."
    );
}

fn format_row(sequence: &str, structural_ok: bool, count: Option<usize>) -> String {
    let count = count
        .map(|value| value.to_string())
        .unwrap_or_else(|| "—".to_owned());
    format!("{sequence} {structural_ok} {count}")
}

fn read_pair(
    bgra_path: &Path,
    sidecar_path: &Path,
    command_line_team: Option<Team>,
) -> Result<(bool, Option<usize>), String> {
    let sidecar_type = fs::symlink_metadata(sidecar_path)
        .map_err(|error| format!("cannot inspect {}: {error}", sidecar_path.display()))?
        .file_type();
    if !sidecar_type.is_file() {
        return Err(format!(
            "{} is not a regular sidecar file",
            sidecar_path.display()
        ));
    }
    let metadata_text = read_text(sidecar_path)?;
    let metadata = parse_metadata(&metadata_text)?;
    let bgra = read_bytes(bgra_path)?;
    match command_line_team {
        Some(team) => {
            let report = analyze(
                &bgra,
                metadata.width,
                metadata.height,
                metadata.stride,
                team,
            );
            if report.structural_ok {
                Ok((true, Some(report.enemy_blobs.len())))
            } else {
                Ok((false, None))
            }
        }
        None => {
            let radiant = analyze(
                &bgra,
                metadata.width,
                metadata.height,
                metadata.stride,
                Team::Radiant,
            );
            let dire = analyze(
                &bgra,
                metadata.width,
                metadata.height,
                metadata.stride,
                Team::Dire,
            );
            let structural_ok = radiant.structural_ok && dire.structural_ok;
            Ok((structural_ok, None))
        }
    }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let file_type = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
        .file_type();
    if !file_type.is_file() {
        return Err(format!("{} is not a regular dump file", path.display()));
    }
    let file =
        File::open(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
        .len();
    let byte_len = usize::try_from(file_len)
        .map_err(|_| format!("{} is too large for this process", path.display()))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(byte_len)
        .map_err(|error| format!("cannot reserve memory for {}: {error}", path.display()))?;
    file.take(file_len)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(bytes)
}

fn read_text(path: &Path) -> Result<String, String> {
    let file =
        File::open(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?
        .len();
    let text_len = usize::try_from(file_len)
        .map_err(|_| format!("{} is too large for this process", path.display()))?;
    let mut text = String::new();
    text.try_reserve_exact(text_len)
        .map_err(|error| format!("cannot reserve memory for {}: {error}", path.display()))?;
    file.take(file_len)
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(text)
}

struct DumpMetadata {
    width: u32,
    height: u32,
    stride: u32,
}

fn parse_metadata(text: &str) -> Result<DumpMetadata, String> {
    let mut width = None;
    let mut height = None;
    let mut stride = None;

    for raw_line in text.lines() {
        let line = raw_line
            .trim_start_matches('\u{feff}')
            .split('#')
            .next()
            .unwrap_or_default()
            .trim();
        if line.is_empty() {
            continue;
        }
        if let Some((plain_width, plain_height, plain_stride)) = parse_plain_dimensions(line) {
            set_parsed_dimension(&mut width, plain_width, "width")?;
            set_parsed_dimension(&mut height, plain_height, "height")?;
            set_parsed_dimension(&mut stride, plain_stride, "stride")?;
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .or_else(|| line.split_once(':'))
            .or_else(|| {
                let mut fields = line.split_whitespace();
                Some((fields.next()?, fields.next()?))
            })
            .ok_or_else(|| format!("malformed metadata line: {line}"))?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"');
        match key.as_str() {
            "width" => set_dimension(&mut width, value, "width")?,
            "height" => set_dimension(&mut height, value, "height")?,
            "stride" => set_dimension(&mut stride, value, "stride")?,
            _ => {}
        }
    }

    Ok(DumpMetadata {
        width: width.ok_or_else(|| "missing width metadata".to_owned())?,
        height: height.ok_or_else(|| "missing height metadata".to_owned())?,
        stride: stride.ok_or_else(|| "missing stride metadata".to_owned())?,
    })
}

fn parse_plain_dimensions(line: &str) -> Option<(u32, u32, u32)> {
    let mut fields = line.split_whitespace();
    let width = fields.next()?.parse().ok()?;
    let height = fields.next()?.parse().ok()?;
    let stride = fields.next()?.parse().ok()?;
    if fields.next().is_some() {
        return None;
    }
    Some((width, height, stride))
}

fn set_dimension(slot: &mut Option<u32>, value: &str, name: &str) -> Result<(), String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("invalid {name} metadata: {value}"))?;
    set_parsed_dimension(slot, parsed, name)
}

fn set_parsed_dimension(slot: &mut Option<u32>, parsed: u32, name: &str) -> Result<(), String> {
    if slot.is_some_and(|previous| previous != parsed) {
        return Err(format!("conflicting {name} metadata"));
    }
    *slot = Some(parsed);
    Ok(())
}

fn parse_team(value: &str) -> Result<Team, String> {
    if value.eq_ignore_ascii_case("radiant") {
        Ok(Team::Radiant)
    } else if value.eq_ignore_ascii_case("dire") {
        Ok(Team::Dire)
    } else {
        Err(format!("invalid team {value}; expected radiant or dire"))
    }
}

#[cfg(test)]
mod tests {
    use super::{format_row, parse_metadata, Team};

    #[test]
    fn parses_named_dimension_forms() {
        let metadata =
            parse_metadata("width=256\nheight: 256\nstride 1024\n").expect("valid named metadata");
        assert_eq!(metadata.width, 256);
        assert_eq!(metadata.height, 256);
        assert_eq!(metadata.stride, 1024);
    }

    #[test]
    fn parses_one_line_of_three_plain_dimensions() {
        let metadata = parse_metadata("292 292 1168\n").expect("valid positional metadata");
        assert_eq!(metadata.width, 292);
        assert_eq!(metadata.height, 292);
        assert_eq!(metadata.stride, 1168);
    }

    #[test]
    fn enemy_team_selection_is_parsed_from_cli_values() {
        assert_eq!(super::parse_team("dire"), Ok(Team::Dire));
        assert_eq!(super::parse_team("radiant"), Ok(Team::Radiant));
    }

    #[test]
    fn formats_unknown_team_count_as_em_dash() {
        assert_eq!(format_row("7", true, None), "7 true —");
        assert_eq!(format_row("7", true, Some(0)), "7 true 0");
        assert_eq!(format_row("7", false, None), "7 false —");
    }

    #[test]
    fn missing_dimensions_remain_an_error() {
        assert!(parse_metadata("width=256\nheight=256\n").is_err());
    }
}
