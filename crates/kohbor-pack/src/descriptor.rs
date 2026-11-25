//! Package descriptor (`descriptor.json`, media-manifest 1.3.0, 1.4.0, and proposed 1.4.1): strict
//! parsing plus the semantic rules of EVENT-AUDIO-SPEC section 7 and MP-03/MP-05.
//! Pure: bytes in, a typed descriptor or one specific [`RejectReason`] out.

use crate::catalog::{Catalog, TaxonomyVersion};
use crate::error::RejectReason;
use crate::fsutil::{has_unsafe_windows_segment, windows_path_key};
use crate::limits::{
    Limits, MAX_AUDIO_MS, MAX_BINDINGS, MAX_DESCRIPTOR_BYTES, MAX_PAYLOAD_BYTES, MAX_PAYLOAD_COUNT,
    MAX_VARIANTS_PER_BINDING, MAX_VARIANTS_PER_PACK, MAX_VISUAL_TAIL_MS,
};
use serde_json::{Map, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRole {
    MixedAudio,
    SilentVisual,
    IdentityAudio,
    SourceAsset,
    Font,
    Cover,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub role: FileRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Random,
    Shuffle,
    Sequence,
}

impl Selection {
    pub fn as_str(self) -> &'static str {
        match self {
            Selection::Random => "random",
            Selection::Shuffle => "shuffle",
            Selection::Sequence => "sequence",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub variant_id: String,
    /// Provenance for local logs only; never an authority or trust claim.
    pub clip_id: Option<String>,
    /// Optional display-only text. Never used as identity, authority, or audio input.
    pub caption: Option<String>,
    pub mixed_audio_path: String,
    pub silent_visual_path: Option<String>,
    pub duration_ms: u32,
    pub visual_tail_ms: u32,
    pub identity_audio_path: Option<String>,
    pub weight: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub event_id: String,
    pub tier: Option<u32>,
    /// Optional own-hero condition for this event binding.
    pub hero: Option<String>,
    pub selection: Option<Selection>,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    pub package_id: String,
    pub revision: u64,
    pub taxonomy: TaxonomyVersion,
    pub display_name: String,
    /// A claim only; never trust (MEDIA-008).
    pub publisher_key_id: Option<String>,
    pub presentation: Option<Presentation>,
    pub files: Vec<FileEntry>,
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Presentation {
    pub description: Option<String>,
    pub author: Option<String>,
    pub languages: Vec<String>,
    pub cover: Option<String>,
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
/// 2. Shape per media-manifest 1.3.0, 1.4.0, 1.4.1, 1.4.2, or 1.4.3 with
///    `additionalProperties: false`: an unknown key -> `UnknownField(key)` (this
///    is how a self-declared `trusted_public_key` or authority claim is rejected);
///    a 1.4.x variant carrying reserved `when` -> `ConditionsReserved`; invalid
///    selection, variant IDs, clip IDs, weights or variant counts have specific
///    reasons; any other schema violation (type, pattern, range, counts,
///    unsupported schema version, missing/null `taxonomy_version`, or a
///    nonzero visual tail without a visual) -> `DescriptorSchema(json_pointer)`;
///    a nonzero audio-only tail points to the binding's or variant's
///    `visual_tail_ms`;
///    a `taxonomy_version` string outside the canonical grammar ->
///    `TaxonomyMalformed`.
/// 3. Taxonomy: package major != runtime major -> `TaxonomyMajorUnsupported`.
/// 4. Paths: every path in `files` and every present binding variant path must be relative,
///    ASCII, without `..`, `.`, empty, backslash, drive, `:` or leading `/`
///    segments -> `UnsafePath`; `files[].path` duplicates (case-insensitive)
///    -> `DuplicateEntry`.
/// 5. References: a variant path missing from `files` -> `DanglingReference`;
///    the role of the referenced file must be `mixed_audio` for
///    `mixed_audio_path`, `silent_visual` for a present `silent_visual_path` and
///    `identity_audio` for `identity_audio_path` -> `BindingRoleMismatch`.
/// 6. Tiers: `tier` on an event without catalog `tier_range` -> `TierNotAllowed`;
///    outside the range -> `TierOutOfRange`; two bindings with the same
///    `(event_id, tier)` (a missing tier counts as one tier) -> `TierDuplicate`;
///    a tiered event bound without any tier is allowed (the tier-less binding).
/// 7. Pairing: every variant names a mixed audio path and may name a silent
///    visual path. Without a visual, its `visual_tail_ms` must be zero; such a
///    variant is audio-only. A `files` entry with role `mixed_audio` or
///    `silent_visual` that no variant uses -> `UnpairedBinding`; one file may be
///    referenced by several variants.
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
    let schema_version = required_string(root, "schema_version", "/schema_version")?;
    let legacy_v13 = schema_version == "1.3.0";
    let captions_supported = matches!(schema_version, "1.4.1" | "1.4.2" | "1.4.3");
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
            "cover" => FileRole::Cover,
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
    let mut variant_ids = HashSet::new();
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
        let (selection, variants) = if legacy_v13 {
            let variant_id = format!("v{}", index + 1);
            let variant = parse_variant_fields(object, &pointer, variant_id, None, None, None)?;
            (None, vec![variant])
        } else {
            let variant_values = object
                .get("variants")
                .and_then(Value::as_array)
                .ok_or_else(|| schema_error(&format!("{pointer}/variants")))?;
            let selection = match object.get("selection") {
                None => None,
                Some(Value::String(value)) => Some(parse_selection(value)?),
                Some(_) => return Err(RejectReason::SelectionInvalid),
            };
            if variant_values.len() > 1 && selection.is_none() {
                return Err(RejectReason::SelectionRequired);
            }
            if variant_values.len() == 1 && selection.is_some() {
                return Err(RejectReason::SelectionUnexpected);
            }
            let mut variants = Vec::with_capacity(variant_values.len());
            for (variant_index, variant_value) in variant_values.iter().enumerate() {
                let variant_pointer = format!("{pointer}/variants/{variant_index}");
                let variant_object = variant_value
                    .as_object()
                    .ok_or_else(|| schema_error(&variant_pointer))?;
                let variant_id = required_string(
                    variant_object,
                    "variant_id",
                    &format!("{variant_pointer}/variant_id"),
                )?
                .to_owned();
                if !variant_ids.insert(variant_id.clone()) {
                    return Err(RejectReason::DuplicateVariantId);
                }
                let clip_id = variant_object
                    .get("clip_id")
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or(RejectReason::ClipIdInvalid)
                    })
                    .transpose()?;
                let caption = if captions_supported {
                    variant_object
                        .get("caption")
                        .map(|value| {
                            value
                                .as_str()
                                .filter(|text| caption_is_valid(text))
                                .map(str::to_owned)
                                .ok_or(RejectReason::CaptionInvalid)
                        })
                        .transpose()?
                } else {
                    None
                };
                let weight = variant_object
                    .get("weight")
                    .map(|value| {
                        value
                            .as_u64()
                            .and_then(|weight| u8::try_from(weight).ok())
                            .ok_or(RejectReason::VariantWeightInvalid)
                    })
                    .transpose()?;
                if weight.is_some() && selection != Some(Selection::Random) {
                    return Err(RejectReason::WeightSelectionMismatch);
                }
                variants.push(parse_variant_fields(
                    variant_object,
                    &variant_pointer,
                    variant_id,
                    clip_id,
                    weight,
                    caption,
                )?);
            }
            (selection, variants)
        };
        bindings.push(Binding {
            event_id: required_string(object, "event_id", &format!("{pointer}/event_id"))?
                .to_owned(),
            tier,
            hero: object
                .get("hero")
                .map(|value| {
                    let hero = value
                        .as_str()
                        .ok_or_else(|| schema_error(&format!("{pointer}/hero")))?;
                    if !valid_hero_id(hero) {
                        return Err(schema_error(&format!("{pointer}/hero")));
                    }
                    Ok(hero.to_owned())
                })
                .transpose()?,
            selection,
            variants,
        });
    }

    // Schema-valid paths can still contain semantically unsafe segments.
    let presentation = parse_presentation(root, schema_version)?;
    if let Some(cover_path) = presentation
        .as_ref()
        .and_then(|value| value.cover.as_deref())
    {
        check_reference(&files, cover_path, FileRole::Cover)?;
        let cover = files
            .iter()
            .find(|file| file.path == cover_path)
            .ok_or(RejectReason::DanglingReference)?;
        if cover.size_bytes > 1_048_576 {
            return Err(RejectReason::CoverInvalid);
        }
    }
    for file in &files {
        if !safe_manifest_path(&file.path) {
            return Err(RejectReason::UnsafePath);
        }
    }
    for binding in &bindings {
        for variant in &binding.variants {
            if !safe_manifest_path(&variant.mixed_audio_path)
                || variant
                    .silent_visual_path
                    .as_deref()
                    .is_some_and(|path| !safe_manifest_path(path))
                || variant
                    .identity_audio_path
                    .as_deref()
                    .is_some_and(|path| !safe_manifest_path(path))
            {
                return Err(RejectReason::UnsafePath);
            }
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
        for variant in &binding.variants {
            check_reference(&files, &variant.mixed_audio_path, FileRole::MixedAudio)?;
            if let Some(path) = variant.silent_visual_path.as_deref() {
                check_reference(&files, path, FileRole::SilentVisual)?;
            }
            if let Some(path) = variant.identity_audio_path.as_deref() {
                check_reference(&files, path, FileRole::IdentityAudio)?;
            }
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
        if event.critical_media_required && binding.variants.len() != 1 {
            return Err(RejectReason::CriticalMediaVariantCount);
        }
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
            && !bindings
                .iter()
                .flat_map(|binding| &binding.variants)
                .any(|variant| match file.role {
                    FileRole::MixedAudio => variant.mixed_audio_path == file.path,
                    FileRole::SilentVisual => {
                        variant.silent_visual_path.as_deref() == Some(file.path.as_str())
                    }
                    _ => false,
                })
        {
            return Err(RejectReason::UnpairedBinding);
        }
        if file.role == FileRole::Cover
            && presentation
                .as_ref()
                .and_then(|value| value.cover.as_deref())
                != Some(file.path.as_str())
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
            presentation,
            files,
            bindings,
        },
        inert_event_ids,
    })
}

fn validate_schema(value: &Value, limits: Limits) -> Result<(), RejectReason> {
    let root = value.as_object().ok_or_else(|| schema_error(""))?;
    let schema_version = root
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| schema_error("/schema_version"))?;
    if !matches!(schema_version, "1.3.0" | "1.4.0" | "1.4.1" | "1.4.2" | "1.4.3") {
        return Err(schema_error("/schema_version"));
    }
    let mut root_keys = vec![
        "schema_version",
        "package_id",
        "revision",
        "taxonomy_version",
        "display_name",
        "publisher_key_id",
        "files",
        "bindings",
    ];
    if matches!(schema_version, "1.4.2" | "1.4.3") {
        root_keys.push("presentation");
    }
    reject_unknown_keys(root, &root_keys)?;

    if let Some(files) = root.get("files").and_then(Value::as_array) {
        for item in files {
            if let Some(object) = item.as_object() {
                reject_unknown_keys(object, &["path", "size_bytes", "sha256", "role"])?;
            }
        }
    }
    if let Some(bindings) = root.get("bindings").and_then(Value::as_array) {
        for binding in bindings {
            if let Some(object) = binding.as_object() {
                if schema_version == "1.3.0" {
                    reject_unknown_keys(
                        object,
                        &[
                            "event_id",
                            "tier",
                            "mixed_audio_path",
                            "silent_visual_path",
                            "duration_ms",
                            "visual_tail_ms",
                            "identity_audio_path",
                        ],
                    )?;
                } else {
                    let binding_keys = if schema_version == "1.4.3" {
                        &["event_id", "tier", "hero", "selection", "variants"][..]
                    } else {
                        &["event_id", "tier", "selection", "variants"][..]
                    };
                    reject_unknown_keys(object, binding_keys)?;
                    if let Some(variants) = object.get("variants").and_then(Value::as_array) {
                        for variant in variants {
                            if let Some(variant_object) = variant.as_object() {
                                if variant_object.contains_key("when") {
                                    return Err(RejectReason::ConditionsReserved);
                                }
                                let allowed = if matches!(schema_version, "1.4.1" | "1.4.2" | "1.4.3") {
                                    &[
                                        "variant_id",
                                        "clip_id",
                                        "mixed_audio_path",
                                        "silent_visual_path",
                                        "duration_ms",
                                        "visual_tail_ms",
                                        "identity_audio_path",
                                        "weight",
                                        "caption",
                                    ][..]
                                } else {
                                    &[
                                        "variant_id",
                                        "clip_id",
                                        "mixed_audio_path",
                                        "silent_visual_path",
                                        "duration_ms",
                                        "visual_tail_ms",
                                        "identity_audio_path",
                                        "weight",
                                    ][..]
                                };
                                reject_unknown_keys(variant_object, allowed)?;
                            }
                        }
                    }
                }
            }
        }
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
    if let Some(presentation) = root.get("presentation") {
        validate_presentation(presentation)?;
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
            "mixed_audio" | "silent_visual" | "identity_audio" | "source_asset" | "font"
            | "cover" => {}
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
    let mut total_variants = 0usize;
    let mut seen_variant_ids = HashSet::new();
    for (index, item) in bindings.iter().enumerate() {
        let pointer = format!("/bindings/{index}");
        let object = item.as_object().ok_or_else(|| schema_error(&pointer))?;
        let event_id = required_string(object, "event_id", &format!("{pointer}/event_id"))?;
        if !valid_event_id(event_id) {
            return Err(schema_error(&format!("{pointer}/event_id")));
        }
        if let Some(hero) = object.get("hero") {
            let hero = hero
                .as_str()
                .ok_or_else(|| schema_error(&format!("{pointer}/hero")))?;
            if schema_version != "1.4.3" || !valid_hero_id(hero) {
                return Err(schema_error(&format!("{pointer}/hero")));
            }
        }
        if let Some(tier) = object.get("tier") {
            let tier = tier
                .as_u64()
                .ok_or_else(|| schema_error(&format!("{pointer}/tier")))?;
            if !(1..=100).contains(&tier) {
                return Err(schema_error(&format!("{pointer}/tier")));
            }
        }
        if schema_version == "1.3.0" {
            validate_legacy_binding(object, &pointer)?;
            total_variants = total_variants
                .checked_add(1)
                .ok_or(RejectReason::TooManyVariantsPerPack)?;
        } else {
            let variant_values = object
                .get("variants")
                .and_then(Value::as_array)
                .ok_or_else(|| schema_error(&format!("{pointer}/variants")))?;
            if variant_values.is_empty() {
                return Err(RejectReason::VariantCountInvalid);
            }
            if variant_values.len() > MAX_VARIANTS_PER_BINDING {
                return Err(RejectReason::TooManyVariantsPerBinding);
            }
            total_variants = total_variants
                .checked_add(variant_values.len())
                .ok_or(RejectReason::TooManyVariantsPerPack)?;
            if total_variants > MAX_VARIANTS_PER_PACK {
                return Err(RejectReason::TooManyVariantsPerPack);
            }

            let selection = match object.get("selection") {
                None => None,
                Some(Value::String(value)) => Some(parse_selection(value)?),
                Some(_) => return Err(RejectReason::SelectionInvalid),
            };
            if variant_values.len() > 1 && selection.is_none() {
                return Err(RejectReason::SelectionRequired);
            }
            if variant_values.len() == 1 && selection.is_some() {
                return Err(RejectReason::SelectionUnexpected);
            }

            for (variant_index, variant_value) in variant_values.iter().enumerate() {
                let variant_pointer = format!("{pointer}/variants/{variant_index}");
                let variant = variant_value
                    .as_object()
                    .ok_or_else(|| schema_error(&variant_pointer))?;
                let variant_id = required_string(
                    variant,
                    "variant_id",
                    &format!("{variant_pointer}/variant_id"),
                )?;
                if !valid_variant_id(variant_id) {
                    return Err(RejectReason::VariantIdInvalid);
                }
                if !seen_variant_ids.insert(variant_id.to_owned()) {
                    return Err(RejectReason::DuplicateVariantId);
                }
                if let Some(value) = variant.get("clip_id") {
                    let clip_id = value.as_str().ok_or(RejectReason::ClipIdInvalid)?;
                    if !valid_clip_id(clip_id) {
                        return Err(RejectReason::ClipIdInvalid);
                    }
                }
                for key in ["mixed_audio_path", "identity_audio_path"] {
                    if key == "identity_audio_path" && !variant.contains_key(key) {
                        continue;
                    }
                    let path = required_string(variant, key, &format!("{variant_pointer}/{key}"))?;
                    if !valid_path_pattern(path) {
                        return Err(schema_error(&format!("{variant_pointer}/{key}")));
                    }
                }
                if let Some(value) = variant.get("silent_visual_path") {
                    let path = value.as_str().ok_or_else(|| {
                        schema_error(&format!("{variant_pointer}/silent_visual_path"))
                    })?;
                    if !valid_path_pattern(path) {
                        return Err(schema_error(&format!(
                            "{variant_pointer}/silent_visual_path"
                        )));
                    }
                }
                let duration = required_u64(
                    variant,
                    "duration_ms",
                    &format!("{variant_pointer}/duration_ms"),
                )?;
                if duration == 0 || duration > u64::from(MAX_AUDIO_MS) {
                    return Err(schema_error(&format!("{variant_pointer}/duration_ms")));
                }
                let tail = required_u64(
                    variant,
                    "visual_tail_ms",
                    &format!("{variant_pointer}/visual_tail_ms"),
                )?;
                if tail > u64::from(MAX_VISUAL_TAIL_MS) {
                    return Err(schema_error(&format!("{variant_pointer}/visual_tail_ms")));
                }
                if !variant.contains_key("silent_visual_path") && tail != 0 {
                    return Err(schema_error(&format!("{variant_pointer}/visual_tail_ms")));
                }
                if let Some(weight) = variant.get("weight") {
                    let weight = weight.as_u64().ok_or(RejectReason::VariantWeightInvalid)?;
                    if !(1..=100).contains(&weight) {
                        return Err(RejectReason::VariantWeightInvalid);
                    }
                    if selection != Some(Selection::Random) {
                        return Err(RejectReason::WeightSelectionMismatch);
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_presentation(value: &Value) -> Result<(), RejectReason> {
    let object = value
        .as_object()
        .ok_or_else(|| schema_error("/presentation"))?;
    reject_unknown_keys(object, &["description", "author", "languages", "cover"])?;
    if object.is_empty() {
        return Err(schema_error("/presentation"));
    }
    for (key, maximum) in [("description", 500usize), ("author", 80usize)] {
        if let Some(value) = object.get(key) {
            let text = value
                .as_str()
                .ok_or_else(|| schema_error(&format!("/presentation/{key}")))?;
            if text.chars().count() > maximum
                || text.chars().any(|character| {
                    character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
                })
            {
                return Err(schema_error(&format!("/presentation/{key}")));
            }
        }
    }
    if let Some(value) = object.get("languages") {
        let languages = value
            .as_array()
            .filter(|languages| languages.len() <= 8)
            .ok_or_else(|| schema_error("/presentation/languages"))?;
        let mut seen = HashSet::new();
        for language in languages {
            let tag = language
                .as_str()
                .filter(|tag| language_tag_is_valid(tag))
                .ok_or_else(|| schema_error("/presentation/languages"))?;
            if !seen.insert(tag.to_ascii_lowercase()) {
                return Err(schema_error("/presentation/languages"));
            }
        }
    }
    if let Some(value) = object.get("cover") {
        let path = value
            .as_str()
            .filter(|path| valid_path_pattern(path) && safe_manifest_path(path))
            .ok_or_else(|| schema_error("/presentation/cover"))?;
        if path.is_empty() {
            return Err(schema_error("/presentation/cover"));
        }
    }
    Ok(())
}

fn parse_presentation(
    root: &Map<String, Value>,
    schema_version: &str,
) -> Result<Option<Presentation>, RejectReason> {
    let Some(value) = root.get("presentation") else {
        return Ok(None);
    };
    if !matches!(schema_version, "1.4.2" | "1.4.3") {
        return Err(RejectReason::unknown_field("presentation"));
    }
    validate_presentation(value)?;
    let object = value
        .as_object()
        .ok_or_else(|| schema_error("/presentation"))?;
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let author = object
        .get("author")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let languages = object
        .get("languages")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let cover = object
        .get("cover")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(Some(Presentation {
        description,
        author,
        languages,
        cover,
    }))
}

/// Validate the BCP-47 subtag grammar without depending on locale services.
pub fn language_tag_is_valid(tag: &str) -> bool {
    if tag.is_empty() || tag.len() > 63 || !tag.is_ascii() {
        return false;
    }
    let parts = tag.split('-').collect::<Vec<_>>();
    let Some(language) = parts.first() else {
        return false;
    };
    let alpha = |part: &str| part.bytes().all(|byte| byte.is_ascii_alphabetic());
    let alnum = |part: &str| part.bytes().all(|byte| byte.is_ascii_alphanumeric());
    if language.eq_ignore_ascii_case("x") {
        return parts.len() > 1
            && parts[1..]
                .iter()
                .all(|part| (1..=8).contains(&part.len()) && alnum(part));
    }
    let language_len = language.len();
    if !(2..=8).contains(&language_len) || !alpha(language) {
        return false;
    }
    let mut index = 1usize;
    if language_len <= 3 {
        let mut extlangs = 0;
        while index < parts.len() && parts[index].len() == 3 && alpha(parts[index]) && extlangs < 3
        {
            index += 1;
            extlangs += 1;
        }
    }
    if index < parts.len() && parts[index].len() == 4 && alpha(parts[index]) {
        index += 1;
    }
    if index < parts.len() {
        let region = parts[index];
        if (region.len() == 2 && alpha(region))
            || (region.len() == 3 && region.bytes().all(|byte| byte.is_ascii_digit()))
        {
            index += 1;
        }
    }
    let mut variants = HashSet::new();
    while index < parts.len() {
        let part = parts[index];
        let variant = ((5..=8).contains(&part.len()) && alnum(part))
            || (part.len() == 4 && part.as_bytes()[0].is_ascii_digit() && alnum(part));
        if !variant {
            break;
        }
        if !variants.insert(part.to_ascii_lowercase()) {
            return false;
        }
        index += 1;
    }
    let mut extensions = HashSet::new();
    while index < parts.len() {
        let singleton = parts[index];
        if singleton.len() != 1
            || !alnum(singleton)
            || singleton.eq_ignore_ascii_case("x")
            || !extensions.insert(singleton.to_ascii_lowercase())
        {
            break;
        }
        index += 1;
        let start = index;
        while index < parts.len() && (2..=8).contains(&parts[index].len()) && alnum(parts[index]) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    if index < parts.len() && parts[index].eq_ignore_ascii_case("x") {
        index += 1;
        let start = index;
        while index < parts.len() && (1..=8).contains(&parts[index].len()) && alnum(parts[index]) {
            index += 1;
        }
        if index == start {
            return false;
        }
    }
    index == parts.len()
}

fn validate_legacy_binding(object: &Map<String, Value>, pointer: &str) -> Result<(), RejectReason> {
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
    Ok(())
}

fn parse_selection(value: &str) -> Result<Selection, RejectReason> {
    match value {
        "random" => Ok(Selection::Random),
        "shuffle" => Ok(Selection::Shuffle),
        "sequence" => Ok(Selection::Sequence),
        _ => Err(RejectReason::SelectionInvalid),
    }
}

fn parse_variant_fields(
    object: &Map<String, Value>,
    pointer: &str,
    variant_id: String,
    clip_id: Option<String>,
    weight: Option<u8>,
    caption: Option<String>,
) -> Result<Variant, RejectReason> {
    let identity_audio_path = object
        .get("identity_audio_path")
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| schema_error(&format!("{pointer}/identity_audio_path")))
        })
        .transpose()?;
    let duration_ms = required_u64(object, "duration_ms", &format!("{pointer}/duration_ms"))?;
    let visual_tail_ms = required_u64(
        object,
        "visual_tail_ms",
        &format!("{pointer}/visual_tail_ms"),
    )?;
    Ok(Variant {
        variant_id,
        clip_id,
        caption,
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
        weight,
    })
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
            if let Some(variants) = binding.get("variants").and_then(Value::as_array) {
                for variant in variants {
                    for key in [
                        "mixed_audio_path",
                        "silent_visual_path",
                        "identity_audio_path",
                    ] {
                        if let Some(path) = variant.get(key).and_then(Value::as_str) {
                            if !path.contains(':') && has_unsafe_windows_segment(path) {
                                return Err(RejectReason::UnsafePath);
                            }
                        }
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

fn valid_hero_id(text: &str) -> bool {
    let Some(name) = text.strip_prefix("npc_dota_hero_") else {
        return false;
    };
    !name.is_empty()
        && text.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_variant_id(text: &str) -> bool {
    let mut bytes = text.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'0'..=b'9'))
        && text.len() <= 32
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_clip_id(text: &str) -> bool {
    if text.len() > 48 || !text.is_ascii() {
        return false;
    }
    let Some((prefix, suffix)) = text.split_once('-') else {
        return false;
    };
    let Some((language, number)) = suffix.split_once('-') else {
        return false;
    };
    !prefix.is_empty()
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && language.len() == 2
        && language.bytes().all(|byte| byte.is_ascii_lowercase())
        && number.len() == 4
        && number.bytes().all(|byte| byte.is_ascii_digit())
}

/// Captions are trimmed, single-line display text with 1 to 200 Unicode scalar
/// values. Authors should supply NFC text.
pub fn caption_is_valid(text: &str) -> bool {
    text == text.trim()
        && (1..=200).contains(&text.chars().count())
        && !text
            .chars()
            .any(|character| character.is_control() || matches!(character, '\u{2028}' | '\u{2029}'))
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
