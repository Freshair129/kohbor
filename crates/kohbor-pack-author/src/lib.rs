mod archive;
pub mod audio;

use archive::{write_zip, ArchiveEntry};
use kohbor_pack::{
    catalog::Catalog,
    container::stage_package,
    descriptor::parse_descriptor,
    media::{inspect_cover, inspect_webm},
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
const MAX_PAYLOAD_COUNT: usize = 2_048;
const MAX_BINDINGS: usize = 128;
const MAX_VARIANTS_PER_BINDING: usize = 16;
const MAX_VARIANTS_PER_PACK: usize = 512;
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
    /// Optional command-line presentation overrides. Mapping values remain defaults.
    pub presentation: Option<PresentationOptions>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresentationOptions {
    pub description: Option<String>,
    pub author: Option<String>,
    pub languages: Option<Vec<String>>,
    /// Path relative to `clips_dir`.
    pub cover: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenBinding {
    pub event_id: String,
    pub critical_media_required: bool,
    pub tier: Option<u32>,
    pub variant_id: Option<String>,
    pub clip_id: Option<String>,
    pub selection: Option<String>,
    pub duration_ms: u32,
    pub audio: String,
    pub audio_facts: audio::AudioFacts,
    /// Package-relative visual path, or `None` for an audio-only variant.
    pub visual: Option<String>,
    pub identity_audio: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedBinding {
    pub event_id: String,
    pub tier: Option<u32>,
    pub variant_id: Option<String>,
    pub clip_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildReport {
    pub package_path: PathBuf,
    pub provenance_path: PathBuf,
    pub container_sha256: String,
    pub descriptor_schema_version: String,
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
struct MappingPlan {
    bindings: Vec<MappingBinding>,
    has_variants: bool,
    has_captions: bool,
    presentation: PresentationOptions,
}

#[derive(Debug, Clone)]
struct MappingBinding {
    event_id: String,
    tier: Option<u32>,
    selection: Option<String>,
    variants: Vec<MappingVariant>,
}

#[derive(Debug, Clone)]
struct MappingVariant {
    audio: String,
    /// Missing means this variant is audio-only unless the CLI requires banners.
    visual: Option<String>,
    identity_audio: Option<String>,
    /// Must be zero for audio-only variants.
    visual_tail_ms: u32,
    variant_id: Option<String>,
    clip_id: Option<String>,
    caption: Option<String>,
    weight: Option<u8>,
}

struct PreparedVariant {
    descriptor: Map<String, Value>,
    written: WrittenBinding,
}

enum VariantBuildResult {
    Written(PreparedVariant),
    Skipped(SkippedBinding),
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

struct VariantBuildState<'a> {
    receipts: &'a mut BTreeMap<(String, String), InputReceipt>,
    payloads: &'a mut Vec<Payload>,
    expanded_payload_bytes: &'a mut u64,
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
    let mut mapping_plan = parse_mapping(&mapping_value)?;
    if let Some(override_presentation) = options.presentation.as_ref() {
        if override_presentation.description.is_some() {
            mapping_plan.presentation.description = override_presentation.description.clone();
        }
        if override_presentation.author.is_some() {
            mapping_plan.presentation.author = override_presentation.author.clone();
        }
        if override_presentation.languages.is_some() {
            mapping_plan.presentation.languages = override_presentation.languages.clone();
        }
        if override_presentation.cover.is_some() {
            mapping_plan.presentation.cover = override_presentation.cover.clone();
        }
    }
    validate_presentation_options(&mapping_plan.presentation)?;
    let has_presentation_input = mapping_plan.presentation.description.is_some()
        || mapping_plan.presentation.author.is_some()
        || mapping_plan.presentation.languages.is_some()
        || mapping_plan.presentation.cover.is_some();
    let use_v14_output = mapping_plan.has_variants || has_presentation_input;
    if use_v14_output {
        assign_implicit_variant_ids(&mut mapping_plan.bindings)?;
    }

    let (catalog_bytes, catalog_receipt) = read_named_input(&options.catalog_path, "catalog")?;
    let catalog_value = kohbor_gsi::strict_json::parse(&catalog_bytes)
        .map_err(|error| BuildError::new(1, format!("catalog JSON is invalid: {error:?}")))?;
    let catalog = Catalog::from_json(&catalog_value)
        .ok_or_else(|| BuildError::new(1, "event catalog has an invalid shape"))?;

    let mut receipts = BTreeMap::<(String, String), InputReceipt>::new();
    insert_receipt(&mut receipts, mapping_receipt);
    insert_receipt(&mut receipts, catalog_receipt);
    let mut seen = BTreeSet::new();
    for mapping in &mapping_plan.bindings {
        let event = catalog
            .event(&mapping.event_id)
            .ok_or_else(|| BuildError::new(1, format!("unknown event id: {}", mapping.event_id)))?;
        if !event.media_mappable {
            return Err(BuildError::new(
                1,
                format!("event is not media-mappable: {}", mapping.event_id),
            ));
        }
        if event.critical_media_required && mapping.variants.len() != 1 {
            return Err(BuildError::new(
                1,
                format!(
                    "critical media event must have exactly one variant: {}",
                    mapping.event_id
                ),
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
    let mut variant_state = VariantBuildState {
        receipts: &mut receipts,
        payloads: &mut payloads,
        expanded_payload_bytes: &mut expanded_payload_bytes,
    };

    for mapping in &mapping_plan.bindings {
        let critical_media_required = catalog
            .event(&mapping.event_id)
            .ok_or_else(|| BuildError::new(1, format!("unknown event id: {}", mapping.event_id)))?
            .critical_media_required;
        let binding_key = binding_key(mapping);
        let mut prepared = Vec::with_capacity(mapping.variants.len());
        for variant in &mapping.variants {
            match prepare_variant(
                &clips_dir,
                mapping,
                variant,
                &binding_key,
                use_v14_output,
                options.require_visual,
                critical_media_required,
                &mut variant_state,
            )? {
                VariantBuildResult::Written(variant) => prepared.push(variant),
                VariantBuildResult::Skipped(reason) => skipped.push(reason),
            }
        }

        if use_v14_output {
            if prepared.is_empty() {
                continue;
            }
            let selection = if prepared.len() > 1 {
                mapping.selection.clone()
            } else {
                prepared[0].descriptor.remove("weight");
                None
            };
            let mut descriptor_binding = Map::new();
            descriptor_binding.insert("event_id".to_owned(), json!(mapping.event_id));
            if let Some(tier) = mapping.tier {
                descriptor_binding.insert("tier".to_owned(), json!(tier));
            }
            if let Some(selection) = selection.as_ref() {
                descriptor_binding.insert("selection".to_owned(), json!(selection));
            }
            descriptor_binding.insert(
                "variants".to_owned(),
                Value::Array(
                    prepared
                        .iter()
                        .map(|variant| Value::Object(variant.descriptor.clone()))
                        .collect(),
                ),
            );
            manifest_bindings.push(Value::Object(descriptor_binding));
            for variant in prepared {
                let mut written_variant = variant.written;
                written_variant.selection = selection.clone();
                written.push(written_variant);
            }
        } else if let Some(variant) = prepared.into_iter().next() {
            let mut descriptor_binding = variant.descriptor;
            descriptor_binding.insert("event_id".to_owned(), json!(mapping.event_id));
            if let Some(tier) = mapping.tier {
                descriptor_binding.insert("tier".to_owned(), json!(tier));
            }
            manifest_bindings.push(Value::Object(descriptor_binding));
            written.push(variant.written);
        }
    }

    let mut presentation_descriptor = Map::new();
    if let Some(description) = mapping_plan.presentation.description.as_deref() {
        presentation_descriptor.insert("description".to_owned(), json!(description));
    }
    if let Some(author) = mapping_plan.presentation.author.as_deref() {
        presentation_descriptor.insert("author".to_owned(), json!(author));
    }
    if let Some(languages) = mapping_plan.presentation.languages.as_ref() {
        presentation_descriptor.insert("languages".to_owned(), json!(languages));
    }
    if let Some(cover_source) = mapping_plan.presentation.cover.as_deref() {
        let bytes = read_cover(&clips_dir, cover_source)?;
        let cover_info = inspect_cover(&bytes).map_err(|_| {
            BuildError::new(
                1,
                "cover must be a still PNG or WebP no larger than 1 MiB and 1920x1080",
            )
        })?;
        add_clip_receipt(&mut receipts, cover_source, &bytes)?;
        let extension = if cover_info.content_type == "image/png" {
            "png"
        } else {
            "webp"
        };
        let cover_path = format!("cover/{}.{}", sha256_hex(&bytes), extension);
        let added = append_payload(&mut payloads, cover_path.clone(), bytes, "cover", 0, true)?;
        expanded_payload_bytes = expanded_payload_bytes
            .checked_add(added)
            .ok_or_else(|| BuildError::new(1, "expanded payload size overflow"))?;
        if expanded_payload_bytes > MAX_EXPANDED_BYTES {
            return Err(BuildError::new(
                1,
                "expanded payloads exceed the package 512 MiB limit",
            ));
        }
        presentation_descriptor.insert("cover".to_owned(), json!(cover_path));
    }
    let has_presentation = !presentation_descriptor.is_empty();

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
    let descriptor_schema_version = if has_presentation {
        "1.4.2"
    } else if mapping_plan.has_captions {
        "1.4.1"
    } else if mapping_plan.has_variants {
        "1.4.0"
    } else {
        "1.3.0"
    };
    let mut descriptor = json!({
        "schema_version": descriptor_schema_version,
        "package_id": options.package_id,
        "revision": options.revision,
        "taxonomy_version": taxonomy_version,
        "display_name": options.display_name,
        "publisher_key_id": options.publisher_key_id,
        "files": file_inventory,
        "bindings": manifest_bindings,
    });
    if has_presentation {
        descriptor["presentation"] = Value::Object(presentation_descriptor);
    }
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
    let mut build_report = json!({
        "written": written
            .iter()
            .map(written_provenance_json)
            .collect::<Vec<_>>(),
        "skipped": skipped.iter().map(skipped_report_json).collect::<Vec<_>>(),
    });
    if use_v14_output {
        build_report["descriptor_schema_version"] = json!(descriptor_schema_version);
    }
    let provenance = json!({
        "schema_version": "1.0.0",
        "tool": { "name": "kohbor-pack-build", "version": env!("CARGO_PKG_VERSION") },
        "build_report": build_report,
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
        descriptor_schema_version: descriptor_schema_version.to_owned(),
        written,
        skipped,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_variant(
    clips_dir: &Path,
    mapping: &MappingBinding,
    variant: &MappingVariant,
    binding_key: &str,
    v14_output: bool,
    require_visual: bool,
    critical_media_required: bool,
    state: &mut VariantBuildState<'_>,
) -> Result<VariantBuildResult, BuildError> {
    if variant.visual.is_none() && variant.visual_tail_ms != 0 {
        return Ok(VariantBuildResult::Skipped(skip_for(
            mapping,
            variant,
            if v14_output {
                "audio-only variant must have visual_tail_ms 0"
            } else {
                "audio-only binding must have visual_tail_ms 0"
            },
        )));
    }
    if require_visual && variant.visual.is_none() {
        return Ok(VariantBuildResult::Skipped(skip_for(
            mapping,
            variant,
            "visual path is missing (--require-visual)",
        )));
    }

    let visual_bytes = if let Some(visual_path) = variant.visual.as_deref() {
        let bytes = match read_clip(clips_dir, visual_path) {
            Ok(bytes) => bytes,
            Err(ClipReadError::TooLarge) => {
                return Ok(VariantBuildResult::Skipped(skip_for(
                    mapping,
                    variant,
                    "visual input exceeds 64 MiB",
                )))
            }
            Err(ClipReadError::Unavailable) => {
                return Ok(VariantBuildResult::Skipped(skip_for(
                    mapping,
                    variant,
                    "visual input is unavailable",
                )))
            }
            Err(ClipReadError::UnsafePath) => {
                return Ok(VariantBuildResult::Skipped(skip_for(
                    mapping,
                    variant,
                    "visual path is not a safe clip path",
                )))
            }
        };
        add_clip_receipt(state.receipts, visual_path, &bytes)?;
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Ok(VariantBuildResult::Skipped(skip_for(
                mapping,
                variant,
                "visual exceeds the package 32 MiB payload limit",
            )));
        }
        if let Err(reason) = inspect_webm(&bytes) {
            return Ok(VariantBuildResult::Skipped(skip_for(
                mapping,
                variant,
                format!("visual failed silent WebM inspection: {}", reason.as_str()),
            )));
        }
        Some(bytes)
    } else {
        None
    };

    let audio_bytes = match read_clip(clips_dir, &variant.audio) {
        Ok(bytes) => bytes,
        Err(ClipReadError::TooLarge) => {
            return Ok(VariantBuildResult::Skipped(skip_for(
                mapping,
                variant,
                "audio input exceeds 64 MiB",
            )))
        }
        Err(ClipReadError::Unavailable) => {
            return Err(BuildError::new(
                1,
                format!(
                    "audio input is unavailable for {}",
                    variant_context(mapping, variant)
                ),
            ))
        }
        Err(ClipReadError::UnsafePath) => {
            return Err(BuildError::new(
                1,
                format!(
                    "audio path is not a safe clip path for {}",
                    variant_context(mapping, variant)
                ),
            ))
        }
    };
    add_clip_receipt(state.receipts, &variant.audio, &audio_bytes)?;
    let mixed = audio::convert_wav(&audio_bytes).map_err(|error| {
        BuildError::new(
            1,
            format!(
                "audio conversion failed for {}: {error}",
                variant_context(mapping, variant)
            ),
        )
    })?;
    let audio_facts = mixed.audio_facts;
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
                variant_context(mapping, variant)
            ),
        ));
    }

    let identity_payload = if let Some(identity_path) = variant.identity_audio.as_deref() {
        let identity_bytes = match read_clip(clips_dir, identity_path) {
            Ok(bytes) => bytes,
            Err(ClipReadError::TooLarge) => {
                return Ok(VariantBuildResult::Skipped(skip_for(
                    mapping,
                    variant,
                    "identity audio input exceeds 64 MiB",
                )))
            }
            Err(ClipReadError::Unavailable) => {
                return Err(BuildError::new(
                    1,
                    format!(
                        "identity audio is unavailable for {}",
                        variant_context(mapping, variant)
                    ),
                ))
            }
            Err(ClipReadError::UnsafePath) => {
                return Err(BuildError::new(
                    1,
                    format!(
                        "identity audio path is unsafe for {}",
                        variant_context(mapping, variant)
                    ),
                ))
            }
        };
        add_clip_receipt(state.receipts, identity_path, &identity_bytes)?;
        let identity = audio::convert_wav(&identity_bytes).map_err(|error| {
            BuildError::new(
                1,
                format!(
                    "identity audio conversion failed for {}: {error}",
                    variant_context(mapping, variant)
                ),
            )
        })?;
        if identity.samples != mixed.samples {
            return Err(BuildError::new(
                1,
                format!(
                    "identity audio sample count differs for {}",
                    variant_context(mapping, variant)
                ),
            ));
        }
        Some(identity.wav)
    } else {
        None
    };

    let mixed_path = if v14_output {
        format!("audio/{}.wav", sha256_hex(&mixed.wav))
    } else {
        format!("audio/{binding_key}.wav")
    };
    let visual_output_path = visual_bytes.as_ref().map(|bytes| {
        if v14_output {
            format!("video/{}.webm", sha256_hex(bytes))
        } else {
            format!("video/{binding_key}.webm")
        }
    });
    let identity_output_path = identity_payload.as_ref().map(|bytes| {
        if v14_output {
            format!("identity/{}.wav", sha256_hex(bytes))
        } else {
            format!("audio/{binding_key}-identity.wav")
        }
    });

    let mut add_generated_payload =
        |path: String, bytes: Vec<u8>, role: &'static str, method: u16| -> Result<(), BuildError> {
            let added = append_payload(state.payloads, path, bytes, role, method, v14_output)?;
            *state.expanded_payload_bytes = (*state.expanded_payload_bytes)
                .checked_add(added)
                .ok_or_else(|| BuildError::new(1, "expanded package size overflow"))?;
            if *state.expanded_payload_bytes > MAX_EXPANDED_BYTES {
                return Err(BuildError::new(
                    1,
                    "expanded payloads exceed the package 512 MiB limit",
                ));
            }
            Ok(())
        };
    add_generated_payload(mixed_path.clone(), mixed.wav, "mixed_audio", 8)?;
    if let (Some(path), Some(bytes)) = (visual_output_path.as_ref(), visual_bytes) {
        add_generated_payload(path.clone(), bytes, "silent_visual", 0)?;
    }
    if let (Some(path), Some(bytes)) = (identity_output_path.as_ref(), identity_payload) {
        add_generated_payload(path.clone(), bytes, "identity_audio", 8)?;
    }

    let mut descriptor = Map::new();
    if v14_output {
        let variant_id = variant
            .variant_id
            .as_ref()
            .expect("1.4.x output has assigned variant IDs");
        descriptor.insert("variant_id".to_owned(), json!(variant_id));
        if let Some(clip_id) = variant.clip_id.as_ref() {
            descriptor.insert("clip_id".to_owned(), json!(clip_id));
        }
        if let Some(caption) = variant.caption.as_ref() {
            descriptor.insert("caption".to_owned(), json!(caption));
        }
        if let Some(weight) = variant.weight {
            descriptor.insert("weight".to_owned(), json!(weight));
        }
    }
    descriptor.insert("mixed_audio_path".to_owned(), json!(mixed_path));
    if let Some(visual_path) = visual_output_path.as_ref() {
        descriptor.insert("silent_visual_path".to_owned(), json!(visual_path));
    }
    descriptor.insert("duration_ms".to_owned(), json!(duration_ms));
    descriptor.insert("visual_tail_ms".to_owned(), json!(variant.visual_tail_ms));
    if let Some(identity_path) = identity_output_path.as_ref() {
        descriptor.insert("identity_audio_path".to_owned(), json!(identity_path));
    }

    Ok(VariantBuildResult::Written(PreparedVariant {
        descriptor,
        written: WrittenBinding {
            event_id: mapping.event_id.clone(),
            critical_media_required,
            tier: mapping.tier,
            variant_id: if v14_output {
                variant.variant_id.clone()
            } else {
                None
            },
            clip_id: variant.clip_id.clone(),
            selection: None,
            duration_ms,
            audio: mixed_path,
            audio_facts,
            visual: visual_output_path,
            identity_audio: identity_output_path,
        },
    }))
}

fn append_payload(
    payloads: &mut Vec<Payload>,
    path: String,
    bytes: Vec<u8>,
    role: &'static str,
    method: u16,
    deduplicate: bool,
) -> Result<u64, BuildError> {
    if bytes.is_empty() || bytes.len() > MAX_PAYLOAD_BYTES {
        return Err(BuildError::new(
            1,
            "generated payload violates the manifest size limit",
        ));
    }
    if let Some(existing) = payloads.iter().find(|payload| payload.path == path) {
        if deduplicate && existing.role == role && existing.bytes == bytes {
            return Ok(0);
        }
        return Err(BuildError::new(
            1,
            "generated payload path collides with different content",
        ));
    }
    payloads.push(Payload {
        path,
        bytes,
        role,
        method,
    });
    if payloads.len() > MAX_PAYLOAD_COUNT {
        return Err(BuildError::new(
            1,
            "package has more than 2048 payload files",
        ));
    }
    u64::try_from(payloads.last().map_or(0, |payload| payload.bytes.len()))
        .map_err(|_| BuildError::new(1, "generated payload size is too large"))
}

fn variant_context(mapping: &MappingBinding, variant: &MappingVariant) -> String {
    match variant.variant_id.as_deref() {
        Some(variant_id) => format!("{} variant {variant_id}", mapping.event_id),
        None => mapping.event_id.clone(),
    }
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

fn parse_mapping(value: &Value) -> Result<MappingPlan, BuildError> {
    let root = value
        .as_object()
        .ok_or_else(|| BuildError::new(1, "mapping root must be an object"))?;
    if root
        .keys()
        .any(|key| key != "schema_version" && key != "bindings" && key != "presentation")
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
    let presentation = parse_presentation_mapping(root.get("presentation"))?;
    if bindings.len() > MAX_BINDINGS {
        return Err(BuildError::new(1, "mapping has more than 128 bindings"));
    }
    let mut result = Vec::with_capacity(bindings.len());
    let mut has_variants = false;
    let mut has_captions = false;
    let mut total_variants = 0usize;
    let mut explicit_variant_ids = BTreeSet::new();
    for value in bindings {
        let object = value
            .as_object()
            .ok_or_else(|| BuildError::new(1, "mapping binding must be an object"))?;
        let event_id = required_string(object, "event_id")?;
        let tier = optional_u32(object, "tier")?;
        if tier.is_some_and(|value| value == 0 || value > 100) {
            return Err(BuildError::new(1, "tier must be between 1 and 100"));
        }
        let (selection, variants) = if object.contains_key("variants") {
            has_variants = true;
            reject_mapping_fields(object, &["event_id", "tier", "selection", "variants"])?;
            let variant_values = object
                .get("variants")
                .and_then(Value::as_array)
                .ok_or_else(|| BuildError::new(1, "mapping variants must be an array"))?;
            if variant_values.is_empty() || variant_values.len() > MAX_VARIANTS_PER_BINDING {
                return Err(BuildError::new(
                    1,
                    "mapping binding must have 1 to 16 variants",
                ));
            }
            let selection = parse_mapping_selection(object)?;
            if variant_values.len() > 1 && selection.is_none() {
                return Err(BuildError::new(
                    1,
                    "mapping selection is required with two or more variants",
                ));
            }
            if variant_values.len() == 1 && selection.is_some() {
                return Err(BuildError::new(
                    1,
                    "mapping selection must be absent with one variant",
                ));
            }
            let mut variants = Vec::with_capacity(variant_values.len());
            for variant_value in variant_values {
                let variant_object = variant_value
                    .as_object()
                    .ok_or_else(|| BuildError::new(1, "mapping variant must be an object"))?;
                reject_mapping_fields(
                    variant_object,
                    &[
                        "audio",
                        "visual",
                        "visual_tail_ms",
                        "identity_audio",
                        "clip_id",
                        "variant_id",
                        "weight",
                        "caption",
                    ],
                )?;
                let audio = required_string(variant_object, "audio")?;
                validate_clip_path(&audio).map_err(|_| {
                    BuildError::new(1, "audio path is not a safe relative clip path")
                })?;
                let visual = optional_string(variant_object, "visual")?;
                if let Some(path) = visual.as_deref() {
                    validate_clip_path(path).map_err(|_| {
                        BuildError::new(1, "visual path is not a safe relative clip path")
                    })?;
                }
                let identity_audio = optional_string(variant_object, "identity_audio")?;
                if let Some(path) = identity_audio.as_deref() {
                    validate_clip_path(path).map_err(|_| {
                        BuildError::new(1, "identity audio path is not a safe relative clip path")
                    })?;
                }
                let visual_tail_ms = optional_u32(variant_object, "visual_tail_ms")?.unwrap_or(0);
                if visual_tail_ms > 3_200 {
                    return Err(BuildError::new(
                        1,
                        "visual_tail_ms must be between 0 and 3200",
                    ));
                }
                let variant_id = optional_string(variant_object, "variant_id")?;
                if variant_id
                    .as_deref()
                    .is_some_and(|id| !valid_variant_id(id))
                {
                    return Err(BuildError::new(
                        1,
                        "variant_id does not match the descriptor pattern",
                    ));
                }
                if let Some(variant_id) = variant_id.as_ref() {
                    if !explicit_variant_ids.insert(variant_id.clone()) {
                        return Err(BuildError::new(1, "variant_id must be unique in the pack"));
                    }
                }
                let clip_id = optional_string(variant_object, "clip_id")?;
                if clip_id.as_deref().is_some_and(|id| !valid_clip_id(id)) {
                    return Err(BuildError::new(
                        1,
                        "clip_id does not match the descriptor pattern",
                    ));
                }
                let caption = optional_string(variant_object, "caption")?;
                if caption
                    .as_deref()
                    .is_some_and(|text| !kohbor_pack::descriptor::caption_is_valid(text))
                {
                    return Err(BuildError::new(
                        1,
                        "caption must be trimmed single-line text with 1 to 200 Unicode scalar values and no control characters",
                    ));
                }
                has_captions |= caption.is_some();
                let weight = optional_u32(variant_object, "weight")?;
                if weight.is_some_and(|value| !(1..=100).contains(&value)) {
                    return Err(BuildError::new(1, "weight must be between 1 and 100"));
                }
                if weight.is_some() && selection.as_deref() != Some("random") {
                    return Err(BuildError::new(
                        1,
                        "weight is allowed only with random selection",
                    ));
                }
                variants.push(MappingVariant {
                    audio,
                    visual,
                    identity_audio,
                    visual_tail_ms,
                    variant_id,
                    clip_id,
                    caption,
                    weight: weight.map(|weight| weight as u8),
                });
            }
            (selection, variants)
        } else {
            reject_mapping_fields(
                object,
                &[
                    "event_id",
                    "tier",
                    "audio",
                    "visual",
                    "identity_audio",
                    "visual_tail_ms",
                ],
            )?;
            let audio = required_string(object, "audio")?;
            validate_clip_path(&audio)
                .map_err(|_| BuildError::new(1, "audio path is not a safe relative clip path"))?;
            let visual = optional_string(object, "visual")?;
            if let Some(path) = visual.as_deref() {
                validate_clip_path(path).map_err(|_| {
                    BuildError::new(1, "visual path is not a safe relative clip path")
                })?;
            }
            let identity_audio = optional_string(object, "identity_audio")?;
            if let Some(path) = identity_audio.as_deref() {
                validate_clip_path(path).map_err(|_| {
                    BuildError::new(1, "identity audio path is not a safe relative clip path")
                })?;
            }
            let visual_tail_ms = optional_u32(object, "visual_tail_ms")?.unwrap_or(0);
            if visual_tail_ms > 3_200 {
                return Err(BuildError::new(
                    1,
                    "visual_tail_ms must be between 0 and 3200",
                ));
            }
            (
                None,
                vec![MappingVariant {
                    audio,
                    visual,
                    identity_audio,
                    visual_tail_ms,
                    variant_id: None,
                    clip_id: None,
                    caption: None,
                    weight: None,
                }],
            )
        };
        total_variants = total_variants
            .checked_add(variants.len())
            .ok_or_else(|| BuildError::new(1, "mapping has more than 512 variants"))?;
        if total_variants > MAX_VARIANTS_PER_PACK {
            return Err(BuildError::new(1, "mapping has more than 512 variants"));
        }
        result.push(MappingBinding {
            event_id,
            tier,
            selection,
            variants,
        });
    }
    Ok(MappingPlan {
        bindings: result,
        has_variants,
        has_captions,
        presentation,
    })
}

fn parse_presentation_mapping(value: Option<&Value>) -> Result<PresentationOptions, BuildError> {
    let Some(value) = value else {
        return Ok(PresentationOptions::default());
    };
    let object = value
        .as_object()
        .ok_or_else(|| BuildError::new(1, "mapping presentation must be an object"))?;
    reject_mapping_fields(object, &["description", "author", "languages", "cover"])?;
    let description = mapping_optional_text(object, "description")?;
    let author = mapping_optional_text(object, "author")?;
    let languages = match object.get("languages") {
        None => None,
        Some(Value::Array(values)) => Some(
            values
                .iter()
                .map(|value| {
                    value.as_str().map(str::to_owned).ok_or_else(|| {
                        BuildError::new(1, "presentation languages must be BCP-47 strings")
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => {
            return Err(BuildError::new(
                1,
                "presentation languages must be an array",
            ))
        }
    };
    let cover = optional_string(object, "cover")?;
    if let Some(path) = cover.as_deref() {
        validate_clip_path(path).map_err(|_| {
            BuildError::new(
                1,
                "presentation cover path must be a safe relative clip path",
            )
        })?;
    }
    let presentation = PresentationOptions {
        description,
        author,
        languages,
        cover,
    };
    validate_presentation_options(&presentation)?;
    Ok(presentation)
}

fn mapping_optional_text(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<String>, BuildError> {
    match object.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(BuildError::new(
            1,
            format!("presentation {key} must be a string"),
        )),
    }
}

fn validate_presentation_options(value: &PresentationOptions) -> Result<(), BuildError> {
    for (field, text, maximum) in [
        ("description", value.description.as_deref(), 500usize),
        ("author", value.author.as_deref(), 80usize),
    ] {
        if text.is_some_and(|text| {
            text.chars().count() > maximum
                || text.chars().any(|character| {
                    character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
                })
        }) {
            return Err(BuildError::new(
                1,
                format!(
                    "presentation {field} exceeds its length limit or contains control characters"
                ),
            ));
        }
    }
    if let Some(languages) = value.languages.as_ref() {
        if languages.len() > 8 {
            return Err(BuildError::new(
                1,
                "presentation languages must contain at most 8 tags",
            ));
        }
        let mut seen = BTreeSet::new();
        for language in languages {
            if !kohbor_pack::descriptor::language_tag_is_valid(language)
                || !seen.insert(language.to_ascii_lowercase())
            {
                return Err(BuildError::new(
                    1,
                    "presentation languages must be unique BCP-47 tags",
                ));
            }
        }
    }
    if value
        .cover
        .as_deref()
        .is_some_and(|path| validate_clip_path(path).is_err())
    {
        return Err(BuildError::new(
            1,
            "presentation cover path must be a safe relative clip path",
        ));
    }
    Ok(())
}

fn reject_mapping_fields(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), BuildError> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(BuildError::new(
            1,
            "mapping object contains an unknown field",
        ));
    }
    Ok(())
}

fn parse_mapping_selection(object: &Map<String, Value>) -> Result<Option<String>, BuildError> {
    match object.get("selection") {
        None => Ok(None),
        Some(Value::String(selection))
            if matches!(selection.as_str(), "random" | "shuffle" | "sequence") =>
        {
            Ok(Some(selection.clone()))
        }
        Some(_) => Err(BuildError::new(
            1,
            "selection must be random, shuffle, or sequence",
        )),
    }
}

fn assign_implicit_variant_ids(bindings: &mut [MappingBinding]) -> Result<(), BuildError> {
    let mut used = BTreeSet::new();
    for variant in bindings.iter().flat_map(|binding| binding.variants.iter()) {
        if let Some(variant_id) = variant.variant_id.as_ref() {
            if !used.insert(variant_id.clone()) {
                return Err(BuildError::new(1, "variant_id must be unique in the pack"));
            }
        }
    }
    let mut next_id = 1usize;
    for variant in bindings
        .iter_mut()
        .flat_map(|binding| binding.variants.iter_mut())
    {
        if variant.variant_id.is_some() {
            continue;
        }
        loop {
            let candidate = format!("v{next_id}");
            next_id = next_id.saturating_add(1);
            if used.insert(candidate.clone()) {
                variant.variant_id = Some(candidate);
                break;
            }
            if next_id == usize::MAX {
                return Err(BuildError::new(1, "could not derive a unique variant_id"));
            }
        }
    }
    Ok(())
}

fn valid_variant_id(value: &str) -> bool {
    let mut bytes = value.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'0'..=b'9'))
        && value.len() <= 32
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_clip_id(value: &str) -> bool {
    if value.len() > 48 || !value.is_ascii() {
        return false;
    }
    let Some((prefix, suffix)) = value.split_once('-') else {
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

fn read_cover(root: &Path, relative: &str) -> Result<Vec<u8>, BuildError> {
    let relative_path = validate_clip_path(relative)
        .map_err(|_| BuildError::new(1, "presentation cover path is unsafe"))?;
    let candidate = root.join(relative_path);
    let canonical = fs::canonicalize(&candidate)
        .map_err(|_| BuildError::new(1, "presentation cover is unavailable"))?;
    if !canonical.starts_with(root) {
        return Err(BuildError::new(1, "presentation cover path is unsafe"));
    }
    let metadata = fs::metadata(&canonical)
        .map_err(|_| BuildError::new(1, "presentation cover is unavailable"))?;
    if !metadata.is_file() || metadata.len() > 1_048_576 {
        return Err(BuildError::new(
            1,
            "presentation cover exceeds 1 MiB or is not a file",
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(canonical)
        .map_err(|_| BuildError::new(1, "presentation cover is unavailable"))?
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| BuildError::new(1, "presentation cover could not be read"))?;
    if bytes.is_empty() || bytes.len() > 1_048_576 {
        return Err(BuildError::new(1, "presentation cover exceeds 1 MiB"));
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

fn skip_for(
    mapping: &MappingBinding,
    variant: &MappingVariant,
    reason: impl Into<String>,
) -> SkippedBinding {
    SkippedBinding {
        event_id: mapping.event_id.clone(),
        tier: mapping.tier,
        variant_id: variant.variant_id.clone(),
        clip_id: variant.clip_id.clone(),
        reason: reason.into(),
    }
}

fn written_provenance_json(binding: &WrittenBinding) -> Value {
    let mut report = Map::new();
    report.insert("event_id".to_owned(), json!(binding.event_id));
    report.insert("tier".to_owned(), json!(binding.tier));
    report.insert("duration_ms".to_owned(), json!(binding.duration_ms));
    report.insert("visual".to_owned(), json!(binding.visual));
    if binding.variant_id.is_some() {
        report.insert("variant_id".to_owned(), json!(binding.variant_id));
        report.insert("audio".to_owned(), json!(binding.audio));
        report.insert("selection".to_owned(), json!(binding.selection));
        report.insert("identity_audio".to_owned(), json!(binding.identity_audio));
    }
    if let Some(clip_id) = binding.clip_id.as_ref() {
        report.insert("clip_id".to_owned(), json!(clip_id));
    }
    Value::Object(report)
}

fn written_report_json(binding: &WrittenBinding) -> Value {
    let mut report = written_provenance_json(binding);
    let report_object = report
        .as_object_mut()
        .expect("written provenance record is a JSON object");
    report_object.insert(
        "audio_facts".to_owned(),
        json!({
            "onset_ms": binding.audio_facts.onset_ms,
            "tail_ms": binding.audio_facts.tail_ms,
            "peak_dbfs_tenths": binding.audio_facts.peak_dbfs_tenths,
            "active_rms_dbfs_tenths": binding.audio_facts.active_rms_dbfs_tenths,
        }),
    );
    report_object.insert(
        "critical_media_required".to_owned(),
        json!(binding.critical_media_required),
    );
    report
}

fn skipped_report_json(binding: &SkippedBinding) -> Value {
    let mut report = Map::new();
    report.insert("event_id".to_owned(), json!(binding.event_id));
    report.insert("tier".to_owned(), json!(binding.tier));
    report.insert("reason".to_owned(), json!(binding.reason));
    if let Some(variant_id) = binding.variant_id.as_ref() {
        report.insert("variant_id".to_owned(), json!(variant_id));
    }
    if let Some(clip_id) = binding.clip_id.as_ref() {
        report.insert("clip_id".to_owned(), json!(clip_id));
    }
    Value::Object(report)
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
    let mut presentation = PresentationOptions::default();
    let mut has_presentation_override = false;
    let mut languages = Vec::new();
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
            "--description" => {
                if presentation.description.replace(value).is_some() {
                    return Err(BuildError::new(1, "duplicate option: --description"));
                }
                has_presentation_override = true;
            }
            "--author" => {
                if presentation.author.replace(value).is_some() {
                    return Err(BuildError::new(1, "duplicate option: --author"));
                }
                has_presentation_override = true;
            }
            "--cover" => {
                if presentation.cover.replace(value).is_some() {
                    return Err(BuildError::new(1, "duplicate option: --cover"));
                }
                has_presentation_override = true;
            }
            "--language" => {
                languages.push(value);
                has_presentation_override = true;
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
    if has_presentation_override && !languages.is_empty() {
        presentation.languages = Some(languages);
    }
    validate_presentation_options(&presentation)?;
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
        presentation: has_presentation_override.then_some(presentation),
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
    let mut output = json!({
        "package": package_name,
        "provenance": provenance_name,
        "container_sha256": report.container_sha256,
        "written": report.written.iter().map(written_report_json).collect::<Vec<_>>(),
        "skipped": report.skipped.iter().map(skipped_report_json).collect::<Vec<_>>(),
    });
    if matches!(
        report.descriptor_schema_version.as_str(),
        "1.4.0" | "1.4.1" | "1.4.2"
    ) {
        output["descriptor_schema_version"] = json!(report.descriptor_schema_version);
    }
    output
}

const CLI_HELP: &str = r#"kohbor-pack-build - build a deterministic local media package

Usage:
  kohbor-pack-build --clips DIR --map FILE --out DIR --package-id ID --revision N --display-name NAME --catalog FILE [--rights-claim TEXT] [--publisher-key-id ID] [--description TEXT] [--author TEXT] [--language BCP-47]... [--cover FILE] [--require-visual]

Mapping JSON keeps schema_version 1.0.0. Existing one-clip bindings remain valid:
  {"event_id":"hero_kill","audio":"kill.wav","visual":"kill.webm"}

To author several clips for one binding, use variants and choose random, shuffle,
or sequence when there are two or more variants. variant_id is assigned as v1,
v2, ... in listed pack order when omitted. clip_id is optional provenance.
  {"event_id":"hero_kill","selection":"shuffle","variants":[
    {"audio":"kill-01.wav","clip_id":"vrzo-th-0019"},
    {"audio":"kill-02.wav","clip_id":"vrzo-th-0024"}
  ]}

Each variant may also contain visual, visual_tail_ms, identity_audio, variant_id,
weight, clip_id, and an optional caption. weight is only valid for random
selection. Captions are trimmed single-line plain text of 1 to 200 Unicode
scalar values without control characters. Authors should supply NFC text.
Captions produce descriptor 1.4.1; variant mappings without captions keep
descriptor 1.4.0.
Mappings without variants keep descriptor 1.3.0.

For 1.4.x output, the JSON report includes descriptor_schema_version and lists
one written or skipped record per variant. Records include variant_id and any
provided clip_id. Written records also include selection and generated media paths.
Written records include audio_facts from packaged mono 48 kHz PCM16: onset is the first 480-sample
window with |sample| >= 328; tail counts whole or partial windows after the last active window.
Peak and active-window RMS are rounded dBFS tenths; no activity makes onset/tail/RMS null, and peak is null for an all-zero clip.
"#;

/// Run the CLI parser and builder. Returns the process exit code.
pub fn run_cli(args: impl IntoIterator<Item = std::ffi::OsString>) -> u8 {
    let args = args.into_iter().collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].to_str(), Some("--help" | "-h")) {
        println!("{CLI_HELP}");
        return 0;
    }
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
