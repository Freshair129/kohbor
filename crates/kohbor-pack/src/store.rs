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
//! Crash tests inject interruption only at declared [`Step`] boundaries. They
//! do not simulate power loss or a failure between operations inside one step.
//! A [`Hook`] can return [`Injection::Crash`] at a step; the call then returns
//! `StoreError::Interrupted(step)` immediately and performs NO cleanup, like a
//! killed process. [`Store::open`] runs recovery on the leftovers (see below).
//! [`Injection::IoError`] models a recoverable write failure (disk full,
//! permission) at that step.
//!
//! Recovery on `open`: remove every direct child of `staging/`; remove each
//! unlisted `revisions/<id>` directory and only `registry.json.tmp` and
//! `active.json.tmp` in the root; leave all other paths alone. Keep
//! `registry.json` and `active.json` as they are (they are
//! only ever replaced atomically). An unreadable or schema-invalid
//! `registry.json`/`active.json` is `StoreError::Corrupt`, never silently reset.
//! An `active.json` naming an install that is not in the registry or whose
//! revision directory is missing is `StoreError::Corrupt`.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::catalog::Catalog;
use crate::container;
use crate::descriptor::{self, Descriptor};
use crate::error::RejectReason;
use crate::fsutil::{flush_dir, is_directory_reparse_point, is_link_or_reparse_point};
use crate::limits::{Limits, MAX_DESCRIPTOR_BYTES, MAX_PAYLOAD_BYTES};
use crate::trust::{self, AuthorityClass, PublisherClaim};
use kohbor_gsi::strict_json;
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};

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
        let value = strict_json::parse(bytes).map_err(|_| "json".to_owned())?;
        let object = value.as_object().ok_or_else(|| "object".to_owned())?;
        const KEYS: [&str; 7] = [
            "schema_version",
            "request_id",
            "profile_id",
            "source_path",
            "expected_container_sha256",
            "project_binding",
            "activation_policy",
        ];
        if object.len() != KEYS.len() || object.keys().any(|key| !KEYS.contains(&key.as_str())) {
            return Err("keys".to_owned());
        }
        if object.get("schema_version").and_then(Value::as_str) != Some("1.0.0") {
            return Err("schema_version".to_owned());
        }
        let request_id = object
            .get("request_id")
            .and_then(Value::as_str)
            .filter(|value| valid_token(value, 8, 64))
            .ok_or_else(|| "request_id".to_owned())?
            .to_owned();
        let profile_id = object
            .get("profile_id")
            .and_then(Value::as_str)
            .filter(|value| valid_token(value, 1, 64))
            .ok_or_else(|| "profile_id".to_owned())?
            .to_owned();
        let source_path = object
            .get("source_path")
            .and_then(Value::as_str)
            .filter(|value| (3..=1024).contains(&value.chars().count()))
            .ok_or_else(|| "source_path".to_owned())?;
        let expected_container_sha256 = object
            .get("expected_container_sha256")
            .and_then(Value::as_str)
            .filter(|value| valid_sha256(value))
            .ok_or_else(|| "expected_container_sha256".to_owned())?
            .to_owned();
        let project_binding = match object.get("project_binding") {
            Some(Value::Null) => None,
            Some(Value::String(value)) if valid_token(value, 1, 64) => Some(value.clone()),
            _ => return Err("project_binding".to_owned()),
        };
        let activation_policy = match object.get("activation_policy").and_then(Value::as_str) {
            Some("never") => ActivationPolicy::Never,
            Some("now_if_idle") => ActivationPolicy::NowIfIdle,
            Some("after_match") => ActivationPolicy::AfterMatch,
            _ => return Err("activation_policy".to_owned()),
        };
        Ok(InstallRequest {
            request_id,
            profile_id,
            source_path: PathBuf::from(source_path),
            expected_container_sha256,
            project_binding,
            activation_policy,
        })
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
        match self {
            Status::Installed => "installed",
            Status::Active => "active",
            Status::Deferred => "deferred",
            Status::ActivationError => "activation_error",
            Status::Rejected => "rejected",
        }
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
        let mut object = Map::new();
        object.insert(
            "schema_version".to_owned(),
            Value::String("1.0.0".to_owned()),
        );
        object.insert(
            "request_id".to_owned(),
            Value::String(self.request_id.clone()),
        );
        object.insert(
            "status".to_owned(),
            Value::String(self.status.as_str().to_owned()),
        );
        object.insert(
            "reason".to_owned(),
            self.reason
                .as_ref()
                .map_or(Value::Null, |reason| Value::String(reason.clone())),
        );
        object.insert(
            "record".to_owned(),
            self.record.as_ref().map_or(Value::Null, record_value),
        );
        Value::Object(object)
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
    pub silent_visual: Option<PathBuf>,
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
    root: PathBuf,
    catalog: Catalog,
    limits: Limits,
    hook: H,
    registry: Registry,
    active: Option<ActivePointer>,
    staging_sequence: u64,
}

impl Store<NoHook> {
    pub fn open(
        root: &Path,
        catalog: Catalog,
        limits: Limits,
    ) -> Result<Store<NoHook>, StoreError> {
        Store::open_with_hook(root, catalog, limits, NoHook)
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
        let root = prepare_root(root)?;
        ensure_child_dir(&root, "staging")?;
        ensure_child_dir(&root, "revisions")?;
        let registry_path = root.join("registry.json");
        let registry_exists = path_exists_nofollow(&registry_path).map_err(corrupt_store_error)?;
        let registry = if registry_exists {
            let bytes = read_store_file(&registry_path, MAX_DESCRIPTOR_BYTES)
                .map_err(corrupt_store_error)?;
            parse_registry(&bytes)?
        } else {
            Registry::default()
        };
        let active_path = root.join("active.json");
        let active_exists = path_exists_nofollow(&active_path).map_err(corrupt_store_error)?;
        let active = if active_exists {
            let bytes = read_store_file(&active_path, 64 * 1024).map_err(corrupt_store_error)?;
            Some(parse_active(&bytes)?)
        } else {
            None
        };
        validate_registry_revisions(&root, &registry, &catalog, limits.clamped())?;
        validate_active(&root, &registry, active.as_ref())?;
        recover_leftovers(&root, &registry)?;
        Ok(Store {
            root,
            catalog,
            limits: limits.clamped(),
            hook,
            registry,
            active,
            staging_sequence: 0,
        })
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
    /// - Idempotency: the same `request_id` with the same digest, profile and
    ///   project binding returns the stored result unchanged (no copy, no new
    ///   record, no counter change). Any different request identity is rejected
    ///   as `RequestConflict`.
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
        if !valid_install_request(request) {
            return Err(StoreError::Io("invalid install request fields".to_owned()));
        }
        if let Some(existing) = self
            .registry
            .request_results
            .iter()
            .find(|entry| entry.request_id == request.request_id)
        {
            let same_identity = existing.expected_container_sha256
                == request.expected_container_sha256
                && existing
                    .fingerprint
                    .as_ref()
                    .is_none_or(|fingerprint| fingerprint == &request_fingerprint(request));
            if same_identity {
                return Ok(existing.result.clone());
            }
            return Ok(rejected_result(
                &request.request_id,
                RejectReason::RequestConflict,
            ));
        }

        let staging = self.create_staging_dir()?;
        match self.hook.before(Step::CopySource) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::CopySource)),
            Injection::IoError => {
                remove_path(&staging)?;
                return Ok(disk_pressure_install(&request.request_id));
            }
        }

        let copied_path = staging.join("source.container");
        let copied_digest = match copy_source(&request.source_path, &copied_path, self.limits) {
            Ok(digest) => digest,
            Err(reason) => {
                remove_path(&staging)?;
                return Ok(rejected_result(&request.request_id, reason));
            }
        };

        // stage_package stays unchanged. Its container, descriptor, inventory and
        // media work is bracketed by the three middle hooks in documented order.
        match self.hook.before(Step::ValidateContainer) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::ValidateContainer)),
            Injection::IoError => {
                remove_path(&staging)?;
                return Ok(disk_pressure_install(&request.request_id));
            }
        }
        if copied_digest != request.expected_container_sha256 {
            remove_path(&staging)?;
            return Ok(rejected_result(
                &request.request_id,
                RejectReason::ContainerDigestMismatch,
            ));
        }
        match self.hook.before(Step::ExtractToStaging) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::ExtractToStaging)),
            Injection::IoError => {
                remove_path(&staging)?;
                return Ok(disk_pressure_install(&request.request_id));
            }
        }
        match self.hook.before(Step::VerifyMedia) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::VerifyMedia)),
            Injection::IoError => {
                remove_path(&staging)?;
                return Ok(disk_pressure_install(&request.request_id));
            }
        }

        let staged = match container::stage_package_for_store(
            &copied_path,
            &staging,
            &self.catalog,
            &self.limits,
        ) {
            Ok(staged) => staged,
            Err(container::StageError::Rejected(reason)) => {
                remove_path(&staging)?;
                return Ok(rejected_result(&request.request_id, reason));
            }
            Err(container::StageError::Io(error)) => {
                remove_path(&staging)?;
                return Err(io_error("flush staged package file", error));
            }
        };
        if staged.container_sha256 != request.expected_container_sha256 {
            remove_path(&staging)?;
            return Ok(rejected_result(
                &request.request_id,
                RejectReason::ContainerDigestMismatch,
            ));
        }

        let descriptor_bytes = match read_store_file(
            &staging.join("descriptor.json"),
            self.limits.descriptor_bytes,
        ) {
            Ok(bytes) => bytes,
            Err(error) => {
                remove_path(&staging)?;
                return Err(error);
            }
        };
        let descriptor_sha256 = sha256(&descriptor_bytes);
        remove_path(&copied_path)?;

        let descriptor = &staged.parsed.descriptor;
        let same_identity = self.registry.records.iter().find(|installed| {
            installed.record.package_id == descriptor.package_id
                && installed.record.revision == descriptor.revision
        });
        let (record, new_revision, next_counter) = match same_identity {
            Some(installed) if installed.record.container_sha256 == staged.container_sha256 => (
                installed.record.clone(),
                false,
                self.registry.install_id_counter,
            ),
            Some(_) => {
                remove_path(&staging)?;
                return Ok(rejected_result(
                    &request.request_id,
                    RejectReason::RevisionConflict,
                ));
            }
            None => {
                let next = match self.registry.install_id_counter.checked_add(1) {
                    Some(value) if value <= 9_999_999_999 => value,
                    _ => {
                        remove_path(&staging)?;
                        return Err(StoreError::Io("install_id counter exhausted".to_owned()));
                    }
                };
                let assessment = trust::assess(descriptor);
                (
                    Record {
                        install_id: format!("ins_{next:010}"),
                        package_id: descriptor.package_id.clone(),
                        revision: descriptor.revision,
                        container_sha256: staged.container_sha256.clone(),
                        authority_class: assessment.authority_class,
                        publisher_claim: assessment.publisher_claim,
                    },
                    true,
                    next,
                )
            }
        };

        match self.hook.before(Step::PublishRevision) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::PublishRevision)),
            Injection::IoError => {
                remove_path(&staging)?;
                return Ok(disk_pressure_install(&request.request_id));
            }
        }

        let revision_path = self.root.join("revisions").join(&record.install_id);
        let mut published = false;
        if new_revision {
            if path_exists_nofollow(&revision_path)? {
                remove_path(&staging)?;
                return Err(StoreError::Corrupt(
                    "unlisted revision directory blocks publication".to_owned(),
                ));
            }
            if let Err(error) = fs::rename(&staging, &revision_path) {
                remove_path(&staging)?;
                return Err(io_error("publish revision", error));
            }
            published = true;
            flush_store_tree(&revision_path)?;
            flush_dir(&self.root.join("revisions"))
                .map_err(|error| io_error("flush revisions directory", error))?;
            flush_dir(&self.root.join("staging"))
                .map_err(|error| io_error("flush staging directory", error))?;
        } else {
            remove_path(&staging)?;
        }

        let result = InstallResult {
            request_id: request.request_id.clone(),
            status: Status::Installed,
            reason: None,
            record: Some(record.clone()),
            reject: None,
        };
        let mut candidate = self.registry.clone();
        if new_revision {
            candidate.install_id_counter = next_counter;
            candidate.records.push(StoredRecord {
                record: record.clone(),
                descriptor_sha256,
            });
        }
        candidate.request_results.push(StoredRequestResult {
            request_id: request.request_id.clone(),
            expected_container_sha256: request.expected_container_sha256.clone(),
            fingerprint: Some(request_fingerprint(request)),
            result: result.clone(),
        });
        if candidate.request_results.len() > MAX_REQUEST_RESULTS {
            let evict_count = candidate.request_results.len() - MAX_REQUEST_RESULTS;
            candidate.request_results.drain(..evict_count);
        }
        let registry_bytes = match serialize_registry(&candidate) {
            Ok(bytes) => bytes,
            Err(error) => {
                if published {
                    remove_path(&revision_path)?;
                }
                return Err(error);
            }
        };

        match self.hook.before(Step::WriteRegistryTemp) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::WriteRegistryTemp)),
            Injection::IoError => {
                if published {
                    remove_path(&revision_path)?;
                }
                return Ok(disk_pressure_install(&request.request_id));
            }
        }
        if let Err(error) = write_temp_file(&self.root, "registry.json", &registry_bytes) {
            if published {
                remove_path(&revision_path)?;
            }
            return Err(error);
        }

        match self.hook.before(Step::RenameRegistry) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::RenameRegistry)),
            Injection::IoError => {
                remove_file_if_exists(&self.root.join("registry.json.tmp"))?;
                if published {
                    remove_path(&revision_path)?;
                }
                return Ok(disk_pressure_install(&request.request_id));
            }
        }
        match rename_temp_file(&self.root, "registry.json") {
            Ok(()) => {}
            Err(AtomicRenameError::BeforeRename(error)) => {
                remove_file_if_exists(&self.root.join("registry.json.tmp"))?;
                if published {
                    remove_path(&revision_path)?;
                }
                return Err(error);
            }
            Err(AtomicRenameError::AfterRename(error)) => {
                self.registry = candidate;
                return Err(error);
            }
        }
        self.registry = candidate;
        Ok(result)
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
        if !valid_token(request_id, 8, 64) {
            return Err(StoreError::Io("invalid activation request_id".to_owned()));
        }
        let Some(installed) = self
            .registry
            .records
            .iter()
            .find(|installed| installed.record.install_id == install_id)
            .cloned()
        else {
            return Ok(rejected_result(request_id, RejectReason::UnknownInstall));
        };
        let record = installed.record.clone();

        if policy == ActivationPolicy::Never {
            return Ok(activation_result(
                request_id,
                Status::Installed,
                None,
                Some(record),
            ));
        }
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.install_id == install_id)
        {
            self.remove_pending_for(install_id)?;
            return Ok(activation_result(
                request_id,
                Status::Active,
                None,
                Some(record),
            ));
        }
        if policy == ActivationPolicy::AfterMatch {
            self.add_pending(request_id, install_id)?;
        }

        let blocked = state.match_active
            || self.active.as_ref().is_some_and(|active| {
                state
                    .leased_install_ids
                    .iter()
                    .any(|leased| leased == &active.install_id)
            });
        if blocked {
            self.add_pending(request_id, install_id)?;
            return Ok(activation_result(
                request_id,
                Status::Deferred,
                Some("ACTIVATION_DEFERRED"),
                Some(record),
            ));
        }

        verify_revision(&self.root, &installed, &self.catalog, self.limits, true)?;
        let pointer = ActivePointer {
            install_id: record.install_id.clone(),
            package_id: record.package_id.clone(),
            revision: record.revision,
            container_sha256: record.container_sha256.clone(),
        };
        let bytes = serialize_active(&pointer)?;

        match self.hook.before(Step::WriteActiveTemp) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::WriteActiveTemp)),
            Injection::IoError => {
                remove_file_if_exists(&self.root.join("active.json.tmp"))?;
                self.remove_pending_for(install_id)?;
                return Ok(activation_result(
                    request_id,
                    Status::ActivationError,
                    Some("DISK_PRESSURE"),
                    Some(record),
                ));
            }
        }
        if let Err(error) = write_temp_file(&self.root, "active.json", &bytes) {
            remove_file_if_exists(&self.root.join("active.json.tmp"))?;
            return Err(error);
        }

        match self.hook.before(Step::RenameActive) {
            Injection::Proceed => {}
            Injection::Crash => return Err(StoreError::Interrupted(Step::RenameActive)),
            Injection::IoError => {
                remove_file_if_exists(&self.root.join("active.json.tmp"))?;
                self.remove_pending_for(install_id)?;
                return Ok(activation_result(
                    request_id,
                    Status::ActivationError,
                    Some("DISK_PRESSURE"),
                    Some(record),
                ));
            }
        }
        match rename_temp_file(&self.root, "active.json") {
            Ok(()) => {}
            Err(AtomicRenameError::BeforeRename(error)) => {
                remove_file_if_exists(&self.root.join("active.json.tmp"))?;
                return Err(error);
            }
            Err(AtomicRenameError::AfterRename(error)) => {
                self.active = Some(pointer);
                return Err(error);
            }
        }
        self.active = Some(pointer);
        self.remove_pending_for(install_id)?;
        Ok(activation_result(
            request_id,
            Status::Active,
            None,
            Some(record),
        ))
    }

    /// Try every pending (deferred) activation, oldest first, against `state`;
    /// returns one result per pending activation that changed status (`active` or
    /// `activation_error`). Pending activations that are still blocked stay pending.
    pub fn resume_deferred(
        &mut self,
        state: &ActivationState,
    ) -> Result<Vec<InstallResult>, StoreError> {
        let pending = self.registry.pending_activations.clone();
        let mut results = Vec::new();
        for activation in pending {
            let blocked = state.match_active
                || self.active.as_ref().is_some_and(|active| {
                    state
                        .leased_install_ids
                        .iter()
                        .any(|leased| leased == &active.install_id)
                });
            if blocked {
                continue;
            }
            let result = self.activate(
                &activation.request_id,
                &activation.install_id,
                ActivationPolicy::AfterMatch,
                state,
            )?;
            if matches!(result.status, Status::Active | Status::ActivationError) {
                results.push(result);
            }
        }
        Ok(results)
    }

    pub fn active(&self) -> Option<ActiveRevision> {
        self.active.as_ref().map(|active| ActiveRevision {
            install_id: active.install_id.clone(),
            package_id: active.package_id.clone(),
            revision: active.revision,
            container_sha256: active.container_sha256.clone(),
        })
    }

    /// Installed records, oldest first.
    pub fn records(&self) -> Vec<Record> {
        self.registry
            .records
            .iter()
            .map(|installed| installed.record.clone())
            .collect()
    }

    /// Install IDs with a pending (deferred) activation, oldest first.
    pub fn pending_activations(&self) -> Vec<String> {
        self.registry
            .pending_activations
            .iter()
            .map(|pending| pending.install_id.clone())
            .collect()
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
        let active = self.active.as_ref().ok_or(ResolveError::Unmapped)?;
        let installed = self
            .registry
            .records
            .iter()
            .find(|installed| installed.record.install_id == active.install_id)
            .ok_or(ResolveError::Invalid(RejectReason::UnknownInstall))?;
        let revision_dir = self
            .root
            .join("revisions")
            .join(&installed.record.install_id);
        let revisions_dir = self.root.join("revisions");
        for directory in [
            self.root.as_path(),
            revisions_dir.as_path(),
            revision_dir.as_path(),
        ] {
            let metadata = fs::symlink_metadata(directory)
                .map_err(|_| ResolveError::Invalid(RejectReason::SourceUnavailable))?;
            if is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(ResolveError::Invalid(RejectReason::UnsafePath));
            }
        }
        ensure_store_tree_no_links(&revision_dir)
            .map_err(|_| ResolveError::Invalid(RejectReason::UnsafePath))?;
        let descriptor_path = revision_dir.join("descriptor.json");
        let descriptor_bytes = read_resolve_file(&descriptor_path, self.limits.descriptor_bytes)
            .map_err(ResolveError::Invalid)?;
        if sha256(&descriptor_bytes) != installed.descriptor_sha256 {
            return Err(ResolveError::Invalid(RejectReason::DigestMismatch));
        }
        let parsed = descriptor::parse_descriptor(&descriptor_bytes, &self.catalog, &self.limits)
            .map_err(ResolveError::Invalid)?;
        if parsed.descriptor.package_id != installed.record.package_id
            || parsed.descriptor.revision != installed.record.revision
        {
            return Err(ResolveError::Invalid(RejectReason::DigestMismatch));
        }
        if parsed.inert_event_ids.iter().any(|inert| inert == event_id) {
            return Err(ResolveError::Inert);
        }
        let Some(binding) = parsed
            .descriptor
            .bindings
            .iter()
            .find(|binding| binding.event_id == event_id && binding.tier == tier)
        else {
            return Err(ResolveError::Unmapped);
        };
        let mixed = parsed
            .descriptor
            .files
            .iter()
            .find(|file| file.path == binding.mixed_audio_path)
            .ok_or(ResolveError::Invalid(RejectReason::DanglingReference))?;
        let mixed_path =
            revision_payload_path(&self.root, &installed.record.install_id, &mixed.path)
                .map_err(ResolveError::Invalid)?;
        verify_payload_file(&mixed_path, mixed.size_bytes, &mixed.sha256)
            .map_err(ResolveError::Invalid)?;
        let visual_path = if let Some(path) = binding.silent_visual_path.as_deref() {
            let visual = parsed
                .descriptor
                .files
                .iter()
                .find(|file| file.path == path)
                .ok_or(ResolveError::Invalid(RejectReason::DanglingReference))?;
            let visual_path =
                revision_payload_path(&self.root, &installed.record.install_id, &visual.path)
                    .map_err(ResolveError::Invalid)?;
            verify_payload_file(&visual_path, visual.size_bytes, &visual.sha256)
                .map_err(ResolveError::Invalid)?;
            Some(visual_path)
        } else {
            None
        };
        Ok(ResolvedMedia {
            install_id: installed.record.install_id.clone(),
            event_id: binding.event_id.clone(),
            tier: binding.tier,
            mixed_audio: mixed_path,
            silent_visual: visual_path,
            duration_ms: binding.duration_ms,
            visual_tail_ms: binding.visual_tail_ms,
        })
    }

    /// The data root this store owns.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl<H: Hook> Store<H> {
    fn create_staging_dir(&mut self) -> Result<PathBuf, StoreError> {
        let staging_root = self.root.join("staging");
        loop {
            let sequence = self.staging_sequence;
            self.staging_sequence = self
                .staging_sequence
                .checked_add(1)
                .ok_or_else(|| StoreError::Io("staging token counter exhausted".to_owned()))?;
            let path = staging_root.join(format!("txn_{sequence:010}"));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(io_error("create staging directory", error)),
            }
        }
    }

    fn add_pending(&mut self, request_id: &str, install_id: &str) -> Result<(), StoreError> {
        if self
            .registry
            .pending_activations
            .iter()
            .any(|pending| pending.install_id == install_id)
        {
            return Ok(());
        }
        let mut candidate = self.registry.clone();
        candidate.pending_activations.push(PendingActivation {
            request_id: request_id.to_owned(),
            install_id: install_id.to_owned(),
        });
        self.commit_registry_without_hook(candidate)
    }

    fn remove_pending_for(&mut self, install_id: &str) -> Result<(), StoreError> {
        if !self
            .registry
            .pending_activations
            .iter()
            .any(|pending| pending.install_id == install_id)
        {
            return Ok(());
        }
        let mut candidate = self.registry.clone();
        candidate
            .pending_activations
            .retain(|pending| pending.install_id != install_id);
        self.commit_registry_without_hook(candidate)
    }

    fn commit_registry_without_hook(&mut self, candidate: Registry) -> Result<(), StoreError> {
        let bytes = serialize_registry(&candidate)?;
        write_temp_file(&self.root, "registry.json", &bytes)?;
        match rename_temp_file(&self.root, "registry.json") {
            Ok(()) => {}
            Err(AtomicRenameError::BeforeRename(error)) => {
                remove_file_if_exists(&self.root.join("registry.json.tmp"))?;
                return Err(error);
            }
            Err(AtomicRenameError::AfterRename(error)) => {
                self.registry = candidate;
                return Err(error);
            }
        }
        self.registry = candidate;
        Ok(())
    }
}

const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// The retained request-result window is a lead proposal, not a product limit.
pub const MAX_REQUEST_RESULTS: usize = 512;

#[derive(Debug, Clone, Default)]
struct Registry {
    install_id_counter: u64,
    records: Vec<StoredRecord>,
    request_results: Vec<StoredRequestResult>,
    pending_activations: Vec<PendingActivation>,
}

#[derive(Debug, Clone)]
struct StoredRecord {
    record: Record,
    descriptor_sha256: String,
}

#[derive(Debug, Clone)]
struct StoredRequestResult {
    request_id: String,
    expected_container_sha256: String,
    fingerprint: Option<RequestFingerprint>,
    result: InstallResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequestFingerprint {
    expected_container_sha256: String,
    profile_id: String,
    project_binding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingActivation {
    request_id: String,
    install_id: String,
}

#[derive(Debug, Clone)]
struct ActivePointer {
    install_id: String,
    package_id: String,
    revision: u64,
    container_sha256: String,
}

fn valid_token(value: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn valid_install_request(request: &InstallRequest) -> bool {
    let source_path_valid = request
        .source_path
        .to_str()
        .is_some_and(|path| (3..=1024).contains(&path.chars().count()));
    valid_token(&request.request_id, 8, 64)
        && valid_token(&request.profile_id, 1, 64)
        && source_path_valid
        && valid_sha256(&request.expected_container_sha256)
        && request
            .project_binding
            .as_ref()
            .is_none_or(|value| valid_token(value, 1, 64))
}

fn request_fingerprint(request: &InstallRequest) -> RequestFingerprint {
    RequestFingerprint {
        expected_container_sha256: request.expected_container_sha256.clone(),
        profile_id: request.profile_id.clone(),
        project_binding: request.project_binding.clone(),
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_package_id(value: &str) -> bool {
    (1..=80).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
}

fn valid_install_id(value: &str) -> Option<u64> {
    let digits = value.strip_prefix("ins_")?;
    if digits.len() != 10 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number = digits.parse::<u64>().ok()?;
    (number > 0).then_some(number)
}

fn valid_payload_path(value: &str) -> bool {
    if value.is_empty() || !value.is_ascii() || value.contains('\\') || value.contains(':') {
        return false;
    }
    if value
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return false;
    }
    let path = Path::new(value);
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn record_value(record: &Record) -> Value {
    let mut object = Map::new();
    object.insert(
        "install_id".to_owned(),
        Value::String(record.install_id.clone()),
    );
    object.insert(
        "package_id".to_owned(),
        Value::String(record.package_id.clone()),
    );
    object.insert(
        "revision".to_owned(),
        Value::Number(Number::from(record.revision)),
    );
    object.insert(
        "container_sha256".to_owned(),
        Value::String(record.container_sha256.clone()),
    );
    object.insert(
        "authority_class".to_owned(),
        Value::String(record.authority_class.as_str().to_owned()),
    );
    object.insert(
        "publisher_claim".to_owned(),
        Value::String(record.publisher_claim.as_str().to_owned()),
    );
    Value::Object(object)
}

fn activation_result(
    request_id: &str,
    status: Status,
    reason: Option<&str>,
    record: Option<Record>,
) -> InstallResult {
    InstallResult {
        request_id: request_id.to_owned(),
        status,
        reason: reason.map(str::to_owned),
        record,
        reject: None,
    }
}

fn rejected_result(request_id: &str, reject: RejectReason) -> InstallResult {
    InstallResult {
        request_id: request_id.to_owned(),
        status: Status::Rejected,
        reason: Some(reject.contract_reason().to_owned()),
        record: None,
        reject: Some(reject),
    }
}

fn disk_pressure_install(request_id: &str) -> InstallResult {
    InstallResult {
        request_id: request_id.to_owned(),
        status: Status::Rejected,
        reason: Some("DISK_PRESSURE".to_owned()),
        record: None,
        reject: None,
    }
}

fn registry_value(registry: &Registry) -> Value {
    let records = registry
        .records
        .iter()
        .map(|installed| {
            let mut object = match record_value(&installed.record) {
                Value::Object(object) => object,
                _ => Map::new(),
            };
            object.insert(
                "descriptor_sha256".to_owned(),
                Value::String(installed.descriptor_sha256.clone()),
            );
            Value::Object(object)
        })
        .collect();
    let request_results = registry
        .request_results
        .iter()
        .map(|entry| {
            let mut object = Map::new();
            object.insert(
                "request_id".to_owned(),
                Value::String(entry.request_id.clone()),
            );
            object.insert(
                "expected_container_sha256".to_owned(),
                Value::String(entry.expected_container_sha256.clone()),
            );
            object.insert(
                "fingerprint".to_owned(),
                entry
                    .fingerprint
                    .as_ref()
                    .map_or(Value::Null, request_fingerprint_value),
            );
            object.insert("result".to_owned(), entry.result.to_json());
            Value::Object(object)
        })
        .collect();
    let pending = registry
        .pending_activations
        .iter()
        .map(|entry| {
            let mut object = Map::new();
            object.insert(
                "request_id".to_owned(),
                Value::String(entry.request_id.clone()),
            );
            object.insert(
                "install_id".to_owned(),
                Value::String(entry.install_id.clone()),
            );
            Value::Object(object)
        })
        .collect();
    let mut object = Map::new();
    object.insert(
        "schema_version".to_owned(),
        Value::String("1.0.0".to_owned()),
    );
    object.insert(
        "install_id_counter".to_owned(),
        Value::Number(Number::from(registry.install_id_counter)),
    );
    object.insert("records".to_owned(), Value::Array(records));
    object.insert("request_results".to_owned(), Value::Array(request_results));
    object.insert("pending_activations".to_owned(), Value::Array(pending));
    Value::Object(object)
}

fn request_fingerprint_value(fingerprint: &RequestFingerprint) -> Value {
    let mut object = Map::new();
    object.insert(
        "expected_container_sha256".to_owned(),
        Value::String(fingerprint.expected_container_sha256.clone()),
    );
    object.insert(
        "profile_id".to_owned(),
        Value::String(fingerprint.profile_id.clone()),
    );
    object.insert(
        "project_binding".to_owned(),
        fingerprint
            .project_binding
            .as_ref()
            .map_or(Value::Null, |value| Value::String(value.clone())),
    );
    Value::Object(object)
}

fn serialize_registry(registry: &Registry) -> Result<Vec<u8>, StoreError> {
    let bytes = serde_json::to_vec(&registry_value(registry))
        .map_err(|error| StoreError::Io(format!("serialize registry: {error}")))?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > MAX_DESCRIPTOR_BYTES) {
        return Err(StoreError::Io(
            "registry exceeds the 2 MiB limit".to_owned(),
        ));
    }
    Ok(bytes)
}

fn serialize_active(active: &ActivePointer) -> Result<Vec<u8>, StoreError> {
    let mut object = Map::new();
    object.insert(
        "schema_version".to_owned(),
        Value::String("1.0.0".to_owned()),
    );
    object.insert(
        "install_id".to_owned(),
        Value::String(active.install_id.clone()),
    );
    object.insert(
        "package_id".to_owned(),
        Value::String(active.package_id.clone()),
    );
    object.insert(
        "revision".to_owned(),
        Value::Number(Number::from(active.revision)),
    );
    object.insert(
        "container_sha256".to_owned(),
        Value::String(active.container_sha256.clone()),
    );
    serde_json::to_vec(&Value::Object(object))
        .map_err(|error| StoreError::Io(format!("serialize active pointer: {error}")))
}

fn parse_registry(bytes: &[u8]) -> Result<Registry, StoreError> {
    let value = strict_json::parse(bytes)
        .map_err(|error| StoreError::Corrupt(format!("registry JSON: {error:?}")))?;
    let object = strict_object(
        &value,
        &[
            "schema_version",
            "install_id_counter",
            "records",
            "request_results",
            "pending_activations",
        ],
    )?;
    if required_string(object, "schema_version")? != "1.0.0" {
        return Err(StoreError::Corrupt("registry schema_version".to_owned()));
    }
    let install_id_counter = required_u64(object, "install_id_counter")?;
    if install_id_counter > 9_999_999_999 {
        return Err(StoreError::Corrupt(
            "registry install_id_counter".to_owned(),
        ));
    }
    let mut records = Vec::new();
    let mut record_ids = HashSet::new();
    let mut identities = HashSet::new();
    let mut max_id = 0u64;
    for value in required_array(object, "records")? {
        let stored = parse_stored_record(value)?;
        let id_number = valid_install_id(&stored.record.install_id)
            .ok_or_else(|| StoreError::Corrupt("registry install_id".to_owned()))?;
        if id_number > install_id_counter
            || !record_ids.insert(stored.record.install_id.clone())
            || !identities.insert((stored.record.package_id.clone(), stored.record.revision))
        {
            return Err(StoreError::Corrupt("registry record identity".to_owned()));
        }
        max_id = max_id.max(id_number);
        records.push(stored);
    }
    if max_id > install_id_counter {
        return Err(StoreError::Corrupt(
            "registry counter behind records".to_owned(),
        ));
    }

    let mut request_results = Vec::new();
    let mut request_ids = HashSet::new();
    for value in required_array(object, "request_results")? {
        let has_fingerprint = value
            .as_object()
            .is_some_and(|object| object.contains_key("fingerprint"));
        let item = if has_fingerprint {
            strict_object(
                value,
                &[
                    "request_id",
                    "expected_container_sha256",
                    "fingerprint",
                    "result",
                ],
            )?
        } else {
            strict_object(
                value,
                &["request_id", "expected_container_sha256", "result"],
            )?
        };
        let request_id = required_string(item, "request_id")?.to_owned();
        let digest = required_string(item, "expected_container_sha256")?.to_owned();
        if !valid_token(&request_id, 8, 64)
            || !valid_sha256(&digest)
            || !request_ids.insert(request_id.clone())
        {
            return Err(StoreError::Corrupt(
                "registry request result identity".to_owned(),
            ));
        }
        let fingerprint = match item.get("fingerprint") {
            None | Some(Value::Null) => None,
            Some(value) => Some(parse_request_fingerprint(value, &digest)?),
        };
        let result = parse_result(required_value(item, "result")?)?;
        if result.request_id != request_id
            || result.status != Status::Installed
            || result.reason.is_some()
            || result.reject.is_some()
        {
            return Err(StoreError::Corrupt("registry request result".to_owned()));
        }
        let result_record = result
            .record
            .as_ref()
            .ok_or_else(|| StoreError::Corrupt("registry request result record".to_owned()))?;
        let installed = records
            .iter()
            .find(|installed| installed.record.install_id == result_record.install_id)
            .ok_or_else(|| {
                StoreError::Corrupt("registry result references unknown record".to_owned())
            })?;
        if result_record != &installed.record || result_record.container_sha256 != digest {
            return Err(StoreError::Corrupt(
                "registry result record mismatch".to_owned(),
            ));
        }
        request_results.push(StoredRequestResult {
            request_id,
            expected_container_sha256: digest,
            fingerprint,
            result,
        });
    }
    if request_results.len() > MAX_REQUEST_RESULTS {
        let evict_count = request_results.len() - MAX_REQUEST_RESULTS;
        request_results.drain(..evict_count);
    }

    let mut pending_activations = Vec::new();
    let mut pending_ids = HashSet::new();
    for value in required_array(object, "pending_activations")? {
        let item = strict_object(value, &["request_id", "install_id"])?;
        let request_id = required_string(item, "request_id")?.to_owned();
        let install_id = required_string(item, "install_id")?.to_owned();
        if !valid_token(&request_id, 8, 64)
            || valid_install_id(&install_id).is_none()
            || !pending_ids.insert(install_id.clone())
            || !records
                .iter()
                .any(|record| record.record.install_id == install_id)
        {
            return Err(StoreError::Corrupt(
                "registry pending activation".to_owned(),
            ));
        }
        pending_activations.push(PendingActivation {
            request_id,
            install_id,
        });
    }

    Ok(Registry {
        install_id_counter,
        records,
        request_results,
        pending_activations,
    })
}

fn parse_request_fingerprint(
    value: &Value,
    expected_digest: &str,
) -> Result<RequestFingerprint, StoreError> {
    let object = strict_object(
        value,
        &["expected_container_sha256", "profile_id", "project_binding"],
    )?;
    let expected_container_sha256 = required_string(object, "expected_container_sha256")?;
    let profile_id = required_string(object, "profile_id")?;
    let project_binding = match required_value(object, "project_binding")? {
        Value::Null => None,
        Value::String(value) if valid_token(value, 1, 64) => Some(value.clone()),
        _ => {
            return Err(StoreError::Corrupt(
                "registry request fingerprint".to_owned(),
            ))
        }
    };
    if expected_container_sha256 != expected_digest
        || !valid_sha256(expected_container_sha256)
        || !valid_token(profile_id, 1, 64)
    {
        return Err(StoreError::Corrupt(
            "registry request fingerprint".to_owned(),
        ));
    }
    Ok(RequestFingerprint {
        expected_container_sha256: expected_container_sha256.to_owned(),
        profile_id: profile_id.to_owned(),
        project_binding,
    })
}

fn parse_stored_record(value: &Value) -> Result<StoredRecord, StoreError> {
    let object = strict_object(
        value,
        &[
            "install_id",
            "package_id",
            "revision",
            "container_sha256",
            "authority_class",
            "publisher_claim",
            "descriptor_sha256",
        ],
    )?;
    let record = parse_record_fields(object)?;
    let descriptor_sha256 = required_string(object, "descriptor_sha256")?.to_owned();
    if !valid_sha256(&descriptor_sha256) {
        return Err(StoreError::Corrupt("registry descriptor_sha256".to_owned()));
    }
    Ok(StoredRecord {
        record,
        descriptor_sha256,
    })
}

fn parse_record(value: &Value) -> Result<Record, StoreError> {
    let object = strict_object(
        value,
        &[
            "install_id",
            "package_id",
            "revision",
            "container_sha256",
            "authority_class",
            "publisher_claim",
        ],
    )?;
    parse_record_fields(object)
}

fn parse_record_fields(object: &Map<String, Value>) -> Result<Record, StoreError> {
    let install_id = required_string(object, "install_id")?.to_owned();
    let package_id = required_string(object, "package_id")?.to_owned();
    let revision = required_u64(object, "revision")?;
    let container_sha256 = required_string(object, "container_sha256")?.to_owned();
    let authority_class = match required_string(object, "authority_class")? {
        "local_import" => AuthorityClass::LocalImport,
        _ => return Err(StoreError::Corrupt("record authority_class".to_owned())),
    };
    let publisher_claim = match required_string(object, "publisher_claim")? {
        "none" => PublisherClaim::None,
        "unverified" => PublisherClaim::Unverified,
        _ => return Err(StoreError::Corrupt("record publisher_claim".to_owned())),
    };
    if valid_install_id(&install_id).is_none()
        || !valid_package_id(&package_id)
        || revision == 0
        || revision > MAX_SAFE_INTEGER
        || !valid_sha256(&container_sha256)
    {
        return Err(StoreError::Corrupt("registry record fields".to_owned()));
    }
    Ok(Record {
        install_id,
        package_id,
        revision,
        container_sha256,
        authority_class,
        publisher_claim,
    })
}

fn parse_result(value: &Value) -> Result<InstallResult, StoreError> {
    let object = strict_object(
        value,
        &["schema_version", "request_id", "status", "reason", "record"],
    )?;
    if required_string(object, "schema_version")? != "1.0.0" {
        return Err(StoreError::Corrupt("result schema_version".to_owned()));
    }
    let request_id = required_string(object, "request_id")?.to_owned();
    if !valid_token(&request_id, 8, 64) {
        return Err(StoreError::Corrupt("result request_id".to_owned()));
    }
    let status = match required_string(object, "status")? {
        "installed" => Status::Installed,
        "active" => Status::Active,
        "deferred" => Status::Deferred,
        "activation_error" => Status::ActivationError,
        "rejected" => Status::Rejected,
        _ => return Err(StoreError::Corrupt("result status".to_owned())),
    };
    let reason = match required_value(object, "reason")? {
        Value::Null => None,
        Value::String(reason) => Some(reason.clone()),
        _ => return Err(StoreError::Corrupt("result reason".to_owned())),
    };
    let record = match required_value(object, "record")? {
        Value::Null => None,
        value => Some(parse_record(value)?),
    };
    if status == Status::Rejected && (record.is_some() || reason.is_none()) {
        return Err(StoreError::Corrupt("rejected result shape".to_owned()));
    }
    if status != Status::Rejected && record.is_none() {
        return Err(StoreError::Corrupt("result record missing".to_owned()));
    }
    Ok(InstallResult {
        request_id,
        status,
        reason,
        record,
        reject: None,
    })
}

fn parse_active(bytes: &[u8]) -> Result<ActivePointer, StoreError> {
    let value = strict_json::parse(bytes)
        .map_err(|error| StoreError::Corrupt(format!("active JSON: {error:?}")))?;
    let object = strict_object(
        &value,
        &[
            "schema_version",
            "install_id",
            "package_id",
            "revision",
            "container_sha256",
        ],
    )?;
    if required_string(object, "schema_version")? != "1.0.0" {
        return Err(StoreError::Corrupt("active schema_version".to_owned()));
    }
    let install_id = required_string(object, "install_id")?.to_owned();
    let package_id = required_string(object, "package_id")?.to_owned();
    let revision = required_u64(object, "revision")?;
    let container_sha256 = required_string(object, "container_sha256")?.to_owned();
    if valid_install_id(&install_id).is_none()
        || !valid_package_id(&package_id)
        || revision == 0
        || revision > MAX_SAFE_INTEGER
        || !valid_sha256(&container_sha256)
    {
        return Err(StoreError::Corrupt("active fields".to_owned()));
    }
    Ok(ActivePointer {
        install_id,
        package_id,
        revision,
        container_sha256,
    })
}

fn strict_object<'a>(
    value: &'a Value,
    keys: &[&str],
) -> Result<&'a Map<String, Value>, StoreError> {
    let object = value
        .as_object()
        .ok_or_else(|| StoreError::Corrupt("expected object".to_owned()))?;
    if object.len() != keys.len() || object.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err(StoreError::Corrupt("unknown or missing field".to_owned()));
    }
    Ok(object)
}

fn required_value<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a Value, StoreError> {
    object
        .get(key)
        .ok_or_else(|| StoreError::Corrupt(format!("missing {key}")))
}

fn required_string<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, StoreError> {
    required_value(object, key)?
        .as_str()
        .ok_or_else(|| StoreError::Corrupt(format!("invalid {key}")))
}

fn required_u64(object: &Map<String, Value>, key: &str) -> Result<u64, StoreError> {
    required_value(object, key)?
        .as_u64()
        .ok_or_else(|| StoreError::Corrupt(format!("invalid {key}")))
}

fn required_array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a Vec<Value>, StoreError> {
    required_value(object, key)?
        .as_array()
        .ok_or_else(|| StoreError::Corrupt(format!("invalid {key}")))
}

fn prepare_root(root: &Path) -> Result<PathBuf, StoreError> {
    match fs::symlink_metadata(root) {
        Ok(metadata) => {
            if is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(StoreError::Corrupt(
                    "store root is not a real directory".to_owned(),
                ));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("inspect store root", error)),
    }
    fs::create_dir_all(root).map_err(|error| io_error("create store root", error))?;
    ensure_store_directory(root, "store root")?;
    Ok(root.to_path_buf())
}

fn ensure_store_directory(path: &Path, label: &str) -> Result<(), StoreError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error(&format!("inspect {label}"), error))?;
    if is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(StoreError::Corrupt(format!(
            "{label} is not a real directory"
        )));
    }
    Ok(())
}

fn corrupt_store_error(error: StoreError) -> StoreError {
    match error {
        StoreError::Interrupted(_) => StoreError::Corrupt("interrupted state read".to_owned()),
        StoreError::Corrupt(message) => StoreError::Corrupt(message),
        StoreError::Io(message) => StoreError::Corrupt(message),
    }
}

fn ensure_child_dir(root: &Path, name: &str) -> Result<(), StoreError> {
    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if is_link_or_reparse_point(&metadata) || !metadata.is_dir() => Err(
            StoreError::Corrupt(format!("{name} is not a real directory")),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&path).map_err(|error| io_error(&format!("create {name}"), error))
        }
        Err(error) => Err(io_error(&format!("inspect {name}"), error)),
    }
}

fn path_exists_nofollow(path: &Path) -> Result<bool, StoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("inspect path", error)),
    }
}

fn read_store_file(path: &Path, maximum: u64) -> Result<Vec<u8>, StoreError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("read store file", error))?;
    if is_link_or_reparse_point(&metadata) || !metadata.is_file() || metadata.len() > maximum {
        return Err(StoreError::Corrupt("store file type or size".to_owned()));
    }
    let limit = maximum
        .checked_add(1)
        .ok_or_else(|| StoreError::Corrupt("store file size limit".to_owned()))?;
    let file = File::open(path).map_err(|error| io_error("open store file", error))?;
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| StoreError::Corrupt("store file too large".to_owned()))?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| StoreError::Io("allocate store file buffer".to_owned()))?;
    file.take(limit)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read store file", error))?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > maximum) {
        return Err(StoreError::Corrupt(
            "store file grew beyond limit".to_owned(),
        ));
    }
    Ok(bytes)
}

fn write_temp_file(root: &Path, name: &str, bytes: &[u8]) -> Result<(), StoreError> {
    ensure_store_directory(root, "store root")?;
    let destination = root.join(name);
    let temp = root.join(format!("{name}.tmp"));
    if destination.parent() != Some(root) || temp.parent() != Some(root) {
        return Err(StoreError::Corrupt(
            "atomic destination escaped root".to_owned(),
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| io_error("create atomic temporary file", error))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        remove_file_if_exists(&temp)?;
        return Err(io_error("write atomic temporary file", error));
    }
    drop(file);
    Ok(())
}

#[derive(Debug)]
enum AtomicRenameError {
    BeforeRename(StoreError),
    AfterRename(StoreError),
}

fn rename_temp_file(root: &Path, name: &str) -> Result<(), AtomicRenameError> {
    let temp = root.join(format!("{name}.tmp"));
    let destination = root.join(name);
    ensure_store_directory(root, "store root").map_err(AtomicRenameError::BeforeRename)?;
    for (path, is_temp) in [(&temp, true), (&destination, false)] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if is_link_or_reparse_point(&metadata) => {
                return Err(AtomicRenameError::BeforeRename(StoreError::Corrupt(
                    "atomic file is a link or reparse point".to_owned(),
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && is_temp => {
                return Err(AtomicRenameError::BeforeRename(io_error(
                    "inspect atomic temporary file",
                    error,
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AtomicRenameError::BeforeRename(io_error(
                    "inspect atomic file",
                    error,
                )))
            }
            Ok(metadata) if !metadata.is_file() => {
                return Err(AtomicRenameError::BeforeRename(StoreError::Corrupt(
                    "atomic path is not a regular file".to_owned(),
                )))
            }
            Ok(_) => {}
        }
    }
    fs::rename(&temp, &destination)
        .map_err(|error| AtomicRenameError::BeforeRename(io_error("rename atomic file", error)))?;
    flush_dir(root).map_err(|error| {
        AtomicRenameError::AfterRename(io_error("flush atomic parent directory", error))
    })
}

fn remove_link_entry(
    path: &Path,
    metadata: &fs::Metadata,
    context: &str,
) -> Result<(), StoreError> {
    let result = if is_directory_reparse_point(metadata) {
        fs::remove_dir(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|error| io_error(context, error))
}

fn remove_file_if_exists(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link_or_reparse_point(&metadata) => {
            remove_link_entry(path, &metadata, "remove temporary link")
        }
        Ok(metadata) if metadata.is_dir() => Err(StoreError::Corrupt(
            "expected file while cleaning temporary".to_owned(),
        )),
        Ok(_) => fs::remove_file(path).map_err(|error| io_error("remove temporary file", error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect temporary file", error)),
    }
}

fn remove_recovery_temp_file(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link_or_reparse_point(&metadata) => {
            remove_link_entry(path, &metadata, "remove recovery temporary link")
        }
        Ok(metadata) if metadata.is_file() => {
            fs::remove_file(path).map_err(|error| io_error("remove recovery temporary file", error))
        }
        // Unexpected directories or special files in the root are protected.
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect recovery temporary file", error)),
    }
}

fn remove_path(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_link_or_reparse_point(&metadata) => {
            remove_link_entry(path, &metadata, "remove link entry")
        }
        Ok(metadata) if metadata.is_dir() => {
            for entry in
                fs::read_dir(path).map_err(|error| io_error("read cleanup directory", error))?
            {
                let entry = entry.map_err(|error| io_error("read cleanup entry", error))?;
                remove_path(&entry.path())?;
            }
            fs::remove_dir(path).map_err(|error| io_error("remove directory", error))
        }
        Ok(_) => fs::remove_file(path).map_err(|error| io_error("remove file", error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect cleanup path", error)),
    }
}

fn copy_source(source: &Path, destination: &Path, limits: Limits) -> Result<String, RejectReason> {
    let mut input = File::open(source).map_err(|_| RejectReason::SourceUnavailable)?;
    let metadata = input
        .metadata()
        .map_err(|_| RejectReason::SourceUnavailable)?;
    if !metadata.is_file() {
        return Err(RejectReason::SourceUnavailable);
    }
    let limits = limits.clamped();
    if metadata.len() > limits.container_bytes {
        return Err(RejectReason::ContainerTooLarge);
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|_| RejectReason::SourceUnavailable)?;
    let mut hasher = Sha256::new();
    let mut copied = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|_| RejectReason::SourceUnavailable)?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).map_err(|_| RejectReason::ContainerTooLarge)?;
        copied = copied
            .checked_add(count_u64)
            .ok_or(RejectReason::ContainerTooLarge)?;
        if copied > limits.container_bytes {
            return Err(RejectReason::ContainerTooLarge);
        }
        let bytes = buffer.get(..count).ok_or(RejectReason::SourceUnavailable)?;
        output
            .write_all(bytes)
            .map_err(|_| RejectReason::SourceUnavailable)?;
        hasher.update(bytes);
    }
    output
        .sync_all()
        .map_err(|_| RejectReason::SourceUnavailable)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn io_error(context: &str, error: std::io::Error) -> StoreError {
    StoreError::Io(format!("{context}: {error}"))
}

fn ensure_store_tree_no_links(path: &Path) -> Result<(), StoreError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("inspect store tree", error))?;
    if is_link_or_reparse_point(&metadata) {
        return Err(StoreError::Corrupt(
            "store tree contains a link or reparse point".to_owned(),
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| io_error("read store tree", error))? {
            let entry = entry.map_err(|error| io_error("read store tree entry", error))?;
            ensure_store_tree_no_links(&entry.path())?;
        }
    } else if !metadata.is_file() {
        return Err(StoreError::Corrupt(
            "store tree contains a non-file entry".to_owned(),
        ));
    }
    Ok(())
}

fn flush_store_tree(path: &Path) -> Result<(), StoreError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("inspect staged tree", error))?;
    if is_link_or_reparse_point(&metadata) {
        return Err(StoreError::Corrupt(
            "staged tree contains a link or reparse point".to_owned(),
        ));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| io_error("read staged tree", error))? {
            let entry = entry.map_err(|error| io_error("read staged tree entry", error))?;
            flush_store_tree(&entry.path())?;
        }
        flush_dir(path).map_err(|error| io_error("flush staged directory", error))?;
    } else if metadata.is_file() {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| io_error("flush staged file", error))?;
    } else {
        return Err(StoreError::Corrupt(
            "staged tree contains a non-file entry".to_owned(),
        ));
    }
    Ok(())
}

fn validate_registry_revisions(
    root: &Path,
    registry: &Registry,
    catalog: &Catalog,
    limits: Limits,
) -> Result<(), StoreError> {
    for installed in &registry.records {
        let path = root.join("revisions").join(&installed.record.install_id);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StoreError::Corrupt("listed revision directory missing".to_owned())
            } else {
                io_error("inspect listed revision", error)
            }
        })?;
        if is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(StoreError::Corrupt(
                "listed revision is not a real directory".to_owned(),
            ));
        }
        verify_revision(root, installed, catalog, limits, false)?;
    }
    Ok(())
}

fn validate_active(
    root: &Path,
    registry: &Registry,
    active: Option<&ActivePointer>,
) -> Result<(), StoreError> {
    let Some(active) = active else {
        return Ok(());
    };
    let installed = registry
        .records
        .iter()
        .find(|installed| installed.record.install_id == active.install_id)
        .ok_or_else(|| StoreError::Corrupt("active pointer names unknown install".to_owned()))?;
    if installed.record.package_id != active.package_id
        || installed.record.revision != active.revision
        || installed.record.container_sha256 != active.container_sha256
    {
        return Err(StoreError::Corrupt(
            "active pointer does not match registry".to_owned(),
        ));
    }
    let revision_path = root.join("revisions").join(&active.install_id);
    let metadata = fs::symlink_metadata(revision_path)
        .map_err(|error| StoreError::Corrupt(format!("active revision missing: {error}")))?;
    if is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(StoreError::Corrupt(
            "active revision is not a real directory".to_owned(),
        ));
    }
    Ok(())
}

fn verify_revision(
    root: &Path,
    installed: &StoredRecord,
    catalog: &Catalog,
    limits: Limits,
    verify_payloads: bool,
) -> Result<Descriptor, StoreError> {
    let revision_dir = root.join("revisions").join(&installed.record.install_id);
    ensure_store_tree_no_links(&revision_dir)?;
    let descriptor_path = revision_dir.join("descriptor.json");
    let descriptor_bytes = read_store_file(&descriptor_path, limits.descriptor_bytes)?;
    if sha256(&descriptor_bytes) != installed.descriptor_sha256 {
        return Err(StoreError::Corrupt(
            "installed descriptor digest mismatch".to_owned(),
        ));
    }
    let parsed =
        descriptor::parse_descriptor(&descriptor_bytes, catalog, &limits).map_err(|reason| {
            StoreError::Corrupt(format!("installed descriptor: {}", reason.as_str()))
        })?;
    if parsed.descriptor.package_id != installed.record.package_id
        || parsed.descriptor.revision != installed.record.revision
    {
        return Err(StoreError::Corrupt(
            "installed descriptor identity mismatch".to_owned(),
        ));
    }
    if verify_payloads {
        for file in &parsed.descriptor.files {
            let path = revision_payload_path(root, &installed.record.install_id, &file.path)
                .map_err(|reason| {
                    StoreError::Corrupt(format!("installed payload path: {}", reason.as_str()))
                })?;
            verify_payload_file(&path, file.size_bytes, &file.sha256).map_err(|reason| {
                StoreError::Corrupt(format!("installed payload: {}", reason.as_str()))
            })?;
        }
    }
    Ok(parsed.descriptor)
}

fn recover_leftovers(root: &Path, registry: &Registry) -> Result<(), StoreError> {
    let staging = root.join("staging");
    for entry in fs::read_dir(&staging).map_err(|error| io_error("read staging", error))? {
        let entry = entry.map_err(|error| io_error("read staging entry", error))?;
        remove_path(&entry.path())?;
    }

    let installed_ids: HashSet<&str> = registry
        .records
        .iter()
        .map(|installed| installed.record.install_id.as_str())
        .collect();
    let revisions = root.join("revisions");
    for entry in fs::read_dir(&revisions).map_err(|error| io_error("read revisions", error))? {
        let entry = entry.map_err(|error| io_error("read revision entry", error))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io_error("inspect revision entry", error))?;
        let name = entry.file_name();
        let listed = name
            .to_str()
            .is_some_and(|name| installed_ids.contains(name));
        let is_unlisted_revision = name
            .to_str()
            .is_some_and(|name| valid_install_id(name).is_some() && !listed);
        if is_unlisted_revision && (metadata.is_dir() || is_link_or_reparse_point(&metadata)) {
            remove_path(&path)?;
        }
    }

    remove_recovery_temp_file(&root.join("registry.json.tmp"))?;
    remove_recovery_temp_file(&root.join("active.json.tmp"))?;
    Ok(())
}

fn revision_payload_path(
    root: &Path,
    install_id: &str,
    relative: &str,
) -> Result<PathBuf, RejectReason> {
    if valid_install_id(install_id).is_none() || !valid_payload_path(relative) {
        return Err(RejectReason::UnsafePath);
    }
    let revisions = root.join("revisions");
    for directory in [root, revisions.as_path()] {
        let metadata =
            fs::symlink_metadata(directory).map_err(|_| RejectReason::SourceUnavailable)?;
        if is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(RejectReason::UnsafePath);
        }
    }
    let revision = root.join("revisions").join(install_id);
    let revision_metadata =
        fs::symlink_metadata(&revision).map_err(|_| RejectReason::SourceUnavailable)?;
    if is_link_or_reparse_point(&revision_metadata) || !revision_metadata.is_dir() {
        return Err(RejectReason::UnsafePath);
    }
    let mut path = revision.clone();
    let components = Path::new(relative).components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(segment) = component else {
            return Err(RejectReason::UnsafePath);
        };
        path.push(segment);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if is_link_or_reparse_point(&metadata) {
                    return Err(RejectReason::UnsafePath);
                }
                if index + 1 < components.len() && !metadata.is_dir() {
                    return Err(RejectReason::SourceUnavailable);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(RejectReason::SourceUnavailable),
        }
        if !path.starts_with(&revision) {
            return Err(RejectReason::UnsafePath);
        }
    }
    if !path.starts_with(&revision) {
        return Err(RejectReason::UnsafePath);
    }
    Ok(path)
}

fn read_resolve_file(path: &Path, maximum: u64) -> Result<Vec<u8>, RejectReason> {
    let metadata = fs::symlink_metadata(path).map_err(|_| RejectReason::SourceUnavailable)?;
    if is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(RejectReason::SourceUnavailable);
    }
    if metadata.len() > maximum {
        return Err(RejectReason::EntryTooLarge);
    }
    let limit = maximum.checked_add(1).ok_or(RejectReason::EntryTooLarge)?;
    let capacity = usize::try_from(metadata.len()).map_err(|_| RejectReason::EntryTooLarge)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| RejectReason::EntryTooLarge)?;
    File::open(path)
        .map_err(|_| RejectReason::SourceUnavailable)?
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|_| RejectReason::SourceUnavailable)?;
    if u64::try_from(bytes.len()).map_or(true, |length| length > maximum) {
        return Err(RejectReason::EntryTooLarge);
    }
    Ok(bytes)
}

fn verify_payload_file(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
) -> Result<(), RejectReason> {
    if expected_size > MAX_PAYLOAD_BYTES || !valid_sha256(expected_sha256) {
        return Err(RejectReason::EntryTooLarge);
    }
    let bytes = read_resolve_file(path, MAX_PAYLOAD_BYTES)?;
    let actual_size = u64::try_from(bytes.len()).map_err(|_| RejectReason::SizeMismatch)?;
    if actual_size != expected_size {
        return Err(RejectReason::SizeMismatch);
    }
    if sha256(&bytes) != expected_sha256 {
        return Err(RejectReason::DigestMismatch);
    }
    Ok(())
}
