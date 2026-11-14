use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn git_output(repository_root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository_root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|value| value.trim().to_owned())
}

fn git_sha(repository_root: &Path) -> String {
    let Some(sha) = git_output(repository_root, &["rev-parse", "--short=9", "HEAD"]) else {
        return String::new();
    };
    if sha.is_empty() {
        return String::new();
    }

    let Some(status) = git_output(repository_root, &["status", "--porcelain"]) else {
        return String::new();
    };
    if status.is_empty() {
        sha
    } else {
        format!("{sha}-dirty")
    }
}

fn git_metadata_path(repository_root: &Path, name: &str) -> PathBuf {
    git_output(repository_root, &["rev-parse", "--git-path", name])
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                repository_root.join(path)
            }
        })
        .unwrap_or_else(|| repository_root.join(".git").join(name))
}

fn utc_timestamp(seconds_since_epoch: i64) -> Option<String> {
    let days = seconds_since_epoch.div_euclid(86_400);
    let seconds_of_day = seconds_since_epoch.rem_euclid(86_400);

    // Convert days since 1970-01-01 to a Gregorian date.
    let shifted_days = days + 719_468;
    let era = shifted_days / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };

    if !(0..=9_999).contains(&year) {
        return None;
    }

    let minutes_of_day = seconds_of_day / 60;
    Some(format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minutes_of_day / 60,
        minutes_of_day % 60
    ))
}

fn build_date() -> String {
    let seconds_since_epoch = match env::var_os("SOURCE_DATE_EPOCH") {
        Some(value) => {
            let Some(seconds) = value.to_str().and_then(|value| value.parse::<u64>().ok()) else {
                return String::new();
            };
            seconds
        }
        None => match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(_) => return String::new(),
        },
    };

    let Ok(seconds_since_epoch) = i64::try_from(seconds_since_epoch) else {
        return String::new();
    };
    utc_timestamp(seconds_since_epoch).unwrap_or_default()
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository_root = manifest_dir.join("../../..");
    let repository_root = repository_root.canonicalize().unwrap_or(repository_root);

    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    // Release builds must never embed a stale commit or dirty marker: a ref update or an unstaged
    // edit is not visible through the git metadata paths below, so a release build always re-runs
    // this script (a path that never exists is always considered changed). Debug builds keep the
    // cheaper metadata triggers.
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        println!("cargo:rerun-if-changed=.kohbor-always-rerun-release");
    }
    println!(
        "cargo:rerun-if-changed={}",
        repository_root.join(".git").display()
    );
    for name in ["HEAD", "index"] {
        println!(
            "cargo:rerun-if-changed={}",
            git_metadata_path(&repository_root, name).display()
        );
    }

    println!(
        "cargo:rustc-env=KOHBOR_GIT_SHA={}",
        git_sha(&repository_root)
    );
    println!("cargo:rustc-env=KOHBOR_BUILD_DATE={}", build_date());

    tauri_build::build();
}
