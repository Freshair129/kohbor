//! Package descriptor (`descriptor.json`, media-manifest 1.2.0): strict parsing
//! plus the semantic rules of EVENT-AUDIO-SPEC section 7 and MP-03/MP-05.
//! Pure: bytes in, a typed descriptor or one specific [`RejectReason`] out.

use crate::catalog::{Catalog, TaxonomyVersion};
use crate::error::RejectReason;
use crate::limits::Limits;

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
    pub silent_visual_path: String,
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
/// 2. Shape per media-manifest 1.2.0 with `additionalProperties: false`: an
///    unknown key -> `UnknownField(key)` (this is how a self-declared
///    `trusted_public_key` or authority claim is rejected); any other schema
///    violation (type, pattern, range, counts, `schema_version != "1.2.0"`,
///    missing/null `taxonomy_version`) -> `DescriptorSchema(json_pointer)`;
///    a `taxonomy_version` string outside the canonical grammar ->
///    `TaxonomyMalformed`.
/// 3. Taxonomy: package major != runtime major -> `TaxonomyMajorUnsupported`.
/// 4. Paths: every path in `files` and every binding path must be relative,
///    ASCII, without `..`, `.`, empty, backslash, drive, `:` or leading `/`
///    segments -> `UnsafePath`; `files[].path` duplicates (case-insensitive)
///    -> `DuplicateEntry`.
/// 5. References: a binding path missing from `files` -> `DanglingReference`;
///    the role of the referenced file must be `mixed_audio` for
///    `mixed_audio_path`, `silent_visual` for `silent_visual_path` and
///    `identity_audio` for `identity_audio_path` -> `BindingRoleMismatch`.
/// 6. Tiers: `tier` on an event without catalog `tier_range` -> `TierNotAllowed`;
///    outside the range -> `TierOutOfRange`; two bindings with the same
///    `(event_id, tier)` (a missing tier counts as one tier) -> `TierDuplicate`;
///    a tiered event bound without any tier is allowed (the tier-less binding).
/// 7. Pairing: every binding names both a mixed audio path and a silent visual
///    path (schema-required); a `files` entry with role `mixed_audio` or
///    `silent_visual` that no binding uses -> `UnpairedBinding`.
///
/// Unknown (or not media-mappable) `event_id`s are not an error: they are
/// listed in `inert_event_ids` and skipped by checks 6 and the playback index.
pub fn parse_descriptor(
    bytes: &[u8],
    catalog: &Catalog,
    limits: &Limits,
) -> Result<ParsedDescriptor, RejectReason> {
    todo!()
}
