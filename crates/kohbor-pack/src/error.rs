//! Specific rejection reasons. Every hostile input is rejected with exactly one
//! of these (Sprint 2 exit gate: "every hostile fixture is rejected with its
//! specific reason"). `as_str` is a frozen snake_case identifier used in local
//! records and tests; `contract_reason` maps it to a media-install-result
//! `reason` code.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    // ---- source / container ----
    SourceUnavailable,
    NotAZip,
    ContainerTooLarge,
    TooManyEntries,
    DuplicateEntry,
    UnsafePath,
    EncryptedEntry,
    UnsupportedCompression,
    UnexpectedExtraField,
    ArchiveComment,
    /// Local file header disagrees with the central directory.
    HeaderMismatch,
    /// Bytes before the first local header, between records, or after the end record.
    HiddenBytes,
    DescriptorNotFirst,
    EntryTooLarge,
    /// Expanded aggregate beyond the limit, or an entry that expands past its declared size.
    ExpansionLimit,
    // ---- descriptor ----
    DuplicateJsonKey,
    DescriptorSyntax,
    DescriptorSchema(String),
    UnknownField(String),
    TaxonomyMalformed,
    TaxonomyMajorUnsupported,
    TierNotAllowed,
    TierOutOfRange,
    TierDuplicate,
    BindingRoleMismatch,
    UnpairedBinding,
    DanglingReference,
    UndeclaredPayload,
    // ---- integrity ----
    SizeMismatch,
    DigestMismatch,
    /// The caller's expected container digest differs from the actual bytes.
    ContainerDigestMismatch,
    // ---- media profile ----
    MixedAudioInvalid,
    IdentityAudioInvalid,
    IdentityAudioLengthMismatch,
    AudioDurationMismatch,
    VisualInvalid,
    VisualHasAudioTrack,
    VisualDurationMismatch,
    // ---- identity ----
    RevisionConflict,
    UnknownInstall,
}

impl RejectReason {
    /// Frozen snake_case identifier (variant name in snake case; the payload of
    /// `DescriptorSchema` and `UnknownField` is not part of it).
    pub fn as_str(&self) -> &'static str {
        match self {
            RejectReason::SourceUnavailable => "source_unavailable",
            RejectReason::NotAZip => "not_a_zip",
            RejectReason::ContainerTooLarge => "container_too_large",
            RejectReason::TooManyEntries => "too_many_entries",
            RejectReason::DuplicateEntry => "duplicate_entry",
            RejectReason::UnsafePath => "unsafe_path",
            RejectReason::EncryptedEntry => "encrypted_entry",
            RejectReason::UnsupportedCompression => "unsupported_compression",
            RejectReason::UnexpectedExtraField => "unexpected_extra_field",
            RejectReason::ArchiveComment => "archive_comment",
            RejectReason::HeaderMismatch => "header_mismatch",
            RejectReason::HiddenBytes => "hidden_bytes",
            RejectReason::DescriptorNotFirst => "descriptor_not_first",
            RejectReason::EntryTooLarge => "entry_too_large",
            RejectReason::ExpansionLimit => "expansion_limit",
            RejectReason::DuplicateJsonKey => "duplicate_json_key",
            RejectReason::DescriptorSyntax => "descriptor_syntax",
            RejectReason::DescriptorSchema(_) => "descriptor_schema",
            RejectReason::UnknownField(_) => "unknown_field",
            RejectReason::TaxonomyMalformed => "taxonomy_malformed",
            RejectReason::TaxonomyMajorUnsupported => "taxonomy_major_unsupported",
            RejectReason::TierNotAllowed => "tier_not_allowed",
            RejectReason::TierOutOfRange => "tier_out_of_range",
            RejectReason::TierDuplicate => "tier_duplicate",
            RejectReason::BindingRoleMismatch => "binding_role_mismatch",
            RejectReason::UnpairedBinding => "unpaired_binding",
            RejectReason::DanglingReference => "dangling_reference",
            RejectReason::UndeclaredPayload => "undeclared_payload",
            RejectReason::SizeMismatch => "size_mismatch",
            RejectReason::DigestMismatch => "digest_mismatch",
            RejectReason::ContainerDigestMismatch => "container_digest_mismatch",
            RejectReason::MixedAudioInvalid => "mixed_audio_invalid",
            RejectReason::IdentityAudioInvalid => "identity_audio_invalid",
            RejectReason::IdentityAudioLengthMismatch => "identity_audio_length_mismatch",
            RejectReason::AudioDurationMismatch => "audio_duration_mismatch",
            RejectReason::VisualInvalid => "visual_invalid",
            RejectReason::VisualHasAudioTrack => "visual_has_audio_track",
            RejectReason::VisualDurationMismatch => "visual_duration_mismatch",
            RejectReason::RevisionConflict => "revision_conflict",
            RejectReason::UnknownInstall => "unknown_install",
        }
    }

    /// media-install-result `reason` code: `SOURCE_UNAVAILABLE` for
    /// `SourceUnavailable`; `SOURCE_INVALID` for `ContainerDigestMismatch`;
    /// `VERSION_UNSUPPORTED` for `TaxonomyMajorUnsupported`; `MEDIA_INVALID` for
    /// every other variant.
    pub fn contract_reason(&self) -> &'static str {
        match self {
            RejectReason::SourceUnavailable => "SOURCE_UNAVAILABLE",
            RejectReason::ContainerDigestMismatch => "SOURCE_INVALID",
            RejectReason::TaxonomyMajorUnsupported => "VERSION_UNSUPPORTED",
            _ => "MEDIA_INVALID",
        }
    }
}
