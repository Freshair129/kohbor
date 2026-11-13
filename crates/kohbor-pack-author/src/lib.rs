mod archive;
pub mod audio;

use archive::{write_zip, ArchiveEntry};
use kohbor_pack::{
    catalog::Catalog, container::stage_package, descriptor::parse_descriptor, media::inspect_webm,
    Limits,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DESCRIPTOR_BYTES: usize = 2 * 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;
const MAX_BINDINGS: usize = 128;
const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub clips_dir: PathBuf,
    pub map_path: PathBuf,
    pub out_dir: PathBuf,
    pub package_id: String,
    pub revision: u64,
    pub display_name: String,
    pub catalog_path: PathBuf,
    pub rights_claim: Option<String>,
    pub publisher_key_id: Option<String>,
    /// Require every written binding to include a verified silent visual.
    pub require_visual: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenBinding {
    pub event_id: String,
    pub tier: Option<u32>,
    pub duration_ms: u32,
    /// Package-relative visual path, or `None` for an audio-only binding.
    pub visual: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedBinding {
    pub event_id: String,
    pub tier: Option<u32>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub package_path: PathBuf,
    pub provenance_path: PathBuf,
    pub container_sha256: String,
    pub written: Vec<WrittenBinding>,
    pub skipped: Vec<SkippedBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError {
    pub exit_code: u8,
    pub message: String,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BuildError {}

impl BuildError {
    fn new(exit_code: u8, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct MappingBinding {
    event_id: String,
    tier: Option<u32>,
    audio: String,
    /// Missing means this mapping is audio-only unless the CLI requires banners.
    visual: Option<String>,
    identity_audio: Option<String>,
    /// Must be zero for audio-only mappings.
    visual_tail_ms: u32,
}

#[derive(Debug, Clone)]
struct InputReceipt {
    kind: &'static str,
    path: String,
    size_bytes: u64,
    sha256: String,
}

struct Payload {
    path: String,
    bytes: Vec<u8>,
    role: &'static str,
    method: u16,
}

/// Build a deterministic profile-1 package and its sidecar provenance record.
pub fn build_package(options: &BuildOptions) -> Result<BuildReport, BuildError> {
    validate_options(options)?;
    let lexical_out = absolute_output_path(&options.out_dir)?;
    ensure_outside_git_worktree(&lexical_out)?;
    let out_dir = resolve_output_dir(&lexical_out)?;
    ensure_outside_git_worktree(&out_dir)?;

    let clips_dir = fs::canonicalize(&options.clips_dir)
        .map_err(|_| BuildError::new(1, "clips directory is unavailable"))?;
    if !clips_dir.is_dir() {
        return Err(BuildError::new(1, "clips path is not a directory"));
    }

    let (mapping_bytes, mapping_receipt) = read_named_input(&options.map_path, "mapping")?;
    let mapping_value = kohbor_gsi::strict_json::parse(&mapping_bytes)
        .map_err(|error| BuildError::new(1, format!("mapping JSON is invalid: {error:?}")))?;
    let mappings = parse_mapping(&mapping_value)?;

    let (catalog_bytes, catalog_receipt) = read_named_input(&options.catalog_path, "catalog")?;
    let catalog_value = kohbor_gsi::strict_json::parse(&catalog_bytes)
        .map_err(|error| BuildError::new(1, format!("catalog JSON is invalid: {error:?}")))?;
    let catalog = Catalog::from_json(&catalog_value)
        .ok_or_else(|| BuildError::new(1, "event catalog has an invalid shape"))?;

    let mut receipts = BTreeMap::<(String, String), InputReceipt>::new();
    insert_receipt(&mut receipts, mapping_receipt);
    insert_receipt(&mut receipts, catalog_receipt);
    let mut seen = BTreeSet::new();
    for mapping in &mappings {
        let event = catalog
            .event(&mapping.event_id)
            .ok_or_else(|| BuildError::new(1, format!("unknown event id: {}", mapping.event_id)))?;
        if !event.media_mappable {
            return Err(BuildError::new(
                1,
                format!("event is not media-mappable: {}", mapping.event_id),
            ));
        }
        match (mapping.tier, event.tier_range) {
            (Some(_), None) => {
                return Err(BuildError::new(
                    1,
                    format!("event does not allow a tier: {}", mapping.event_id),
                ));
            }
            (Some(tier), Some((minimum, maximum))) if !(minimum..=maximum).contains(&tier) => {
                return Err(BuildError::new(
                    1,
                    format!("tier is outside the catalog range for {}", mapping.event_id),
                ));
            }
            _ => {}
        }
        if !seen.insert((mapping.event_id.clone(), mapping.tier)) {
            return Err(BuildError::new(
                1,
                format!("duplicate event/tier binding: {}", mapping.event_id),
            ));
        }
    }

    let mut payloads = Vec::<Payload>::new();
    let mut manifest_bindings = Vec::<Value>::new();
    let mut written = Vec::<WrittenBinding>::new();
    let mut skipped = Vec::<SkippedBinding>::new();
    let mut expanded_payload_bytes = 0u64;

    for mapping in &mappings {
        if mapping.visual.is_none() && mapping.visual_tail_ms != 0 {
            skipped.push(skip_for(
                mapping,
                "audio-only binding must have visual_tail_ms 0",
            ));
            continue;
        }
        if options.require_visual && mapping.visual.is_none() {
            skipped.push(skip_for(
                mapping,
                "visual path is missing (--require-visual)",
            ));
            continue;
        }
        let visual_bytes = if let Some(visual_path) = mapping.visual.as_deref() {
            let bytes = match read_clip(&clips_dir, visual_path) {
                Ok(bytes) => bytes,
                Err(ClipReadError::TooLarge) => {
                    skipped.push(skip_for(mapping, "visual input exceeds 64 MiB"));
                    continue;
                }
                Err(ClipReadError::Unavailable) => {
                    skipped.push(skip_for(mapping, "visual input is unavailable"));
                    continue;
                }
                Err(ClipReadError::UnsafePath) => {
                    skipped.push(skip_for(mapping, "visual path is not a safe clip path"));
                    continue;
                }
            };
            add_clip_receipt(&mut receipts, visual_path, &bytes)?;
            if bytes.len() > MAX_PAYLOAD_BYTES {
                skipped.push(skip_for(
                    mapping,
                    "visual exceeds the package 32 MiB payload limit",
                ));
                continue;
            }
            if let Err(reason) = inspect_webm(&bytes) {
                skipped.push(skip_for(
                    mapping,
                    format!("visual failed silent WebM inspection: {}", reason.as_str()),
                ));
                continue;
            }
            Some(bytes)
        } else {
            None
        };

        let audio_bytes = match read_clip(&clips_dir, &mapping.audio) {
            Ok(bytes) => bytes,
            Err(ClipReadError::TooLarge) => {
                skipped.push(skip_for(mapping, "audio input exceeds 64 MiB"));
                continue;
            }
            Err(ClipReadError::Unavailable) => {
                return Err(BuildError::new(
                    1,
                    format!("audio input is unavailable for {}", mapping.event_id),
                ));
            }
            Err(ClipReadError::UnsafePath) => {
                return Err(BuildError::new(
                    1,
                    format!(
                        "audio path is not a safe clip path for {}",
                        mapping.event_id
                    ),
                ));
            }
        };
        add_clip_receipt(&mut receipts, &mapping.audio, &audio_bytes)?;
        let mixed = audio::convert_wav(&audio_bytes).map_err(|error| {
            BuildError::new(
                1,
                format!("audio conversion failed for {}: {error}", mapping.event_id),
            )
        })?;
        let duration_ms = u32::try_from(
            u64::try_from(mixed.samples)
                .map_err(|_| BuildError::new(1, "audio sample count is too large"))?
                .checked_mul(1_000)
                .ok_or_else(|| BuildError::new(1, "audio duration overflow"))?
                / 48_000,
        )
        .map_err(|_| BuildError::new(1, "audio duration is too large"))?;
        if duration_ms == 0 {
            return Err(BuildError::new(
                1,
                format!(
                    "audio duration is below the manifest minimum for {}",
                    mapping.event_id
                ),
            ));
        }

        let key = binding_key(mapping);
        let identity_payload = if let Some(identity_path) = mapping.identity_audio.as_deref() {
            let identity_bytes = match read_clip(&clips_dir, identity_path) {
                Ok(bytes) => bytes,
                Err(ClipReadError::TooLarge) => {
                    skipped.push(skip_for(mapping, "identity audio input exceeds 64 MiB"));
                    continue;
                }
                Err(ClipReadError::Unavailable) => {
                    return Err(BuildError::new(
                        1,
                        format!("identity audio is unavailable for {}", mapping.event_id),
                    ));
                }
                Err(ClipReadError::UnsafePath) => {
                    return Err(BuildError::new(
                        1,
                        format!("identity audio path is unsafe for {}", mapping.event_id),
                    ));
                }
            };
            add_clip_receipt(&mut receipts, identity_path, &identity_bytes)?;
            let identity = audio::convert_wav(&identity_bytes).map_err(|error| {
                BuildError::new(
                    1,
                    format!(
                        "identity audio conversion failed for {}: {error}",
                        mapping.event_id
                    ),
                )
            })?;
            if identity.samples != mixed.samples {
                return Err(BuildError::new(
                    1,
                    format!(
                        "identity audio sample count differs for {}",
                        mapping.event_id
                    ),
                ));
            }
            let identity_output_path = format!("audio/{key}-identity.wav");
            Some((identity_output_path, identity.wav))
        } else {
            None
        };

        let mixed_path = format!("audio/{key}.wav");
        let visual_output_path = visual_bytes.as_ref().map(|_| format!("video/{key}.webm"));
        let mut binding_payload_bytes = u64::try_from(mixed.wav.len())
            .map_err(|_| BuildError::new(1, "generated WAV size is too large"))?;
        if let Some(bytes) = visual_bytes.as_ref() {
            binding_payload_bytes = binding_payload_bytes
                .checked_add(
                    u64::try_from(bytes.len())
                        .map_err(|_| BuildError::new(1, "visual size is too large"))?,
                )
                .ok_or_else(|| BuildError::new(1, "expanded package size overflow"))?;
        }
        if let Some((_, identity_bytes)) = identity_payload.as_ref() {
            binding_payload_bytes = binding_payload_bytes
                .checked_add(
                    u64::try_from(identity_bytes.len())
                        .map_err(|_| BuildError::new(1, "identity WAV size is too large"))?,
                )
                .ok_or_else(|| BuildError::new(1, "expanded package size overflow"))?;
        }
        expanded_payload_bytes = expanded_payload_bytes
            .checked_add(binding_payload_bytes)
            .ok_or_else(|| BuildError::new(1, "expanded package size overflow"))?;
        if expanded_payload_bytes > MAX_EXPANDED_BYTES {
            return Err(BuildError::new(
                1,
                "expanded payloads exceed the package 512 MiB limit",
            ));
        }
        payloads.push(Payload {
            path: mixed_path.clone(),
            bytes: mixed.wav,
            role: "mixed_audio",
            method: 8,
        });
        if let (Some(path), Some(bytes)) = (visual_output_path.as_ref(), visual_bytes) {
            payloads.push(Payload {
                path: path.clone(),
                bytes,
                role: "silent_visual",
                method: 0,
            });
        }
        let mut descriptor_binding = Map::new();
        descriptor_binding.insert("event_id".to_owned(), json!(mapping.event_id));
        if let Some(tier) = mapping.tier {
            descriptor_binding.insert("tier".to_owned(), json!(tier));
        }
        descriptor_binding.insert("mixed_audio_path".to_owned(), json!(mixed_path));
        if let Some(visual_path) = visual_output_path.as_ref() {
            descriptor_binding.insert("silent_visual_path".to_owned(), json!(visual_path));
        }
        descriptor_binding.insert("duration_ms".to_owned(), json!(duration_ms));
        descriptor_binding.insert("visual_tail_ms".to_owned(), json!(mapping.visual_tail_ms));
        if let Some((identity_output_path, identity_bytes)) = identity_payload {
            descriptor_binding.insert(
                "identity_audio_path".to_owned(),
                json!(identity_output_path),
            );
            payloads.push(Payload {
                path: identity_output_path,
                bytes: identity_bytes,
                role: "identity_audio",
                method: 8,
            });
        }

        manifest_bindings.push(Value::Object(descriptor_binding));
        written.push(WrittenBinding {
            event_id: mapping.event_id.clone(),
            tier: mapping.tier,
            duration_ms,
            visual: visual_output_path,
        });
    }

    if written.is_empty() {
        let reasons = skipped
            .iter()
            .map(|binding| format!("{}: {}", binding.event_id, binding.reason))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(BuildError::new(
            2,
            format!("no bindings remain after media checks; skipped: {reasons}"),
        ));
    }

    payloads.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    let mut file_inventory = Vec::new();
    for payload in &payloads {
        let size_bytes = payload.bytes.len();
        if size_bytes == 0 || size_bytes > MAX_PAYLOAD_BYTES {
            return Err(BuildError::new(
                1,
                "generated payload violates the manifest size limit",
            ));
        }
        file_inventory.push(json!({
            "path": payload.path,
            "size_bytes": size_bytes,
            "sha256": sha256_hex(&payload.bytes),
            "role": payload.role,
        }));
    }
    let taxonomy_version = format!("{}.{}", catalog.taxonomy.major, catalog.taxonomy.minor);
    let descriptor = json!({
        "schema_version": "1.3.0",
        "package_id": options.package_id,
        "revision": options.revision,
        "taxonomy_version": taxonomy_version,
        "display_name": options.display_name,
        "publisher_key_id": options.publisher_key_id,
        "files": file_inventory,
        "bindings": manifest_bindings,
    });
    let descriptor_bytes = serde_json::to_vec(&descriptor)
        .map_err(|error| BuildError::new(1, format!("descriptor serialization failed: {error}")))?;
    if descriptor_bytes.len() > MAX_DESCRIPTOR_BYTES {
        return Err(BuildError::new(1, "descriptor exceeds 2 MiB"));
    }
    let expanded_total = expanded_payload_bytes
        .checked_add(
            u64::try_from(descriptor_bytes.len())
                .map_err(|_| BuildError::new(1, "descriptor size is too large"))?,
        )
        .ok_or_else(|| BuildError::new(1, "expanded package size overflow"))?;
    if expanded_total > MAX_EXPANDED_BYTES {
        return Err(BuildError::new(
            1,
            "expanded package exceeds the package 512 MiB limit",
        ));
    }
    parse_descriptor(&descriptor_bytes, &catalog, &Limits::default()).map_err(|reason| {
        BuildError::new(
            1,
            format!("generated descriptor was rejected: {}", reason.as_str()),
        )
    })?;

    let mut archive_entries = Vec::with_capacity(payloads.len() + 1);
    archive_entries.push(ArchiveEntry {
        path: "descriptor.json",
        bytes: &descriptor_bytes,
        method: 8,
    });
    for payload in &payloads {
        archive_entries.push(ArchiveEntry {
            path: &payload.path,
            bytes: &payload.bytes,
            method: payload.method,
        });
    }
    let container_bytes = write_zip(&archive_entries).map_err(|message| {
        BuildError::new(1, format!("package container creation failed: {message}"))
    })?;
    let container_sha256 = sha256_hex(&container_bytes);
    let package_name = format!("{}-r{}.zip", options.package_id, options.revision);
    let provenance_name = format!(
        "{}-r{}.provenance.json",
        options.package_id, options.revision
    );
    let package_path = out_dir.join(&package_name);
    let provenance_path = out_dir.join(&provenance_name);
    if package_path.exists() || provenance_path.exists() {
        return Err(BuildError::new(
            1,
            "package or provenance output already exists",
        ));
    }

    fs::create_dir_all(&out_dir)
        .map_err(|_| BuildError::new(1, "could not create output directory"))?;
    if let Err(error) = write_new_file(&package_path, &container_bytes) {
        return Err(BuildError::new(
            1,
            format!("could not write package: {error}"),
        ));
    }

    let staging_path = create_staging_dir(&out_dir, &options.package_id, options.revision)
        .map_err(|message| {
            remove_if_exists(&package_path);
            BuildError::new(3, message)
        })?;
    let staged_result = stage_package(&package_path, &staging_path, &catalog, &Limits::default());
    let cleanup_result = fs::remove_dir_all(&staging_path);
    if staged_result.is_err() || cleanup_result.is_err() {
        remove_if_exists(&package_path);
        let detail = match staged_result {
            Err(reason) => reason.as_str().to_owned(),
            Ok(_) => "staging cleanup failed".to_owned(),
        };
        return Err(BuildError::new(
            3,
            format!("package round trip failed: {detail}"),
        ));
    }

    let input_files = receipts
        .into_values()
        .map(|receipt| {
            json!({
                "kind": receipt.kind,
                "path": receipt.path,
                "size_bytes": receipt.size_bytes,
                "sha256": receipt.sha256,
            })
        })
        .collect::<Vec<_>>();
    let provenance = json!({
        "schema_version": "1.0.0",
        "tool": { "name": "kohbor-pack-build", "version": env!("CARGO_PKG_VERSION") },
        "build_report": {
            "written": written.iter().map(|binding| json!({
                "event_id": binding.event_id,
                "tier": binding.tier,
                "duration_ms": binding.duration_ms,
                "visual": binding.visual,
            })).collect::<Vec<_>>(),
            "skipped": skipped.iter().map(|binding| json!({
                "event_id": binding.event_id,
                "tier": binding.tier,
                "reason": binding.reason,
            })).collect::<Vec<_>>(),
        },
        "input_files": input_files,
        "output_container": {
            "sha256": container_sha256,
            "size_bytes": container_bytes.len(),
        },
        "rights_claim": options.rights_claim.as_ref().map(|text| json!({
            "text": text,
            "status": "owner_claim_unverified",
        })),
    });
    let mut provenance_bytes = match serde_json::to_vec_pretty(&provenance) {
        Ok(bytes) => bytes,
        Err(error) => {
            remove_if_exists(&package_path);
            return Err(BuildError::new(
                1,
                format!("provenance serialization failed: {error}"),
            ));
        }
    };
    provenance_bytes.push(b'\n');
    if let Err(error) = write_new_file(&provenance_path, &provenance_bytes) {
        remove_if_exists(&package_path);
        return Err(BuildError::new(
            1,
            format!("could not write provenance: {error}"),
        ));
    }

    Ok(BuildReport {
        package_path,
        provenance_path,
        container_sha256,
        written,
        skipped,
    })
}

fn validate_options(options: &BuildOptions) -> Result<(), BuildError> {
    let package_id = options.package_id.as_bytes();
    if package_id.is_empty()
        || package_id.len() > 80
        || !package_id[0].is_ascii_lowercase() && !package_id[0].is_ascii_digit()
        || !package_id.iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(*byte, b'_' | b'.' | b'-')
        })
    {
        return Err(BuildError::new(
            1,
            "package id does not match the manifest pattern",
        ));
    }
    if options.revision == 0 || options.revision > 9_007_199_254_740_991 {
        return Err(BuildError::new(
            1,
            "revision must be between 1 and 9007199254740991",
        ));
    }
    if options.display_name.is_empty() || options.display_name.chars().count() > 96 {
        return Err(BuildError::new(
            1,
            "display name must contain 1 to 96 characters",
        ));
    }
    if let Some(key_id) = options.publisher_key_id.as_deref() {
        if key_id.is_empty() || key_id.chars().count() > 96 {
            return Err(BuildError::new(
                1,
                "publisher key id must contain 1 to 96 characters",
            ));
        }
    }
    Ok(())
}

fn parse_mapping(value: &Value) -> Result<Vec<MappingBinding>, BuildError> {
    let root = value
        .as_object()
        .ok_or_else(|| BuildError::new(1, "mapping root must be an object"))?;
    if root
        .keys()
        .any(|key| key != "schema_version" && key != "bindings")
    {
        return Err(BuildError::new(1, "mapping contains an unknown root field"));
    }
    if root.get("schema_version").and_then(Value::as_str) != Some("1.0.0") {
        return Err(BuildError::new(1, "mapping schema_version must be 1.0.0"));
    }
    let bindings = root
        .get("bindings")
        .and_then(Value::as_array)
        .ok_or_else(|| BuildError::new(1, "mapping bindings must be an array"))?;
    if bindings.len() > MAX_BINDINGS {
        return Err(BuildError::new(1, "mapping has more than 128 bindings"));
    }
    let mut result = Vec::with_capacity(bindings.len());
    for value in bindings {
        let object = value
            .as_object()
            .ok_or_else(|| BuildError::new(1, "mapping binding must be an object"))?;
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "event_id" | "tier" | "audio" | "visual" | "identity_audio" | "visual_tail_ms"
            )
        }) {
            return Err(BuildError::new(
                1,
                "mapping binding contains an unknown field",
            ));
        }
        let event_id = required_string(object, "event_id")?;
        let audio = required_string(object, "audio")?;
        validate_clip_path(&audio)
            .map_err(|_| BuildError::new(1, "audio path is not a safe relative clip path"))?;
        let visual = optional_string(object, "visual")?;
        if let Some(path) = visual.as_deref() {
            validate_clip_path(path)
                .map_err(|_| BuildError::new(1, "visual path is not a safe relative clip path"))?;
        }
        let identity_audio = optional_string(object, "identity_audio")?;
        if let Some(path) = identity_audio.as_deref() {
            validate_clip_path(path).map_err(|_| {
                BuildError::new(1, "identity audio path is not a safe relative clip path")
            })?;
        }
        let tier = optional_u32(object, "tier")?;
        if tier.is_some_and(|value| value == 0 || value > 100) {
            return Err(BuildError::new(1, "tier must be between 1 and 100"));
        }
        let visual_tail_ms =
            optional_u32(object, "visual_tail_ms")?.map_or(0, std::convert::identity);
        if visual_tail_ms > 3_200 {
            return Err(BuildError::new(
                1,
                "visual_tail_ms must be between 0 and 3200",
            ));
        }
        result.push(MappingBinding {
            event_id,
            tier,
            audio,
            visual,
            identity_audio,
            visual_tail_ms,
        });
    }
    Ok(result)
}

fn required_string(object: &Map<String, Value>, key: &str) -> Result<String, BuildError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            BuildError::new(1, format!("mapping field {key} must be a nonempty string"))
        })?;
    Ok(value.to_owned())
}

fn optional_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, BuildError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => Ok(Some(value.clone())),
        Some(_) => Err(BuildError::new(
            1,
            format!("mapping field {key} must be a nonempty string"),
        )),
    }
}

fn optional_u32(object: &Map<String, Value>, key: &str) -> Result<Option<u32>, BuildError> {
    match object.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|number| u32::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| {
                BuildError::new(
                    1,
                    format!("mapping field {key} must be an unsigned integer"),
                )
            }),
    }
}

enum ClipReadError {
    TooLarge,
    Unavailable,
    UnsafePath,
}

fn read_clip(root: &Path, relative: &str) -> Result<Vec<u8>, ClipReadError> {
    let relative_path = validate_clip_path(relative).map_err(|()| ClipReadError::UnsafePath)?;
    let candidate = root.join(relative_path);
    let canonical = fs::canonicalize(&candidate).map_err(|_| ClipReadError::Unavailable)?;
    if !canonical.starts_with(root) {
        return Err(ClipReadError::UnsafePath);
    }
    let metadata = fs::metadata(&canonical).map_err(|_| ClipReadError::Unavailable)?;
    if !metadata.is_file() {
        return Err(ClipReadError::Unavailable);
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(ClipReadError::TooLarge);
    }
    let file = File::open(canonical).map_err(|_| ClipReadError::Unavailable)?;
    let read_limit = MAX_INPUT_BYTES
        .checked_add(1)
        .ok_or(ClipReadError::TooLarge)?;
    let mut bytes = Vec::new();
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| ClipReadError::Unavailable)?;
    if u64::try_from(bytes.len()).map_err(|_| ClipReadError::TooLarge)? > MAX_INPUT_BYTES {
        return Err(ClipReadError::TooLarge);
    }
    Ok(bytes)
}

fn validate_clip_path(path: &str) -> Result<PathBuf, ()> {
    if path.is_empty() || path.contains(['\\', ':', '\0']) || path.starts_with('/') {
        return Err(());
    }
    let path_buf = Path::new(path);
    if path_buf.is_absolute() {
        return Err(());
    }
    for component in path_buf.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(());
        }
    }
    Ok(path_buf.to_path_buf())
}

fn read_named_input(
    path: &Path,
    kind: &'static str,
) -> Result<(Vec<u8>, InputReceipt), BuildError> {
    let metadata = fs::metadata(path)
        .map_err(|_| BuildError::new(1, format!("{kind} file is unavailable")))?;
    if !metadata.is_file() {
        return Err(BuildError::new(1, format!("{kind} path is not a file")));
    }
    if metadata.len() > MAX_INPUT_BYTES {
        return Err(BuildError::new(1, format!("{kind} file exceeds 64 MiB")));
    }
    let file =
        File::open(path).map_err(|_| BuildError::new(1, format!("{kind} file is unavailable")))?;
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| BuildError::new(1, format!("{kind} file could not be read")))?;
    let byte_len = u64::try_from(bytes.len())
        .map_err(|_| BuildError::new(1, format!("{kind} file is too large")))?;
    if byte_len > MAX_INPUT_BYTES {
        return Err(BuildError::new(1, format!("{kind} file exceeds 64 MiB")));
    }
    let path_label = match kind {
        "mapping" => "mapping.json".to_owned(),
        "catalog" => "event-catalog.json".to_owned(),
        _ => kind.to_owned(),
    };
    let size_bytes = u64::try_from(bytes.len())
        .map_err(|_| BuildError::new(1, format!("{kind} file is too large")))?;
    let receipt = InputReceipt {
        kind,
        path: path_label,
        size_bytes,
        sha256: sha256_hex(&bytes),
    };
    Ok((bytes, receipt))
}

fn insert_receipt(receipts: &mut BTreeMap<(String, String), InputReceipt>, receipt: InputReceipt) {
    receipts.insert((receipt.kind.to_owned(), receipt.path.clone()), receipt);
}

fn add_clip_receipt(
    receipts: &mut BTreeMap<(String, String), InputReceipt>,
    path: &str,
    bytes: &[u8],
) -> Result<(), BuildError> {
    let normalized = path.replace('/', "\\").replace('\\', "/");
    let size_bytes = u64::try_from(bytes.len())
        .map_err(|_| BuildError::new(1, "clip input length is too large"))?;
    receipts.insert(
        ("clip".to_owned(), normalized.clone()),
        InputReceipt {
            kind: "clip",
            path: normalized,
            size_bytes,
            sha256: sha256_hex(bytes),
        },
    );
    Ok(())
}

fn skip_for(mapping: &MappingBinding, reason: impl Into<String>) -> SkippedBinding {
    SkippedBinding {
        event_id: mapping.event_id.clone(),
        tier: mapping.tier,
        reason: reason.into(),
    }
}

fn binding_key(mapping: &MappingBinding) -> String {
    match mapping.tier {
        Some(tier) => format!("{}-t{tier}", mapping.event_id),
        None => mapping.event_id.clone(),
    }
}

fn absolute_output_path(path: &Path) -> Result<PathBuf, BuildError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| BuildError::new(1, "current directory is unavailable"))?
            .join(path)
    };
    Ok(normalize_absolute(&absolute))
}

fn resolve_output_dir(absolute: &Path) -> Result<PathBuf, BuildError> {
    let mut existing = absolute;
    let mut suffix = Vec::<std::ffi::OsString>::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| BuildError::new(1, "output directory path is invalid"))?;
        suffix.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| BuildError::new(1, "output directory path is invalid"))?;
    }
    let mut resolved = fs::canonicalize(existing)
        .map_err(|_| BuildError::new(1, "output directory parent is unavailable"))?;
    for component in suffix.iter().rev() {
        resolved.push(component);
    }
    if resolved.exists() && !resolved.is_dir() {
        return Err(BuildError::new(1, "output path is not a directory"));
    }
    Ok(resolved)
}

fn normalize_absolute(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn ensure_outside_git_worktree(out_dir: &Path) -> Result<(), BuildError> {
    let mut current = Some(out_dir);
    while let Some(directory) = current {
        let marker = directory.join(".git");
        if fs::symlink_metadata(&marker).is_ok() {
            return Err(BuildError::new(
                4,
                "output directory is inside a git work tree",
            ));
        }
        current = directory.parent();
    }
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let result = file.write_all(bytes).and_then(|()| file.flush());
    if let Err(error) = result {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

fn remove_if_exists(path: &Path) {
    let _ = fs::remove_file(path);
}

fn create_staging_dir(out_dir: &Path, package_id: &str, revision: u64) -> Result<PathBuf, String> {
    for suffix in 0u32..1000 {
        let path = out_dir.join(format!(".{package_id}-r{revision}-stage-{suffix}"));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => {
                return Err("could not create package round-trip staging directory".to_owned())
            }
        }
    }
    Err("could not reserve a fresh package staging directory".to_owned())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Parse command-line arguments for the `kohbor-pack-build` binary.
pub fn parse_cli_args(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> Result<BuildOptions, BuildError> {
    let mut values = BTreeMap::<String, String>::new();
    let mut rights_claim = None;
    let mut publisher_key_id = None;
    let mut require_visual = false;
    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        let flag = argument
            .into_string()
            .map_err(|_| BuildError::new(1, "argument is not valid Unicode"))?;
        if flag == "--require-visual" {
            if require_visual {
                return Err(BuildError::new(1, "duplicate option: --require-visual"));
            }
            require_visual = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| BuildError::new(1, format!("missing value for {flag}")))?
            .into_string()
            .map_err(|_| BuildError::new(1, "argument is not valid Unicode"))?;
        match flag.as_str() {
            "--clips" | "--map" | "--out" | "--package-id" | "--revision" | "--display-name"
            | "--catalog" => {
                if values.insert(flag.clone(), value).is_some() {
                    return Err(BuildError::new(1, format!("duplicate option: {flag}")));
                }
            }
            "--rights-claim" => {
                if rights_claim.replace(value).is_some() {
                    return Err(BuildError::new(1, "duplicate option: --rights-claim"));
                }
            }
            "--publisher-key-id" => {
                if publisher_key_id.replace(value).is_some() {
                    return Err(BuildError::new(1, "duplicate option: --publisher-key-id"));
                }
            }
            _ => return Err(BuildError::new(1, format!("unknown option: {flag}"))),
        }
    }
    let required = |key: &str| {
        values
            .get(key)
            .cloned()
            .ok_or_else(|| BuildError::new(1, format!("missing required option: {key}")))
    };
    let revision = required("--revision")?
        .parse::<u64>()
        .map_err(|_| BuildError::new(1, "revision must be an unsigned integer"))?;
    Ok(BuildOptions {
        clips_dir: PathBuf::from(required("--clips")?),
        map_path: PathBuf::from(required("--map")?),
        out_dir: PathBuf::from(required("--out")?),
        package_id: required("--package-id")?,
        revision,
        display_name: required("--display-name")?,
        catalog_path: PathBuf::from(required("--catalog")?),
        rights_claim,
        publisher_key_id,
        require_visual,
    })
}

fn cli_report_json(report: &BuildReport) -> Value {
    let package_name = report
        .package_path
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(String::new, str::to_owned);
    let provenance_name = report
        .provenance_path
        .file_name()
        .and_then(|name| name.to_str())
        .map_or_else(String::new, str::to_owned);
    json!({
        "package": package_name,
        "provenance": provenance_name,
        "container_sha256": report.container_sha256,
        "written": report.written.iter().map(|binding| json!({
            "event_id": binding.event_id,
            "tier": binding.tier,
            "duration_ms": binding.duration_ms,
            "visual": binding.visual,
        })).collect::<Vec<_>>(),
        "skipped": report.skipped.iter().map(|binding| json!({
            "event_id": binding.event_id,
            "tier": binding.tier,
            "reason": binding.reason,
        })).collect::<Vec<_>>(),
    })
}

/// Run the CLI parser and builder. Returns the process exit code.
pub fn run_cli(args: impl IntoIterator<Item = std::ffi::OsString>) -> u8 {
    match parse_cli_args(args).and_then(|options| build_package(&options)) {
        Ok(report) => match serde_json::to_string(&cli_report_json(&report)) {
            Ok(output) => {
                println!("{output}");
                0
            }
            Err(error) => {
                eprintln!("report serialization failed: {error}");
                1
            }
        },
        Err(error) => {
            eprintln!("{}", error.message);
            error.exit_code
        }
    }
}
