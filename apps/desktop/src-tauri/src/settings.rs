//! Locale settings commands and the settings-patch 1.0.0 contract.

use crate::{atomic, intake};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

const MAX_REVISION: u64 = 9_007_199_254_740_991;
const SOURCE_INVALID: &str = "SOURCE_INVALID";
const SOURCE_UNAVAILABLE: &str = "SOURCE_UNAVAILABLE";
const VERSION_UNSUPPORTED: &str = "VERSION_UNSUPPORTED";
const WRITE_FAILED: &str = "GSI_CONFIG_WRITE_FAILED";

pub(crate) fn schema_integer(value: &Value) -> Option<u64> {
    if let Some(integer) = value.as_u64() {
        return (1..=MAX_REVISION).contains(&integer).then_some(integer);
    }

    // JSON Schema considers a number with no fractional part to be an integer,
    // including a JSON representation such as 1.0.
    let number = value.as_f64()?;
    (number.is_finite() && number.fract() == 0.0 && (1.0..=MAX_REVISION as f64).contains(&number))
        .then_some(number as u64)
}

/// Advances a persisted settings revision using the contract's integer
/// semantics and maximum safe JSON integer.
pub(crate) fn checked_next_revision(revision: &Value) -> Result<u64, &'static str> {
    let revision = schema_integer(revision).ok_or(SOURCE_INVALID)?;
    revision
        .checked_add(1)
        .filter(|next| *next <= MAX_REVISION)
        .ok_or(VERSION_UNSUPPORTED)
}

fn has_exact_keys(object: &Map<String, Value>, expected: &[&str]) -> bool {
    object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
}

fn valid_request_id(value: &Value) -> bool {
    let Some(value) = value.as_str() else {
        return false;
    };
    (8..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn validate_request(request: &Value) -> Result<(u64, &str), &'static str> {
    let Some(object) = request.as_object() else {
        return Err(SOURCE_INVALID);
    };
    if !has_exact_keys(
        object,
        &["schema_version", "request_id", "base_revision", "patch"],
    ) {
        return Err(SOURCE_INVALID);
    }
    if object.get("schema_version").and_then(Value::as_str) != Some("1.0.0") {
        return Err(VERSION_UNSUPPORTED);
    }
    if !valid_request_id(&object["request_id"]) {
        return Err(SOURCE_INVALID);
    }
    let Some(base_revision) = schema_integer(&object["base_revision"]) else {
        return Err(SOURCE_INVALID);
    };

    let Some(patch) = object["patch"].as_object() else {
        return Err(SOURCE_INVALID);
    };
    if !has_exact_keys(patch, &["locale"]) {
        return Err(SOURCE_INVALID);
    }
    match patch["locale"].as_str() {
        Some(locale @ ("th" | "en")) => Ok((base_revision, locale)),
        _ => Err(SOURCE_INVALID),
    }
}

fn current_settings(value: &Value) -> Result<(u64, &str), &'static str> {
    let Some(object) = value.as_object() else {
        return Err(SOURCE_INVALID);
    };
    if object.get("schema_version").and_then(Value::as_str) != Some("1.0.0") {
        return Err(VERSION_UNSUPPORTED);
    }
    let Some(revision) = object.get("revision").and_then(schema_integer) else {
        return Err(SOURCE_INVALID);
    };
    if revision == 0 {
        return Err(SOURCE_INVALID);
    }
    match object.get("locale").and_then(Value::as_str) {
        Some(locale @ ("th" | "en")) => Ok((revision, locale)),
        _ => Err(SOURCE_INVALID),
    }
}

fn valid_port(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    value
        .as_u64()
        .map(|port| (3000..=3010).contains(&port))
        .or_else(|| {
            let number = value.as_f64()?;
            (number.is_finite() && number.fract() == 0.0 && (3000.0..=3010.0).contains(&number))
                .then_some(true)
        })
        .unwrap_or(false)
}

fn valid_game_dir(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(path) => (3..=1024).contains(&path.chars().count()),
        _ => false,
    }
}

fn valid_config_hash(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(hash) => {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }
        _ => false,
    }
}

fn valid_token_ref(value: &Value) -> bool {
    value.is_null() || value.as_str() == Some("dpapi:gsi-token")
}

/// Checks the complete persisted contract before allowing a read-modify-write.
/// This prevents a locale change from making an invalid or partially recoverable
/// settings file look healthy or from silently normalizing unrelated data.
pub(crate) fn validate_persisted_settings(value: &Value) -> Result<(u64, &str), &'static str> {
    let Some(object) = value.as_object() else {
        return Err(SOURCE_INVALID);
    };
    if !has_exact_keys(object, &["schema_version", "revision", "locale", "gsi"]) {
        return Err(SOURCE_INVALID);
    }
    if object.get("schema_version").and_then(Value::as_str) != Some("1.0.0") {
        return Err(VERSION_UNSUPPORTED);
    }
    let Some(revision) = object.get("revision").and_then(schema_integer) else {
        return Err(SOURCE_INVALID);
    };
    let locale = match object.get("locale").and_then(Value::as_str) {
        Some(locale @ ("th" | "en")) => locale,
        _ => return Err(SOURCE_INVALID),
    };
    let Some(gsi) = object.get("gsi").and_then(Value::as_object) else {
        return Err(SOURCE_INVALID);
    };
    if !has_exact_keys(gsi, &["game_dir", "port", "config_sha256", "token_ref"])
        || !valid_game_dir(&gsi["game_dir"])
        || !valid_port(&gsi["port"])
        || !valid_config_hash(&gsi["config_sha256"])
        || !valid_token_ref(&gsi["token_ref"])
    {
        return Err(SOURCE_INVALID);
    }
    if gsi["config_sha256"].is_string()
        && (!gsi["game_dir"].is_string()
            || !gsi["port"].is_number()
            || gsi["port"].is_null()
            || !gsi["token_ref"].is_string())
    {
        return Err(SOURCE_INVALID);
    }
    Ok((revision, locale))
}

pub(crate) fn settings_for_patch(data_dir: &Path) -> Result<Value, String> {
    let path = intake::settings_path(data_dir);
    match fs::read(path) {
        Ok(bytes) => kohbor_gsi::strict_json::parse(&bytes).map_err(|_| SOURCE_INVALID.to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(intake::default_settings())
        }
        Err(_) => Err(SOURCE_UNAVAILABLE.to_owned()),
    }
}

fn response(value: &Value) -> Result<Value, &'static str> {
    let (revision, locale) = current_settings(value)?;
    Ok(json!({"revision": revision, "locale": locale}))
}

/// Returns the persisted settings revision and locale. A missing file uses
/// defaults; unreadable, malformed, or contract-invalid files remain errors.
pub fn get(data_dir: &Path) -> Result<Value, String> {
    let settings = settings_for_patch(data_dir)?;
    validate_persisted_settings(&settings).map_err(|_| SOURCE_INVALID.to_owned())?;
    response(&settings).map_err(str::to_owned)
}

/// Validates and atomically applies the locale-only settings patch.
pub fn patch(data_dir: &Path, request: &Value) -> Result<Value, String> {
    let (base_revision, locale) = validate_request(request).map_err(str::to_owned)?;
    let mut settings = settings_for_patch(data_dir)?;
    let (current_revision, _) = validate_persisted_settings(&settings).map_err(str::to_owned)?;
    if base_revision != current_revision {
        // No settings-specific conflict code exists in error-catalog.json.
        return Err(VERSION_UNSUPPORTED.to_owned());
    }
    let next_revision = checked_next_revision(&settings["revision"]).map_err(str::to_owned)?;

    let Some(object) = settings.as_object_mut() else {
        return Err(SOURCE_INVALID.to_owned());
    };
    object.insert("revision".into(), json!(next_revision));
    object.insert("locale".into(), json!(locale));

    let bytes = serde_json::to_vec_pretty(&settings).map_err(|_| WRITE_FAILED.to_owned())?;
    atomic::write(&intake::settings_path(data_dir), &bytes).map_err(|_| WRITE_FAILED.to_owned())?;

    response(&settings).map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn data_dir() -> std::path::PathBuf {
        let index = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "kohbor-settings-test-{}-{index}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn request(base_revision: Value, locale: &str) -> Value {
        json!({
            "schema_version": "1.0.0",
            "request_id": "settings-test_01",
            "base_revision": base_revision,
            "patch": {"locale": locale}
        })
    }

    fn write_settings(dir: &Path, settings: &Value) -> Vec<u8> {
        let bytes = serde_json::to_vec(settings).unwrap();
        fs::write(intake::settings_path(dir), &bytes).unwrap();
        bytes
    }

    fn settings_with_gsi() -> Value {
        json!({
            "schema_version": "1.0.0",
            "revision": 7,
            "locale": "th",
            "gsi": {
                "game_dir": "C:\\Games\\Dota 2 beta",
                "port": 3004,
                "config_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "token_ref": "dpapi:gsi-token"
            }
        })
    }

    #[test]
    fn missing_files_keep_defaults_and_corrupt_files_are_rejected() {
        let dir = data_dir();
        assert_eq!(get(&dir).unwrap(), json!({"revision": 1, "locale": "th"}));
        let path = intake::settings_path(&dir);
        let corrupt = b"{not json";
        fs::write(&path, corrupt).unwrap();
        assert_eq!(get(&dir), Err(SOURCE_INVALID.to_owned()));
        assert_eq!(fs::read(&path).unwrap(), corrupt);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn patch_bumps_revision_and_preserves_the_entire_gsi_block() {
        let dir = data_dir();
        let original = settings_with_gsi();
        write_settings(&dir, &original);

        assert_eq!(
            patch(&dir, &request(json!(7), "en")).unwrap(),
            json!({"revision": 8, "locale": "en"})
        );
        let saved: Value =
            serde_json::from_slice(&fs::read(intake::settings_path(&dir)).unwrap()).unwrap();
        let mut expected = original;
        expected["revision"] = json!(8);
        expected["locale"] = json!("en");
        assert_eq!(saved, expected);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn schema_integer_accepts_integral_number_notation() {
        let dir = data_dir();
        assert_eq!(
            patch(&dir, &request(json!(1.0), "en")).unwrap(),
            json!({"revision": 2, "locale": "en"})
        );
        let saved: Value =
            serde_json::from_slice(&fs::read(intake::settings_path(&dir)).unwrap()).unwrap();
        assert_eq!(saved["gsi"], intake::default_settings()["gsi"]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn patch_rejects_corrupt_or_schema_invalid_files_without_replacing_them() {
        let dir = data_dir();
        let path = intake::settings_path(&dir);
        let invalid_files = [
            br#"{not json"#.as_slice(),
            br#"{"schema_version":"1.0.0","revision":1,"revision":2,"locale":"th","gsi":{"game_dir":null,"port":null,"config_sha256":null,"token_ref":null}}"#,
            br#"{"schema_version":"1.0.0","revision":1,"locale":"th","gsi":{"port":3000}}"#,
            br#"{"schema_version":"1.0.0","revision":1,"locale":"th","gsi":{"game_dir":null,"port":null,"config_sha256":null,"token_ref":null},"extra":true}"#,
            br#"{"schema_version":"1.0.0","revision":1,"locale":"th","gsi":{"game_dir":null,"port":3000,"config_sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","token_ref":null}}"#,
        ];

        for original in invalid_files {
            fs::write(&path, original).unwrap();
            assert_eq!(
                patch(&dir, &request(json!(1), "en")),
                Err(SOURCE_INVALID.to_owned())
            );
            assert_eq!(fs::read(&path).unwrap(), original);
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn get_rejects_parseable_settings_with_invalid_gsi_without_fabricating_defaults() {
        let dir = data_dir();
        let path = intake::settings_path(&dir);
        let original =
            br#"{"schema_version":"1.0.0","revision":9,"locale":"en","gsi":{"port":3000}}"#;
        fs::write(&path, original).unwrap();

        assert_eq!(get(&dir), Err(SOURCE_INVALID.to_owned()));
        assert_eq!(fs::read(path).unwrap(), original);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn get_rejects_a_parseable_but_unsupported_settings_version_as_invalid() {
        let dir = data_dir();
        let path = intake::settings_path(&dir);
        let original = br#"{"schema_version":"2.0.0","revision":1,"locale":"th","gsi":{"game_dir":null,"port":null,"config_sha256":null,"token_ref":null}}"#;
        fs::write(&path, original).unwrap();

        assert_eq!(get(&dir), Err(SOURCE_INVALID.to_owned()));
        assert_eq!(fs::read(path).unwrap(), original);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_persisted_revision_is_not_replaced_with_a_default() {
        let dir = data_dir();
        let path = intake::settings_path(&dir);
        let original = br#"{"schema_version":"1.0.0","locale":"th","gsi":{"port":3000}}"#;
        fs::write(&path, original).unwrap();

        assert_eq!(get(&dir), Err(SOURCE_INVALID.to_owned()));
        assert_eq!(
            patch(&dir, &request(json!(1), "en")),
            Err(SOURCE_INVALID.to_owned())
        );
        assert_eq!(fs::read(path).unwrap(), original);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invalid_requests_and_revision_conflicts_do_not_write() {
        let dir = data_dir();
        let original = settings_with_gsi();
        let original_bytes = write_settings(&dir, &original);

        let mut invalid = vec![
            request(json!(7), "en"),
            request(json!(7), "en"),
            request(json!(7), "en"),
            request(json!(7), "en"),
            request(json!(7), "en"),
            request(json!(7), "en"),
            request(json!(7), "en"),
            request(json!(7), "en"),
        ];
        invalid[0]["request_id"] = json!("short");
        invalid[1]["base_revision"] = json!(1.5);
        invalid[2]["patch"] = json!({});
        invalid[3]["patch"] = json!({"locale": "fr"});
        invalid[4]["patch"] = json!({"locale": "en", "revision": 8});
        invalid[5]["extra"] = json!(true);
        invalid[6]["base_revision"] = json!(0);
        invalid[7]["base_revision"] = json!(9_007_199_254_740_992_u64);
        for request in invalid {
            assert_eq!(patch(&dir, &request), Err(SOURCE_INVALID.to_owned()));
            assert_eq!(
                fs::read(intake::settings_path(&dir)).unwrap(),
                original_bytes
            );
        }

        let mut unsupported = request(json!(7), "en");
        unsupported["schema_version"] = json!("2.0.0");
        assert_eq!(
            patch(&dir, &unsupported),
            Err(VERSION_UNSUPPORTED.to_owned())
        );
        assert_eq!(
            patch(&dir, &request(json!(6), "en")),
            Err(VERSION_UNSUPPORTED.to_owned())
        );
        assert_eq!(
            fs::read(intake::settings_path(&dir)).unwrap(),
            original_bytes
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_atomic_write_keeps_original_settings_and_gsi_data() {
        let dir = data_dir();
        let original = settings_with_gsi();
        let original_bytes = write_settings(&dir, &original);
        let temp_path = dir.join("user-settings.json.tmp");
        fs::create_dir_all(&temp_path).unwrap();
        fs::write(temp_path.join("sentinel"), b"preserve").unwrap();

        assert_eq!(
            patch(&dir, &request(json!(7), "en")),
            Err(WRITE_FAILED.to_owned())
        );
        assert_eq!(
            fs::read(intake::settings_path(&dir)).unwrap(),
            original_bytes
        );
        let after: Value =
            serde_json::from_slice(&fs::read(intake::settings_path(&dir)).unwrap()).unwrap();
        assert_eq!(after["gsi"], original["gsi"]);
        assert_eq!(fs::read(temp_path.join("sentinel")).unwrap(), b"preserve");
        let _ = fs::remove_dir_all(dir);
    }
}
