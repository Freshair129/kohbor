//! Package cache: install and activation transactions (MP-06, MP-07, MEDIA-007).
//!
//! On-disk layout under the data root given to [`Store::open`]:
//! - `staging/<token>/`      work area of one install; never active, never listed.
//! - `revisions/<install_id>/` a published, immutable revision (descriptor + payloads).
//! - `registry.json`         installed records, request results, pending activation.
//! - `active.json`           the activation pointer (absent = nothing active).
//!
//! `registry.json` and `active.json` are replaced atomically (write a temporary
//! file next to it, then rename). A revision directory is published by renaming
//! a fully verified staging directory. Nothing else is ever modified in place.
//!
//! Crash model: the only crash point is "between two [`Step`]s". A [`Hook`] can
//! return [`Injection::Crash`] at a step; the call then returns
//! `StoreError::Interrupted(step)` immediately and performs NO cleanup, like a
//! killed process. [`Store::open`] runs recovery on the leftovers (see below).
//! [`Injection::IoError`] models a recoverable write failure (disk full,
//! permission) at that step.
//!
//! Recovery on `open`: remove every `staging/*` directory; remove any
//! `revisions/<id>` directory that `registry.json` does not list; remove leftover
//! temporary files; keep `registry.json` and `active.json` as they are (they are
//! only ever replaced atomically). An unreadable or schema-invalid
//! `registry.json`/`active.json` is `StoreError::Corrupt`, never silently reset.
//! An `active.json` naming an install that is not in the registry or whose
//! revision directory is missing is `StoreError::Corrupt`.

use std::path::{Path, PathBuf};

use crate::catalog::Catalog;
use crate::error::RejectReason;
use crate::limits::Limits;
use crate::trust::{AuthorityClass, PublisherClaim};

/// Transaction steps in execution order. Install runs the first six steps that
/// apply to it (`CopySource` .. `RenameRegistry`); activation runs `WriteActiveTemp`
/// and `RenameActive`. The hook is consulted immediately BEFORE each step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    /// Copy the source container into staging (the installer validates the bytes
    /// it copied, never the original path).
    CopySource,
    /// Container structure, descriptor, limits (everything in `stage_package`
    /// that does not need extraction).
    ValidateContainer,
    /// Bounded streaming extraction and inventory verification.
    ExtractToStaging,
    /// WAV/WebM profile checks.
    VerifyMedia,
    /// Rename the verified staging directory to `revisions/<install_id>`.
    PublishRevision,
    /// Write `registry.json.tmp` with the new record and request result.
    WriteRegistryTemp,
    /// Rename it over `registry.json` (the install commit point).
    RenameRegistry,
    /// Write `active.json.tmp` (activation).
    WriteActiveTemp,
    /// Rename it over `active.json` (the activation commit point).
    RenameActive,
}

impl Step {
    /// Every step in execution order.
    pub const ALL: [Step; 9] = [
        Step::CopySource,
        Step::ValidateContainer,
        Step::ExtractToStaging,
        Step::VerifyMedia,
        Step::PublishRevision,
        Step::WriteRegistryTemp,
        Step::RenameRegistry,
        Step::WriteActiveTemp,
        Step::RenameActive,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Injection {
    Proceed,
    /// Simulated process death before the step: return `Interrupted`, no cleanup.
    Crash,
    /// Simulated recoverable I/O failure at the step.
    IoError,
}

pub trait Hook {
    fn before(&mut self, step: Step) -> Injection;
}

/// The production hook: never injects anything.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoHook;

impl Hook for NoHook {
    fn before(&mut self, _step: Step) -> Injection {
        Injection::Proceed
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Io(String),
    Corrupt(String),
    /// A hook returned `Crash` before this step; nothing was cleaned up.
    Interrupted(Step),
}

/// media-install-request 1.0.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRequest {
    pub request_id: String,
    pub profile_id: String,
    pub source_path: PathBuf,
    pub expected_container_sha256: String,
    pub project_binding: Option<String>,
    pub activation_policy: ActivationPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationPolicy {
    Never,
    NowIfIdle,
    AfterMatch,
}

impl InstallRequest {
    /// Parse and validate a media-install-request 1.0.0 JSON document (strict:
    /// unknown keys, bad patterns and an unknown `activation_policy` such as
    /// `immediately` are errors). `Err` carries a short field name.
    pub fn from_json(bytes: &[u8]) -> Result<InstallRequest, String> {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Installed,
    Active,
    Deferred,
    ActivationError,
    Rejected,
}

impl Status {
    /// `installed`, `active`, `deferred`, `activation_error`, `rejected`.
    pub fn as_str(&self) -> &'static str {
        todo!()
    }
}

/// The installer-bound registry record (media-install-result `record`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// `ins_` + 10 decimal digits from a registry counter (`ins_0000000001`, ...).
    pub install_id: String,
    pub package_id: String,
    pub revision: u64,
    pub container_sha256: String,
    pub authority_class: AuthorityClass,
    pub publisher_claim: PublisherClaim,
}

/// media-install-result 1.0.0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallResult {
    pub request_id: String,
    pub status: Status,
    /// Contract reason code (see `RejectReason::contract_reason`,
    /// `ACTIVATION_DEFERRED`, `DISK_PRESSURE`); `None` for `installed` and `active`.
    pub reason: Option<String>,
    /// Present for every status except `rejected`.
    pub record: Option<Record>,
    /// The specific local reason for `rejected` results (not part of the contract JSON).
    pub reject: Option<RejectReason>,
}

impl InstallResult {
    /// The media-install-result 1.0.0 document; it must validate against
    /// contracts/media-install-result.schema.json.
    pub fn to_json(&self) -> serde_json::Value {
        todo!()
    }
}

/// What the host knows when activation is requested.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivationState {
    /// True while a match (or any barrier) forbids changing the selection.
    pub match_active: bool,
    /// Install IDs held by a playback lease; the active revision is among them
    /// while audio or a visual tail is playing.
    pub leased_install_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRevision {
    pub install_id: String,
    pub package_id: String,
    pub revision: u64,
    pub container_sha256: String,
}

/// Verified media for one event, resolved from the ACTIVE revision only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMedia {
    pub install_id: String,
    pub event_id: String,
    pub tier: Option<u32>,
    pub mixed_audio: PathBuf,
    pub silent_visual: PathBuf,
    pub duration_ms: u32,
    pub visual_tail_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// No active revision, or no binding for the event (and tier).
    Unmapped,
    /// A binding exists but is inert (unknown or non-mappable event id).
    Inert,
    /// The pair failed re-verification (missing file, size or SHA-256 mismatch):
    /// the pair is rejected as a whole; identity audio is never substituted.
    Invalid(RejectReason),
}

pub struct Store<H: Hook = NoHook> {
    _root: PathBuf,
    _catalog: Catalog,
    _limits: Limits,
    _hook: H,
}

impl Store<NoHook> {
    pub fn open(
        root: &Path,
        catalog: Catalog,
        limits: Limits,
    ) -> Result<Store<NoHook>, StoreError> {
        todo!()
    }
}

impl<H: Hook> Store<H> {
    /// Like `open` with an injection hook (tests).
    pub fn open_with_hook(
        root: &Path,
        catalog: Catalog,
        limits: Limits,
        hook: H,
    ) -> Result<Store<H>, StoreError> {
        todo!()
    }

    /// Install transaction (never activates). Steps: copy the source into
    /// staging, validate, extract, verify media, publish the revision, commit the
    /// registry (the commit stores the request result). Returns `Ok` with status
    /// `installed` (reason `None`) or `rejected`; a rejected install leaves no
    /// staging directory and no registry change. `Err` is only an I/O/corruption
    /// failure or an injected crash.
    ///
    /// - `expected_container_sha256` is compared with the digest of the bytes the
    ///   installer copied; mismatch -> rejected `ContainerDigestMismatch`.
    /// - Missing source -> rejected `SourceUnavailable`. An `IoError` injection
    ///   before any commit -> rejected with reason `DISK_PRESSURE` and `reject` `None`.
    /// - Idempotency: the same `request_id` with the same digest returns the stored
    ///   result unchanged (no copy, no new record, no counter change). The same
    ///   `request_id` with a different digest -> rejected `ContainerDigestMismatch`.
    /// - Identity: a container whose `(package_id, revision)` is already installed
    ///   with the same container digest is already installed: returns `installed`
    ///   with the existing record (a new request id still gets its own stored
    ///   result). The same `(package_id, revision)` with a different digest ->
    ///   rejected `RevisionConflict`. A lower revision of an installed package is
    ///   installable (rollback copies are kept).
    /// - The record carries `assess(descriptor)` authority and claim. A package that
    ///   names a publisher key is installed as `local_import` with claim `unverified`.
    /// - A previously active revision stays active and untouched by any install.
    pub fn install(&mut self, request: &InstallRequest) -> Result<InstallResult, StoreError> {
        todo!()
    }

    /// Activation transaction for an installed revision. `request_id` names the
    /// request the result is reported for.
    ///
    /// - `ActivationPolicy::Never` -> `installed` (no change).
    /// - `NowIfIdle`: activate when `!state.match_active` and the currently active
    ///   revision is not in `state.leased_install_ids`; otherwise `deferred` with
    ///   reason `ACTIVATION_DEFERRED`, the pending activation recorded in the
    ///   registry, the old revision still active and usable.
    /// - `AfterMatch`: `deferred` while `state.match_active` or the active revision
    ///   is leased; activates otherwise (same rule as `NowIfIdle` but always
    ///   recorded as pending first, so that `resume_deferred` picks it up).
    /// - Activating the already active install is idempotent: `active`.
    /// - An unknown `install_id` -> rejected `UnknownInstall`.
    /// - An `IoError` injection at `WriteActiveTemp` or `RenameActive` ->
    ///   `activation_error` with reason `DISK_PRESSURE`; the install stays
    ///   installed, the old revision stays active, no partial `active.json`
    ///   remains. A `Crash` returns `Interrupted`; the next `open` finds either
    ///   the old or the new pointer, never a mix.
    pub fn activate(
        &mut self,
        request_id: &str,
        install_id: &str,
        policy: ActivationPolicy,
        state: &ActivationState,
    ) -> Result<InstallResult, StoreError> {
        todo!()
    }

    /// Try every pending (deferred) activation, oldest first, against `state`;
    /// returns one result per pending activation that changed status (`active` or
    /// `activation_error`). Pending activations that are still blocked stay pending.
    pub fn resume_deferred(
        &mut self,
        state: &ActivationState,
    ) -> Result<Vec<InstallResult>, StoreError> {
        todo!()
    }

    pub fn active(&self) -> Option<ActiveRevision> {
        todo!()
    }

    /// Installed records, oldest first.
    pub fn records(&self) -> Vec<Record> {
        todo!()
    }

    /// Install IDs with a pending (deferred) activation, oldest first.
    pub fn pending_activations(&self) -> Vec<String> {
        todo!()
    }

    /// Resolve the media pair for an event from the active revision only.
    /// `tier == Some(n)` resolves only the binding with exactly that tier;
    /// `tier == None` resolves only the tier-less binding. There is no fallback
    /// to another tier or from a tier to the tier-less binding (`Unmapped`).
    /// Bindings whose event id is inert give `Inert`. Both files are re-verified (exists, size, SHA-256) before the
    /// pair is returned. No loose-file, default-pack or generated fallback.
    pub fn resolve(
        &self,
        event_id: &str,
        tier: Option<u32>,
    ) -> Result<ResolvedMedia, ResolveError> {
        todo!()
    }

    /// The data root this store owns.
    pub fn root(&self) -> &Path {
        todo!()
    }
}
