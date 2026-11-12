//! Package descriptor (`descriptor.json`, media-manifest 1.3.0): strict parsing
//! plus the semantic rules of EVENT-AUDIO-SPEC section 7 and MP-03/MP-05.
//! Pure: bytes in, a typed descriptor or one specific [`RejectReason`] out.

use crate::catalog::{Catalog, TaxonomyVersion};
use crate::error::RejectReason;
use crate::fsutil::{has_unsafe_windows_segment, windows_path_key};
use crate::limits::{
    Limits, MAX_AUDIO_MS, MAX_BINDINGS, MAX_DESCRIPTOR_BYTES, MAX_PAYLOAD_BYTES, MAX_PAYLOAD_COUNT,
    MAX_VISUAL_TAIL_MS,
};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRole {
    MixedAudio,
    SilentVisual,
    IdentityAudio,
    SourceAsset,
    Font,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub role: FileRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub event_id: String,
    pub tier: Option<u32>,
    pub mixed_audio_path: String,
    pub silent_visual_path: Option<String>,
    pub duration_ms: u32,
    pub visual_tail_ms: u32,
    pub identity_audio_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    pub package_id: String,
    pub revision: u64,
    pub taxonomy: TaxonomyVersion,
    pub display_name: String,
    /// A claim only; never trust (MEDIA-008).
    pub publisher_key_id: Option<String>,
    pub files: Vec<FileEntry>,
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDescriptor {
    pub descriptor: Descriptor,
    /// Binding event IDs that the runtime catalog does not know or that are not
    /// media-mappable: verified like the rest but excluded from playback and
    /// preview, reported locally (MP-05).
    pub inert_event_ids: Vec<String>,
}

/// Parse and validate descriptor bytes.
///
/// Order of checks (the first failure is the reported reason):
/// 1. size > `limits.descriptor_bytes` -> `EntryTooLarge`; not UTF-8 JSON ->
///    `DescriptorSyntax`; duplicate object key anywhere -> `DuplicateJsonKey`.
/// 2. Shape per media-manifest 1.3.0 with `additionalProperties: false`: an
///    unknown key -> `UnknownField(key)` (this is how a self-declared
///    `trusted_public_key` or authority claim is rejected); any other schema
///    violation (type, pattern, range, counts, `schema_version != "1.3.0"`,
///    missing/null `taxonomy_version`, or a nonzero visual tail without a
///    visual) -> `DescriptorSchema(json_pointer)`; a nonzero audio-only tail
///    points to `/bindings/<index>/visual_tail_ms`;
///    a `taxonomy_version` string outside the canonical grammar ->
///    `TaxonomyMalformed`.
/// 3. Taxonomy: package major != runtime major -> `TaxonomyMajorUnsupported`.
/// 4. Paths: every path in `files` and every present binding path must be relative,
///    ASCII, without `..`, `.`, empty, backslash, drive, `:` or leading `/`
///    segments -> `UnsafePath`; `files[].path` duplicates (case-insensitive)
///    -> `DuplicateEntry`.
/// 5. References: a binding path missing from `files` -> `DanglingReference`;
///    the role of the referenced file must be `mixed_audio` for
///    `mixed_audio_path`, `silent_visual` for a present `silent_visual_path` and
///    `identity_audio` for `identity_audio_path` -> `BindingRoleMismatch`.
/// 6. Tiers: `tier` on an event without catalog `tier_range` -> `TierNotAllowed`;
///    outside the range -> `TierOutOfRange`; two bindings with the same
///    `(event_id, tier)` (a missing tier counts as one tier) -> `TierDuplicate`;
///    a tiered event bound without any tier is allowed (the tier-less binding).
/// 7. Pairing: every binding names a mixed audio path and may name a silent
///    visual path. Without a visual, `visual_tail_ms` must be zero; such a
///    binding is audio-only. A `files` entry with role `mixed_audio` or
///    `silent_visual` that no binding uses -> `UnpairedBinding`.
///
/// Unknown (or not media-mappable) `event_id`s are not an error: they are
/// listed in `inert_event_ids` and skipped by checks 6 and the playback index.
pub fn parse_descriptor(
    bytes: &[u8],
    catalog: &Catalog,
    limits: &Limits,
) -> Result<ParsedDescriptor, RejectReason> {
    let limits = limits.clamped();
    let byte_len = u64::try_from(bytes.len()).map_err(|_| RejectReason::EntryTooLarge)?;
    if byte_len > limits.descriptor_bytes.min(MAX_DESCRIPTOR_BYTES) {
        return Err(RejectReason::EntryTooLarge);
    }

    let value = kohbor_gsi::strict_json::parse(bytes).map_err(|error| match error {
        kohbor_gsi::strict_json::StrictJsonError::DuplicateKey => RejectReason::DuplicateJsonKey,
        kohbor_gsi::strict_json::StrictJsonError::Syntax
        | kohbor_gsi::strict_json::StrictJsonError::TooDeep => RejectReason::DescriptorSyntax,
    })?;
    reject_unsafe_windows_paths(&value)?;
    validate_schema(&value, limits)?;

    let root = value.as_object().ok_or_else(|| schema_error(""))?;
    let taxonomy_text = required_string(root, "taxonomy_version", "/taxonomy_version")?;
    let taxonomy = TaxonomyVersion::parse(taxonomy_text).ok_or(RejectReason::TaxonomyMalformed)?;
    if taxonomy.major != catalog.taxonomy.major {
        return Err(RejectReason::TaxonomyMajorUnsupported);
    }

    let file_values = root
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| schema_error("/files"))?;
    let mut files = Vec::with_capacity(file_values.len());
    for (index, item) in file_values.iter().enumerate() {
        let pointer = format!("/files/{index}");
        let object = item.as_object().ok_or_else(|| schema_error(&pointer))?;
        let path = required_string(object, "path", &format!("{pointer}/path"))?.to_owned();
        let size_bytes = required_u64(object, "size_bytes", &format!("{pointer}/size_bytes"))?;
        let sha256 = required_string(object, "sha256", &format!("{pointer}/sha256"))?.to_owned();
        let role = match required_string(object, "role", &format!("{pointer}/role"))? {
            "mixed_audio" => FileRole::MixedAudio,
            "silent_visual" => FileRole::SilentVisual,
            "identity_audio" => FileRole::IdentityAudio,
            "source_asset" => FileRole::SourceAsset,
            "font" => FileRole::Font,
            _ => return Err(schema_error(&format!("{pointer}/role"))),
        };
        files.push(FileEntry {
            path,
            size_bytes,
            sha256,
            role,
        });
    }

    let binding_values = root
        .get("bindings")
        .and_then(Value::as_array)
        .ok_or_else(|| schema_error("/bindings"))?;
    let mut bindings = Vec::with_capacity(binding_values.len());
    for (index, item) in binding_values.iter().enumerate() {
        let pointer = format!("/bindings/{index}");
        let object = item.as_object().ok_or_else(|| schema_error(&pointer))?;
        let tier = match object.get("tier") {
            Some(value) => Some(
                value
                    .as_u64()
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| schema_error(&format!("{pointer}/tier")))?,
            ),
            None => None,
        };
        let identity_audio_path = match object.get("identity_audio_path") {
            Some(value) => Some(
                value
                    .as_str()
                    .ok_or_else(|| schema_error(&format!("{pointer}/identity_audio_path")))?
                    .to_owned(),
            ),
            None => None,
        };
        let duration_ms = required_u64(object, "duration_ms", &format!("{pointer}/duration_ms"))?;
        let visual_tail_ms = required_u64(
            object,
            "visual_tail_ms",
            &format!("{pointer}/visual_tail_ms"),
        )?;
        bindings.push(Binding {
            event_id: required_string(object, "event_id", &format!("{pointer}/event_id"))?
                .to_owned(),
            tier,
            mixed_audio_path: required_string(
                object,
                "mixed_audio_path",
                &format!("{pointer}/mixed_audio_path"),
            )?
            .to_owned(),
            silent_visual_path: object
                .get("silent_visual_path")
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| schema_error(&format!("{pointer}/silent_visual_path")))
                })
                .transpose()?,
            duration_ms: u32::try_from(duration_ms)
                .map_err(|_| schema_error(&format!("{pointer}/duration_ms")))?,
            visual_tail_ms: u32::try_from(visual_tail_ms)
                .map_err(|_| schema_error(&format!("{pointer}/visual_tail_ms")))?,
            identity_audio_path,
        });
    }

    // Schema-valid paths can still contain semantically unsafe segments.
    for file in &files {
        if !safe_manifest_path(&file.path) {
            return Err(RejectReason::UnsafePath);
        }
    }
    for binding in &bindings {
        if !safe_manifest_path(&binding.mixed_audio_path)
            || binding
                .silent_visual_path
                .as_deref()
                .is_some_and(|path| !safe_manifest_path(path))
            || binding
                .identity_audio_path
                .as_deref()
                .is_some_and(|path| !safe_manifest_path(path))
        {
            return Err(RejectReason::UnsafePath);
        }
    }

    for (index, file) in files.iter().enumerate() {
        if files
            .iter()
            .take(index)
            .any(|previous| windows_path_key(&previous.path) == windows_path_key(&file.path))
        {
            return Err(RejectReason::DuplicateEntry);
        }
    }

    for binding in &bindings {
        check_reference(&files, &binding.mixed_audio_path, FileRole::MixedAudio)?;
        if let Some(path) = binding.silent_visual_path.as_deref() {
            check_reference(&files, path, FileRole::SilentVisual)?;
        }
        if let Some(path) = binding.identity_audio_path.as_deref() {
            check_reference(&files, path, FileRole::IdentityAudio)?;
        }
    }

    let mut inert_event_ids = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        let Some(event) = catalog.event(&binding.event_id) else {
            if !inert_event_ids.iter().any(|id| id == &binding.event_id) {
                inert_event_ids.push(binding.event_id.clone());
            }
            continue;
        };
        if !event.media_mappable {
            if !inert_event_ids.iter().any(|id| id == &binding.event_id) {
                inert_event_ids.push(binding.event_id.clone());
            }
            continue;
        }
        if let Some(tier) = binding.tier {
            match event.tier_range {
                None => return Err(RejectReason::TierNotAllowed),
                Some((min, max)) if tier < min || tier > max => {
                    return Err(RejectReason::TierOutOfRange)
                }
                Some(_) => {}
            }
        }
        if bindings.iter().take(index).any(|previous| {
            previous.event_id == binding.event_id
                && previous.tier == binding.tier
                && catalog
                    .event(&previous.event_id)
                    .is_some_and(|previous_event| previous_event.media_mappable)
        }) {
            return Err(RejectReason::TierDuplicate);
        }
    }

    for file in &files {
        let is_pair_role = matches!(file.role, FileRole::MixedAudio | FileRole::SilentVisual);
        if is_pair_role
            && !bindings.iter().any(|binding| match file.role {
                FileRole::MixedAudio => binding.mixed_audio_path == file.path,
                FileRole::SilentVisual => {
                    binding.silent_visual_path.as_deref() == Some(file.path.as_str())
                }
                _ => false,
            })
        {
            return Err(RejectReason::UnpairedBinding);
        }
    }

    Ok(ParsedDescriptor {
        descriptor: Descriptor {
            package_id: required_string(root, "package_id", "/package_id")?.to_owned(),
            revision: required_u64(root, "revision", "/revision")?,
            taxonomy,
            display_name: required_string(root, "display_name", "/display_name")?.to_owned(),
            publisher_key_id: optional_nullable_string(
                root,
                "publisher_key_id",
                "/publisher_key_id",
            )?,
            files,
            bindings,
        },
        inert_event_ids,
    })
}

fn validate_schema(value: &Value, limits: Limits) -> Result<(), RejectReason> {
    let root = value.as_object().ok_or_else(|| schema_error(""))?;
    reject_unknown_keys(
        root,
        &[
            "schema_version",
            "package_id",
            "revision",
            "taxonomy_version",
            "display_name",
            "publisher_key_id",
            "files",
            "bindings",
        ],
    )?;

    for (key, allowed) in [
        ("files", &["path", "size_bytes", "sha256", "role"][..]),
        (
            "bindings",
            &[
                "event_id",
                "tier",
                "mixed_audio_path",
                "silent_visual_path",
                "duration_ms",
                "visual_tail_ms",
                "identity_audio_path",
            ][..],
        ),
    ] {
        if let Some(items) = root.get(key).and_then(Value::as_array) {
            for item in items {
                if let Some(object) = item.as_object() {
                    reject_unknown_keys(object, allowed)?;
                }
            }
        }
    }

    if root.get("schema_version").and_then(Value::as_str) != Some("1.3.0") {
        return Err(schema_error("/schema_version"));
    }
    let package_id = required_string(root, "package_id", "/package_id")?;
    if package_id.is_empty()
        || package_id.len() > 80
        || !package_id.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_lowercase() || byte.is_ascii_digit()
            } else {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'.' | b'-')
            }
        })
    {
        return Err(schema_error("/package_id"));
    }
    let revision = required_u64(root, "revision", "/revision")?;
    if !(1..=9_007_199_254_740_991).contains(&revision) {
        return Err(schema_error("/revision"));
    }
    let taxonomy_text = required_string(root, "taxonomy_version", "/taxonomy_version")?;
    if TaxonomyVersion::parse(taxonomy_text).is_none() {
        return Err(RejectReason::TaxonomyMalformed);
    }
    let display_name = required_string(root, "display_name", "/display_name")?;
    if display_name.is_empty() || display_name.chars().count() > 96 {
        return Err(schema_error("/display_name"));
    }
    match root.get("publisher_key_id") {
        Some(Value::Null) => {}
        Some(Value::String(value)) if !value.is_empty() && value.chars().count() <= 96 => {}
        _ => return Err(schema_error("/publisher_key_id")),
    }

    let files = root
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(|| schema_error("/files"))?;
    let max_files = limits.payload_count.min(MAX_PAYLOAD_COUNT);
    if files.is_empty() || files.len() > max_files {
        return Err(schema_error("/files"));
    }
    for (index, item) in files.iter().enumerate() {
        let pointer = format!("/files/{index}");
        let object = item.as_object().ok_or_else(|| schema_error(&pointer))?;
        let path = required_string(object, "path", &format!("{pointer}/path"))?;
        if !valid_path_pattern(path) {
            return Err(schema_error(&format!("{pointer}/path")));
        }
        let size = required_u64(object, "size_bytes", &format!("{pointer}/size_bytes"))?;
        if size == 0 || size > limits.payload_bytes.min(MAX_PAYLOAD_BYTES) {
            return Err(schema_error(&format!("{pointer}/size_bytes")));
        }
        let hash = required_string(object, "sha256", &format!("{pointer}/sha256"))?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(schema_error(&format!("{pointer}/sha256")));
        }
        match required_string(object, "role", &format!("{pointer}/role"))? {
            "mixed_audio" | "silent_visual" | "identity_audio" | "source_asset" | "font" => {}
            _ => return Err(schema_error(&format!("{pointer}/role"))),
        }
    }

    let bindings = root
        .get("bindings")
        .and_then(Value::as_array)
        .ok_or_else(|| schema_error("/bindings"))?;
    if bindings.len() > limits.bindings.min(MAX_BINDINGS) {
        return Err(schema_error("/bindings"));
    }
    for (index, item) in bindings.iter().enumerate() {
        let pointer = format!("/bindings/{index}");
        let object = item.as_object().ok_or_else(|| schema_error(&pointer))?;
        let event_id = required_string(object, "event_id", &format!("{pointer}/event_id"))?;
        if !valid_event_id(event_id) {
            return Err(schema_error(&format!("{pointer}/event_id")));
        }
        if let Some(tier) = object.get("tier") {
            let tier = tier
                .as_u64()
                .ok_or_else(|| schema_error(&format!("{pointer}/tier")))?;
            if !(1..=100).contains(&tier) {
                return Err(schema_error(&format!("{pointer}/tier")));
            }
        }
        for key in ["mixed_audio_path", "identity_audio_path"] {
            if key == "identity_audio_path" && !object.contains_key(key) {
                continue;
            }
            let path = required_string(object, key, &format!("{pointer}/{key}"))?;
            if !valid_path_pattern(path) {
                return Err(schema_error(&format!("{pointer}/{key}")));
            }
        }
        if let Some(value) = object.get("silent_visual_path") {
            let path = value
                .as_str()
                .ok_or_else(|| schema_error(&format!("{pointer}/silent_visual_path")))?;
            if !valid_path_pattern(path) {
                return Err(schema_error(&format!("{pointer}/silent_visual_path")));
            }
        }
        let duration = required_u64(object, "duration_ms", &format!("{pointer}/duration_ms"))?;
        if duration == 0 || duration > u64::from(MAX_AUDIO_MS) {
            return Err(schema_error(&format!("{pointer}/duration_ms")));
        }
        let tail = required_u64(
            object,
            "visual_tail_ms",
            &format!("{pointer}/visual_tail_ms"),
        )?;
        if tail > u64::from(MAX_VISUAL_TAIL_MS) {
            return Err(schema_error(&format!("{pointer}/visual_tail_ms")));
        }
        if !object.contains_key("silent_visual_path") && tail != 0 {
            return Err(schema_error(&format!("{pointer}/visual_tail_ms")));
        }
    }
    Ok(())
}

fn reject_unknown_keys(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), RejectReason> {
    if let Some(key) = object
        .keys()
        .find(|key| !allowed.iter().any(|allowed| key == allowed))
    {
        return Err(RejectReason::unknown_field(key));
    }
    Ok(())
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    pointer: &str,
) -> Result<&'a str, RejectReason> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| schema_error(pointer))
}

fn required_u64(
    object: &Map<String, Value>,
    key: &str,
    pointer: &str,
) -> Result<u64, RejectReason> {
    object
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| schema_error(pointer))
}

fn optional_nullable_string(
    object: &Map<String, Value>,
    key: &str,
    pointer: &str,
) -> Result<Option<String>, RejectReason> {
    match object.get(key) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(schema_error(pointer)),
    }
}

fn schema_error(pointer: &str) -> RejectReason {
    RejectReason::descriptor_schema(pointer)
}

fn reject_unsafe_windows_paths(value: &Value) -> Result<(), RejectReason> {
    let Some(root) = value.as_object() else {
        return Ok(());
    };
    if let Some(files) = root.get("files").and_then(Value::as_array) {
        for file in files {
            if let Some(path) = file.get("path").and_then(Value::as_str) {
                // Colons already fail the descriptor's frozen path pattern and
                // retain its DescriptorSchema diagnostic for compatibility.
                if !path.contains(':') && has_unsafe_windows_segment(path) {
                    return Err(RejectReason::UnsafePath);
                }
            }
        }
    }
    if let Some(bindings) = root.get("bindings").and_then(Value::as_array) {
        for binding in bindings {
            for key in [
                "mixed_audio_path",
                "silent_visual_path",
                "identity_audio_path",
            ] {
                if let Some(path) = binding.get(key).and_then(Value::as_str) {
                    if !path.contains(':') && has_unsafe_windows_segment(path) {
                        return Err(RejectReason::UnsafePath);
                    }
                }
            }
        }
    }
    Ok(())
}

fn valid_path_pattern(path: &str) -> bool {
    let mut bytes = path.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    path.len() <= 240
        && (first.is_ascii_alphanumeric() || first == b'_')
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'/' | b'-'))
}

fn safe_manifest_path(path: &str) -> bool {
    path.is_ascii()
        && !has_unsafe_windows_segment(path)
        && path.split('/').all(|segment| {
            !segment.is_empty() && segment != "." && segment != ".." && !segment.starts_with('.')
        })
}

fn valid_event_id(text: &str) -> bool {
    let mut bytes = text.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z'))
        && text.len() <= 80
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn check_reference(
    files: &[FileEntry],
    path: &str,
    expected_role: FileRole,
) -> Result<(), RejectReason> {
    match files.iter().find(|file| file.path == path) {
        None => Err(RejectReason::DanglingReference),
        Some(file) if file.role != expected_role => Err(RejectReason::BindingRoleMismatch),
        Some(_) => Ok(()),
    }
}
