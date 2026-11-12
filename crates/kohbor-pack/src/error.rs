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
        todo!()
    }

    /// media-install-result `reason` code: `SOURCE_UNAVAILABLE` for
    /// `SourceUnavailable`; `SOURCE_INVALID` for `ContainerDigestMismatch`;
    /// `VERSION_UNSUPPORTED` for `TaxonomyMajorUnsupported`; `MEDIA_INVALID` for
    /// every other variant.
    pub fn contract_reason(&self) -> &'static str {
        todo!()
    }
}
