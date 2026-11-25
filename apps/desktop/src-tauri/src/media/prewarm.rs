//! Off-engine media loading for the immutable audio table and visual allowlist.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use kohbor_audio::pcm::{Pcm, decode_wav};
use kohbor_events::{Event, EventId};
use kohbor_pack::catalog::Catalog;
use kohbor_pack::descriptor::Selection;
use kohbor_pack::error::RejectReason;
use kohbor_pack::media::inspect_wav;
use kohbor_pack::store::{Hook, ResolveError, ResolvedBinding, ResolvedVariant, Store};
use sha2::{Digest, Sha256};

use crate::audio::resolve::{
    Clip, Entry, PREWARM_CAP_REASON, PrewarmTable, SelectedClip, VariantMedia, VariantSelection,
    VariantSet,
};

/// Verified visual file metadata captured alongside the audio table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub modified: SystemTime,
    pub content_type: &'static str,
}

/// Renderer-visible files from the one revision represented by the table.
#[derive(Debug, Clone, Default)]
pub struct VisualIndex {
    /// Legacy primary ID retained for the first-generation preview path.
    pub install_id: Option<Arc<str>>,
    pub install_ids: Vec<Arc<str>>,
    pub entries: HashMap<(String, String), VisualEntry>,
    /// Exact verified routes published for drawer previews of non-active packs.
    pub preview_entries: HashMap<String, VisualEntry>,
    /// One verified card cover per installed revision, separate from playback.
    pub covers: Arc<HashMap<String, VisualEntry>>,
}

pub type VisualIndexHandle = Arc<Mutex<Arc<VisualIndex>>>;

pub fn new_visual_index_handle() -> VisualIndexHandle {
    Arc::new(Mutex::new(Arc::new(VisualIndex::default())))
}

pub const MAX_PREWARMED_SAMPLES: u64 = 40_000_000;
const AUDIO_SAMPLE_RATE: u64 = 48_000;

/// Return every voiced host event's tier-less key and catalog-defined tiers.
pub fn key_space(catalog: &Catalog) -> Vec<(EventId, Option<u8>)> {
    let mut keys = Vec::new();
    for id in crate::media::inventory::ALL_EVENTS
        .iter()
        .copied()
        .filter(|id| id.voiced())
    {
        keys.push((id, None));
        if let Some((minimum, maximum)) = catalog
            .event(id.as_str())
            .and_then(|event| event.tier_range)
        {
            // Catalog::from_json bounds tiers at 100. Keep this safe for callers
            // that construct Catalog directly instead of using that validator.
            if minimum == 0 || maximum < minimum || maximum > 100 {
                continue;
            }
            for tier in minimum..=maximum {
                if let Ok(tier) = u8::try_from(tier) {
                    keys.push((id, Some(tier)));
                }
            }
        }
    }
    keys
}

/// Return one cabinet key for each voiced detector event and catalog tier.
/// Tiered events have only their declared tier tiles; the tier-less binding,
/// when present, remains an admission fallback and is reflected in each tile.
pub fn cabinet_key_space(catalog: &Catalog) -> Vec<(EventId, Option<u8>)> {
    let mut keys = Vec::new();
    for id in crate::media::inventory::ALL_EVENTS
        .iter()
        .copied()
        .filter(|id| id.voiced())
    {
        let Some(event) = catalog.event(id.as_str()) else {
            continue;
        };
        if let Some((minimum, maximum)) = event.tier_range {
            if minimum == 0 || maximum < minimum || maximum > 100 {
                continue;
            }
            for tier in minimum..=maximum {
                if let Ok(tier) = u8::try_from(tier) {
                    keys.push((id, Some(tier)));
                }
            }
        } else {
            keys.push((id, None));
        }
    }
    keys
}

/// Prewarm critical clips and then the first variant of every other binding.
/// Use the remaining PCM budget for additional variants in descriptor order.
/// The cap bounds retained samples across unique audio paths. Even at 15 seconds
/// each, the 23 current event/tier keys need less than 17 million samples for
/// their first variants. File I/O and decoding stay off the audio engine.
#[derive(Debug)]
struct PlannedVariant {
    resolved: ResolvedVariant,
    source: crate::audio::resolve::VariantSource,
    authored_selection: Option<VariantSelection>,
    ordinal: usize,
    loaded: Option<Result<Clip, &'static str>>,
}

#[derive(Debug)]
struct PlannedEntry {
    id: EventId,
    tier: Option<u8>,
    critical: bool,
    selection: Option<VariantSelection>,
    variants: Vec<PlannedVariant>,
}

fn detector_event(id: &str) -> Option<EventId> {
    crate::media::inventory::ALL_EVENTS
        .iter()
        .copied()
        .find(|event| event.as_str() == id)
}

fn selection_mode(selection: Option<Selection>) -> Option<VariantSelection> {
    selection.map(|selection| match selection {
        Selection::Random => VariantSelection::Random,
        Selection::Shuffle => VariantSelection::Shuffle,
        Selection::Sequence => VariantSelection::Sequence,
    })
}

/// Merge every equipped pack into one bounded, immutable table. Descriptor
/// verification, file reads, SHA-256 deduplication and decoding all run here,
/// before the engine receives the replacement table.
pub fn build_table<H: Hook>(
    store: &Store<H>,
    catalog: &Catalog,
    pause: &mut dyn FnMut(),
) -> (Arc<PrewarmTable>, VisualIndex) {
    let started = std::time::Instant::now();
    let loadout = store.loadout();
    if loadout.equipped.is_empty() {
        let table = PrewarmTable::new_loadout(
            loadout.generation,
            Vec::new(),
            loadout.match_consistency,
            Vec::new(),
        );
        return (Arc::new(table), VisualIndex::default());
    }

    let equipped_ids: Vec<Arc<str>> = loadout
        .equipped
        .iter()
        .map(|pack| Arc::from(pack.install_id.as_str()))
        .collect();
    let mut visual_index = VisualIndex {
        install_id: equipped_ids.first().cloned(),
        install_ids: equipped_ids.clone(),
        entries: HashMap::new(),
        preview_entries: HashMap::new(),
        covers: Default::default(),
    };

    // Resolve each descriptor once. A revision that cannot be reverified is
    // excluded as a whole; the table keeps the remaining equipped slots ordered.
    let mut bindings_by_slot: Vec<Vec<ResolvedBinding>> =
        Vec::with_capacity(loadout.equipped.len());
    for pack in &loadout.equipped {
        pause();
        match store.resolve_installed_bindings(&pack.install_id) {
            Ok(bindings) => bindings_by_slot.push(bindings),
            Err(error) => {
                crate::log_info!(
                    "audio",
                    "loadout_pack_unavailable",
                    "an equipped announcer pack failed verification",
                    install_id = &pack.install_id,
                    reason = format!("{error:?}"),
                );
                bindings_by_slot.push(Vec::new());
            }
        }
    }

    let mut plans = Vec::<PlannedEntry>::new();
    let mut keys = key_space(catalog);
    keys.sort_by_key(|(id, tier)| (id.as_str(), tier.unwrap_or(0)));
    for (id, tier) in keys {
        pause();
        let locked_package = loadout.event_locks.get(id.as_str());
        let locked_slot = locked_package.and_then(|package_id| {
            loadout
                .equipped
                .iter()
                .position(|pack| &pack.package_id == package_id)
                .filter(|slot| {
                    bindings_by_slot
                        .get(*slot)
                        .into_iter()
                        .flatten()
                        .filter(|binding| binding.event_id == id.as_str())
                        .any(|binding| binding.variants.iter().any(|variant| variant.media.is_ok()))
                })
        });
        let candidate_slots: Vec<usize> = if let Some(slot) = locked_slot {
            vec![slot]
        } else {
            (0..loadout.equipped.len()).collect()
        };

        // A tier-less binding cannot be selected if an equipped pack binds the
        // event's minimum declared tier; do not spend PCM budget on it.
        if tier.is_none() {
            let impossible_tierless = catalog
                .event(id.as_str())
                .and_then(|event| event.tier_range)
                .is_some_and(|(minimum, _)| {
                    candidate_slots.iter().any(|slot| {
                        bindings_by_slot
                            .get(*slot)
                            .into_iter()
                            .flatten()
                            .any(|binding| {
                                binding.event_id == id.as_str() && binding.tier == Some(minimum)
                            })
                    })
                });
            if impossible_tierless {
                continue;
            }
        }

        let mut source_bindings = Vec::new();
        for slot in candidate_slots {
            let Some(pack) = loadout.equipped.get(slot) else {
                continue;
            };
            let binding = bindings_by_slot
                .get(slot)
                .into_iter()
                .flatten()
                .find(|binding| {
                    binding.event_id == id.as_str()
                        && binding.tier.and_then(|value| u8::try_from(value).ok()) == tier
                });
            if let Some(binding) = binding {
                source_bindings.push((slot, pack, binding));
            }
        }
        if source_bindings.is_empty() {
            continue;
        }

        let critical = catalog
            .event(id.as_str())
            .is_some_and(|event| event.critical_media_required);
        if critical {
            let chosen = source_bindings
                .iter()
                .find(|(_, _, binding)| {
                    binding.variants.iter().any(|variant| variant.media.is_ok())
                })
                .or_else(|| source_bindings.first());
            let Some((slot, pack, binding)) = chosen else {
                continue;
            };
            let selected = binding
                .variants
                .iter()
                .position(|variant| variant.media.is_ok())
                .unwrap_or(0);
            let variant = binding.variants.get(selected).cloned();
            let Some(variant) = variant else { continue };
            let mode = selection_mode(binding.selection);
            plans.push(PlannedEntry {
                id,
                tier,
                critical: true,
                selection: mode,
                variants: vec![PlannedVariant {
                    resolved: variant,
                    source: crate::audio::resolve::VariantSource {
                        slot: u8::try_from(*slot).unwrap_or(u8::MAX),
                        install_id: Arc::from(pack.install_id.as_str()),
                        package_id: Arc::from(pack.package_id.as_str()),
                    },
                    authored_selection: mode,
                    ordinal: selected,
                    loaded: None,
                }],
            });
            continue;
        }

        let mixed = source_bindings.len() > 1;
        let selection = if mixed {
            Some(VariantSelection::Shuffle)
        } else {
            source_bindings
                .first()
                .and_then(|(_, _, binding)| selection_mode(binding.selection))
        };
        let mut variants = Vec::new();
        for (slot, pack, binding) in source_bindings {
            let authored_selection = selection_mode(binding.selection);
            for (ordinal, resolved) in binding.variants.iter().cloned().enumerate() {
                variants.push(PlannedVariant {
                    resolved,
                    source: crate::audio::resolve::VariantSource {
                        slot: u8::try_from(slot).unwrap_or(u8::MAX),
                        install_id: Arc::from(pack.install_id.as_str()),
                        package_id: Arc::from(pack.package_id.as_str()),
                    },
                    authored_selection,
                    ordinal,
                    loaded: None,
                });
            }
        }
        plans.push(PlannedEntry {
            id,
            tier,
            critical,
            selection,
            variants,
        });
    }

    // Build deterministic R0-R3 priority rounds. Duplicates are removed so
    // every payload is considered once, even when it is shared by two packs.
    let mut schedule = Vec::<(usize, usize)>::new();
    let mut scheduled = std::collections::HashSet::<(usize, usize)>::new();
    let mut push_schedule = |entry: usize, variant: usize| {
        if scheduled.insert((entry, variant)) {
            schedule.push((entry, variant));
        }
    };
    for (entry_index, entry) in plans.iter().enumerate().filter(|(_, entry)| entry.critical) {
        if !entry.variants.is_empty() {
            push_schedule(entry_index, 0);
        }
    }
    for entry_index in 0..plans.len() {
        if plans[entry_index].critical {
            continue;
        }
        let Some(first_slot) = plans[entry_index]
            .variants
            .iter()
            .map(|variant| variant.source.slot)
            .min()
        else {
            continue;
        };
        if let Some(first) = plans[entry_index]
            .variants
            .iter()
            .position(|variant| variant.source.slot == first_slot)
        {
            push_schedule(entry_index, first);
        }
    }
    for slot in 0..loadout.equipped.len() {
        for entry_index in 0..plans.len() {
            if plans[entry_index].critical {
                continue;
            }
            let Some(first) = plans[entry_index]
                .variants
                .iter()
                .position(|variant| usize::from(variant.source.slot) == slot)
            else {
                continue;
            };
            if plans[entry_index]
                .variants
                .get(first)
                .is_some_and(|variant| {
                    variant.source.slot
                        != plans[entry_index]
                            .variants
                            .first()
                            .map_or(variant.source.slot, |first| first.source.slot)
                })
            {
                push_schedule(entry_index, first);
            }
        }
    }
    for ordinal in 1..16 {
        for entry_index in 0..plans.len() {
            if plans[entry_index].critical {
                continue;
            }
            for slot in 0..loadout.equipped.len() {
                if let Some(index) = plans[entry_index].variants.iter().position(|variant| {
                    usize::from(variant.source.slot) == slot && variant.ordinal == ordinal
                }) {
                    push_schedule(entry_index, index);
                }
            }
        }
    }
    drop(push_schedule);

    let mut pcm_by_digest: HashMap<[u8; 32], Pcm> = HashMap::new();
    let mut remaining_samples = MAX_PREWARMED_SAMPLES;
    let mut used_samples = 0u64;
    let mut skipped_variants = 0usize;
    for (entry_index, variant_index) in schedule {
        pause();
        let Some(entry) = plans.get_mut(entry_index) else {
            continue;
        };
        let Some(variant) = entry.variants.get_mut(variant_index) else {
            continue;
        };
        let result = load_planned_variant(
            &store
                .root()
                .join("revisions")
                .join(variant.source.install_id.as_ref()),
            entry.id,
            entry.tier,
            variant,
            &mut remaining_samples,
            &mut used_samples,
            &mut pcm_by_digest,
            &mut visual_index,
        );
        if result.as_ref().err().copied() == Some(PREWARM_CAP_REASON) {
            skipped_variants = skipped_variants.saturating_add(1);
        }
        variant.loaded = Some(result);
    }

    let entries = plans
        .into_iter()
        .map(|plan| {
            let variants: Vec<VariantMedia> = plan
                .variants
                .into_iter()
                .map(|variant| VariantMedia {
                    variant_id: variant.resolved.variant_id,
                    clip_id: variant.resolved.clip_id,
                    caption: variant.resolved.caption,
                    weight: variant
                        .resolved
                        .media
                        .as_ref()
                        .ok()
                        .and_then(|media| media.weight)
                        .unwrap_or(1),
                    source: Some(variant.source),
                    authored_selection: variant.authored_selection,
                    media: variant
                        .loaded
                        .unwrap_or_else(|| match variant.resolved.media {
                            Ok(_) => Err("prewarm_not_scheduled"),
                            Err(reason) => Err(reason.as_str()),
                        }),
                })
                .collect();
            let entry = if variants.iter().any(|variant| variant.media.is_ok()) {
                Entry::Variants(VariantSet {
                    selection: plan.selection,
                    variants,
                })
            } else {
                Entry::Variants(VariantSet {
                    selection: plan.selection,
                    variants,
                })
            };
            (plan.id, plan.tier, entry)
        })
        .collect();

    let mut table = PrewarmTable::new_loadout(
        loadout.generation,
        equipped_ids,
        loadout.match_consistency,
        entries,
    );
    table.used_samples = used_samples;
    table.skipped_variants = skipped_variants;
    crate::log_info!(
        "audio",
        "loadout_prewarm_finished",
        "merged announcer loadout prewarm finished",
        used_samples = used_samples,
        cap_samples = MAX_PREWARMED_SAMPLES,
        skipped_variants = skipped_variants,
        duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    );
    (Arc::new(table), visual_index)
}

fn load_planned_variant(
    revision_root: &Path,
    event_id: EventId,
    tier: Option<u8>,
    variant: &PlannedVariant,
    remaining_samples: &mut u64,
    used_samples: &mut u64,
    pcm_by_digest: &mut HashMap<[u8; 32], Pcm>,
    visual_index: &mut VisualIndex,
) -> Result<Clip, &'static str> {
    let media = variant
        .resolved
        .media
        .as_ref()
        .map_err(|reason| reason.as_str())?;
    if media.install_id != variant.source.install_id.as_ref() {
        return Err(RejectReason::UnknownInstall.as_str());
    }
    let visual = media
        .silent_visual
        .as_deref()
        .map(|path| {
            visual_entry(revision_root, path, "video/webm")
                .ok_or(RejectReason::VisualInvalid.as_str())
        })
        .transpose()?;
    let audio_bytes =
        fs::read(&media.mixed_audio).map_err(|_| RejectReason::SourceUnavailable.as_str())?;
    let digest: [u8; 32] = Sha256::digest(&audio_bytes).into();
    let pcm = if let Some(pcm) = pcm_by_digest.get(&digest) {
        pcm.clone()
    } else {
        let samples = inspect_wav(&audio_bytes, RejectReason::MixedAudioInvalid)
            .map_err(|reason| reason.as_str())?
            .samples;
        if samples > *remaining_samples {
            crate::log_info!(
                "audio",
                "clip_variant_skipped",
                "announcer variant exceeds the prewarm budget",
                event_id = event_id.as_str(),
                tier = tier,
                variant_id = &variant.resolved.variant_id,
                reason = PREWARM_CAP_REASON,
            );
            return Err(PREWARM_CAP_REASON);
        }
        let pcm = decode_wav(&audio_bytes)
            .filter(|pcm| u64::try_from(pcm.len()).ok() == Some(samples))
            .ok_or(RejectReason::MixedAudioInvalid.as_str())?;
        *remaining_samples -= samples;
        *used_samples = (*used_samples).saturating_add(samples);
        pcm_by_digest.insert(digest, pcm.clone());
        pcm
    };
    let visual_url = visual.map(|(token, entry)| {
        visual_index
            .entries
            .insert((media.install_id.clone(), token.clone()), entry);
        Arc::<str>::from(format!(
            "kohbor-media://localhost/v1/{}/{token}",
            media.install_id
        ))
    });
    Ok(Clip {
        audio_ms: audio_duration_ms(u64::try_from(pcm.len()).unwrap_or(u64::MAX)),
        pcm,
        bound_tier: tier,
        visual_tail_ms: media.visual_tail_ms,
        visual_url,
    })
}

pub struct PreviewVariant {
    pub selected: SelectedClip,
    pub visual: Option<PreviewVisual>,
}

pub struct PreviewVisual {
    pub route: String,
    pub url: String,
    pub entry: VisualEntry,
}

/// Decode one store-verified installed variant for the isolated drawer audition path.
/// The returned visual URL is logical only; callers publish its file in the exact
/// preview allowlist before exposing it to the renderer.
pub fn build_preview_variant<H: Hook>(
    store: &Store<H>,
    install_id: &str,
    event: Event,
    variant_id: &str,
) -> Result<PreviewVariant, &'static str> {
    let media = store
        .resolve_installed_variant(
            install_id,
            event.id.as_str(),
            event.tier.map(u32::from),
            variant_id,
        )
        .map_err(|error| match error {
            ResolveError::Unmapped | ResolveError::Inert => "AUDITION_VARIANT_UNAVAILABLE",
            ResolveError::Invalid(_) | ResolveError::NoVerifiedVariants(_) => "MEDIA_INVALID",
        })?;
    if media.variant_id != variant_id {
        return Err("AUDITION_VARIANT_UNAVAILABLE");
    }

    let revision_root = store.root().join("revisions").join(&media.install_id);
    let visual_data = media
        .silent_visual
        .as_deref()
        .map(|path| {
            let (token, entry) =
                visual_entry(&revision_root, path, "video/webm").ok_or("MEDIA_INVALID")?;
            let route = format!("/v1/{}/{token}", media.install_id);
            let url = format!("kohbor-media://localhost{route}");
            Ok::<_, &'static str>(PreviewVisual { route, url, entry })
        })
        .transpose()?;
    let audio_bytes = fs::read(&media.mixed_audio).map_err(|_| "MEDIA_INVALID")?;
    let samples = inspect_wav(&audio_bytes, RejectReason::MixedAudioInvalid)
        .map_err(|_| "MEDIA_INVALID")?
        .samples;
    let pcm = decode_wav(&audio_bytes)
        .filter(|pcm| u64::try_from(pcm.len()).ok() == Some(samples))
        .ok_or("MEDIA_INVALID")?;
    let selection = media.selection.map(|selection| match selection {
        Selection::Random => VariantSelection::Random,
        Selection::Shuffle => VariantSelection::Shuffle,
        Selection::Sequence => VariantSelection::Sequence,
    });
    let clip = Clip {
        pcm,
        bound_tier: media.tier.and_then(|tier| u8::try_from(tier).ok()),
        audio_ms: audio_duration_ms(samples),
        visual_tail_ms: media.visual_tail_ms,
        visual_url: visual_data
            .as_ref()
            .map(|visual| Arc::<str>::from(visual.url.as_str())),
    };
    let selected = SelectedClip {
        event_id: event.id,
        bound_tier: clip.bound_tier,
        variant_index: 0,
        variant_id: media.variant_id,
        clip_id: media.clip_id,
        selection,
        source: None,
        match_consistency: false,
        media: clip,
    };
    Ok(PreviewVariant {
        selected,
        visual: visual_data,
    })
}

fn install_number(install_id: &str) -> u64 {
    let Some(digits) = install_id.strip_prefix("ins_") else {
        return 0;
    };
    if digits.len() != 10 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return 0;
    }
    digits.parse::<u64>().unwrap_or_default()
}

fn audio_duration_ms(samples: u64) -> u32 {
    let milliseconds = samples
        .saturating_mul(1_000)
        .saturating_add(AUDIO_SAMPLE_RATE.saturating_sub(1))
        / AUDIO_SAMPLE_RATE;
    u32::try_from(milliseconds).unwrap_or(u32::MAX)
}

pub fn visual_entry(
    revision_root: &Path,
    path: &Path,
    content_type: &'static str,
) -> Option<(String, VisualEntry)> {
    let relative = path.strip_prefix(revision_root).ok()?;
    let descriptor_path = relative.to_str()?.replace('\\', "/");
    if descriptor_path.is_empty() || descriptor_path.starts_with('/') {
        return None;
    }
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return None;
    }
    let modified = metadata.modified().ok()?;
    let digest = Sha256::digest(descriptor_path.as_bytes());
    let token = format!("{digest:x}");
    Some((
        token,
        VisualEntry {
            path: path.to_path_buf(),
            size_bytes: metadata.len(),
            modified,
            content_type,
        },
    ))
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use kohbor_pack::catalog::Catalog;
    use kohbor_pack::store::{
        ActivationPolicy, ActivationState, InstallRequest, InstallResult, Store,
    };
    use kohbor_pack_author::{BuildOptions, build_package};
    use serde_json::{Value, json};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(label: &str) -> Self {
            // CARGO_TARGET_TMPDIR exists only for integration tests, not for lib unit
            // tests: keep the scratch area inside the target directory by walking up
            // from the test executable (target/<profile>/deps/<exe>) to target/.
            let base = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.ancestors().nth(3).map(|target| target.join("tmp")))
                .unwrap_or_else(std::env::temp_dir);
            fs::create_dir_all(&base).expect("create Cargo target temporary root");
            let index = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0);
            let path = base.join(format!(
                "kohbor-s3-5i-{label}-{}-{nanos}-{index}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create unique media test directory");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Clone, Copy)]
    pub struct BindingSpec {
        pub event_id: &'static str,
        pub tier: Option<u32>,
        pub samples: usize,
        pub visual: bool,
        pub visual_tail_ms: u32,
    }

    pub struct Package {
        pub package_path: PathBuf,
        pub digest: String,
        pub package_id: String,
    }

    pub fn catalog() -> Catalog {
        let value: Value =
            serde_json::from_str(include_str!("../../../../../contracts/event-catalog.json"))
                .expect("repository event catalog parses");
        Catalog::from_json(&value).expect("repository event catalog is valid for package use")
    }

    pub fn package(temp: &TempDir, package_id: &str, bindings: &[BindingSpec]) -> Package {
        package_with_options(temp, package_id, bindings, None, None, None)
    }

    pub fn package_with_caption(
        temp: &TempDir,
        package_id: &str,
        binding: BindingSpec,
        caption: &str,
    ) -> Package {
        package_with_options(temp, package_id, &[binding], None, None, Some(caption))
    }

    pub fn package_with_cover(
        temp: &TempDir,
        package_id: &str,
        bindings: &[BindingSpec],
        cover_bytes: &[u8],
    ) -> Package {
        package_with_options(
            temp,
            package_id,
            bindings,
            Some(kohbor_pack_author::PresentationOptions {
                description: Some("Synthetic cover fixture".to_owned()),
                author: Some("Kohbor test".to_owned()),
                languages: Some(vec!["en-US".to_owned()]),
                cover: Some("cover.png".to_owned()),
            }),
            Some(cover_bytes.to_vec()),
            None,
        )
    }

    fn package_with_options(
        temp: &TempDir,
        package_id: &str,
        bindings: &[BindingSpec],
        presentation: Option<kohbor_pack_author::PresentationOptions>,
        cover_bytes: Option<Vec<u8>>,
        caption: Option<&str>,
    ) -> Package {
        let clips = temp.path().join("clips");
        fs::create_dir_all(&clips).expect("create generated input clip directory");
        if let Some(bytes) = cover_bytes {
            fs::write(clips.join("cover.png"), bytes).expect("write generated still cover");
        }
        let mut mapping_bindings = Vec::with_capacity(bindings.len());
        for (index, binding) in bindings.iter().enumerate() {
            let audio_name = format!("clip-{index}.wav");
            fs::write(clips.join(&audio_name), pcm_wav(binding.samples))
                .expect("write generated PCM16 WAV input");
            let mut row = json!({
                "event_id": binding.event_id,
                "audio": audio_name,
                "visual_tail_ms": binding.visual_tail_ms,
            });
            if let Some(tier) = binding.tier {
                row["tier"] = json!(tier);
            }
            if binding.visual {
                let visual_name = format!("visual-{index}.webm");
                fs::write(clips.join(&visual_name), webm())
                    .expect("write generated single-track VP9 WebM input");
                row["visual"] = json!(visual_name);
            }
            if let Some(caption) = caption {
                let mut variant = row;
                let mut fields = json!({ "event_id": binding.event_id });
                if let Some(tier) = binding.tier {
                    fields["tier"] = json!(tier);
                }
                for key in ["event_id", "tier"] {
                    variant.as_object_mut().unwrap().remove(key);
                }
                variant["variant_id"] = json!("v1");
                variant["clip_id"] = json!("kohbor-en-0001");
                variant["caption"] = json!(caption);
                fields["variants"] = json!([variant]);
                row = fields;
            }
            mapping_bindings.push(row);
        }
        let mapping = temp.path().join("mapping.json");
        let mapping_bytes = serde_json::to_vec(&json!({
            "schema_version": "1.0.0",
            "bindings": mapping_bindings,
        }))
        .expect("serialize generated mapping");
        fs::write(mapping, mapping_bytes).expect("write generated mapping");

        let output = temp.path().join(format!("out-{package_id}"));
        let options = BuildOptions {
            clips_dir: clips,
            map_path: temp.path().join("mapping.json"),
            out_dir: output,
            package_id: package_id.to_owned(),
            revision: 1,
            display_name: "Synthetic media service fixture".to_owned(),
            catalog_path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../contracts/event-catalog.json"),
            rights_claim: None,
            publisher_key_id: None,
            require_visual: false,
            presentation,
        };
        let report = build_package(&options).expect("author generated fixture package");
        Package {
            package_path: report.package_path,
            digest: report.container_sha256,
            package_id: package_id.to_owned(),
        }
    }

    pub fn install_request_json(package: &Package, request_id: &str, policy: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema_version": "1.0.0",
            "request_id": request_id,
            "profile_id": "test-profile",
            "source_path": package.package_path.to_string_lossy(),
            "expected_container_sha256": package.digest,
            "project_binding": null,
            "activation_policy": policy,
        }))
        .expect("serialize valid install request")
    }

    pub fn install(
        store: &mut Store<kohbor_pack::store::NoHook>,
        package: &Package,
        request_id: &str,
    ) -> InstallResult {
        let request_bytes = install_request_json(package, request_id, "never");
        let request =
            InstallRequest::from_json(&request_bytes).expect("parse test install request");
        store.install(&request).expect("install generated package")
    }

    pub fn activate(
        store: &mut Store<kohbor_pack::store::NoHook>,
        request_id: &str,
        install_id: &str,
    ) -> InstallResult {
        store
            .activate(
                request_id,
                install_id,
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .expect("activate generated package")
    }

    pub fn pcm_wav(samples: usize) -> Vec<u8> {
        let data_len = u32::try_from(samples.saturating_mul(2)).expect("fixture WAV fits u32");
        let riff_len = 36_u32.checked_add(data_len).expect("fixture RIFF fits u32");
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&riff_len.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&48_000_u32.to_le_bytes());
        bytes.extend_from_slice(&96_000_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        let payload = usize::try_from(data_len).expect("fixture data size fits usize");
        bytes.resize(
            44_usize
                .checked_add(payload)
                .expect("fixture WAV fits usize"),
            0,
        );
        bytes
    }

    fn ebml_element(id: &[u8], data: &[u8]) -> Vec<u8> {
        let size = u8::try_from(data.len()).expect("test EBML element is small");
        let mut bytes = id.to_vec();
        bytes.push(0x80 | size);
        bytes.extend_from_slice(data);
        bytes
    }

    fn ebml_uint(id: &[u8], value: u64) -> Vec<u8> {
        let data = value.to_be_bytes();
        let first = data
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(data.len().saturating_sub(1));
        ebml_element(id, &data[first..])
    }

    pub fn webm() -> Vec<u8> {
        let mut header = ebml_element(&[0x42, 0x82], b"webm");
        header.extend(ebml_uint(&[0x42, 0x86], 1));
        let mut video = ebml_uint(&[0xb0], 640);
        video.extend(ebml_uint(&[0xba], 360));
        let mut track = ebml_uint(&[0xd7], 1);
        track.extend(ebml_uint(&[0x73, 0xc5], 1));
        track.extend(ebml_uint(&[0x83], 1));
        track.extend(ebml_element(&[0x86], b"V_VP9"));
        track.extend(ebml_element(&[0xe0], &video));
        let tracks = ebml_element(&[0xae], &track);
        let segment = ebml_element(&[0x16, 0x54, 0xae, 0x6b], &tracks);
        let mut bytes = ebml_element(&[0x1a, 0x45, 0xdf, 0xa3], &header);
        bytes.extend(ebml_element(&[0x18, 0x53, 0x80, 0x67], &segment));
        bytes
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::sync::atomic::Ordering;

    use kohbor_audio::CriticalMedia;
    use kohbor_events::EventId;
    use kohbor_pack::Limits;
    use kohbor_pack::error::RejectReason;
    use kohbor_pack::store::{
        ActivationPolicy, ActivationState, LoadoutDesired, ResolveError, Store,
    };
    use serde_json::Value;
    use sha2::{Digest, Sha256};

    use super::test_support::{
        BindingSpec, Package, TempDir, activate, catalog, install, package, pcm_wav,
    };
    use super::{build_table, cabinet_key_space, key_space, new_visual_index_handle};
    use crate::audio::lanes::EngineShared;
    use crate::audio::resolve::{BindingLookup, Lookup};
    use crate::media::inventory::{ALL_EVENTS, facts_from_table, media_status};
    use crate::media::selection::VariantSelector;

    fn open_store(root: &TempDir) -> Store<kohbor_pack::store::NoHook> {
        Store::open(&root.path().join("store"), catalog(), Limits::default())
            .expect("open isolated media store")
    }

    fn audio(event_id: &'static str, samples: usize) -> BindingSpec {
        BindingSpec {
            event_id,
            tier: None,
            samples,
            visual: false,
            visual_tail_ms: 0,
        }
    }

    fn activated_store(
        root: &TempDir,
        package_id: &str,
        bindings: &[BindingSpec],
        request_prefix: &str,
    ) -> (Store<kohbor_pack::store::NoHook>, String) {
        let mut store = open_store(root);
        let pkg = package(root, package_id, bindings);
        let installed = install(&mut store, &pkg, &format!("{request_prefix}-install"));
        let install_id = installed
            .record
            .expect("generated package install returns a record")
            .install_id;
        let activated = activate(&mut store, &format!("{request_prefix}-active"), &install_id);
        assert_eq!(
            activated.status.as_str(),
            "active",
            "generated package activates"
        );
        (store, install_id)
    }

    #[test]
    fn oversized_pcm_pack_keeps_critical_and_first_variants_playable() {
        use serde_json::json;

        const SAMPLES: usize = 720_000;
        let temp = TempDir::new("prewarm-cap");
        let mut store = open_store(&temp);
        let wav = pcm_wav(SAMPLES);
        let mut files = Vec::new();
        let mut paths = Vec::new();
        let mut audio_files = Vec::new();
        let keys = [
            (EventId::MultiKill, Some(2)),
            (EventId::MultiKill, Some(3)),
            (EventId::MultiKill, Some(4)),
            (EventId::MultiKill, Some(5)),
            (EventId::HealthDanger, None),
        ];
        let bindings: Vec<_> = keys
            .iter()
            .map(|&(id, tier)| {
                let count = if id.is_critical() { 1 } else { 14 };
                let variants: Vec<_> = (0..count)
                    .map(|_| {
                        let index = files.len();
                        let path = format!("audio/clip-{index}.wav");
                        let mut variant_wav = wav.clone();
                        let sample_offset = 44 + index * 2;
                        variant_wav[sample_offset] = u8::try_from(index + 1).unwrap();
                        let digest = format!("{:x}", Sha256::digest(&variant_wav));
                        files.push(json!({
                            "path": path,
                            "size_bytes": variant_wav.len(),
                            "sha256": digest,
                            "role": "mixed_audio",
                        }));
                        paths.push(path.clone());
                        audio_files.push((path.clone(), variant_wav));
                        json!({
                            "variant_id": format!("clip-{index}"), "mixed_audio_path": path,
                            "duration_ms": 15_000, "visual_tail_ms": 0,
                        })
                    })
                    .collect();
                let mut binding = json!({"event_id": id.as_str(), "variants": variants});
                if let Some(tier) = tier {
                    binding["tier"] = json!(tier);
                }
                if count > 1 {
                    binding["selection"] = json!("sequence");
                }
                binding
            })
            .collect();
        assert!(paths.len() * SAMPLES > super::MAX_PREWARMED_SAMPLES as usize);
        let descriptor = serde_json::to_vec(&json!({
            "schema_version": "1.4.0", "package_id": "prewarm-cap-pack",
            "revision": 1, "taxonomy_version": "1.1", "display_name": "Prewarm cap fixture",
            "publisher_key_id": null, "files": files, "bindings": bindings,
        }))
        .unwrap();
        let archive = fixture_zip(&descriptor, &audio_files);
        let package = Package {
            package_path: temp.path().join("prewarm-cap.zip"),
            digest: format!("{:x}", Sha256::digest(&archive)),
            package_id: "prewarm-cap-pack".to_owned(),
        };
        fs::write(&package.package_path, archive).unwrap();
        let installed = install(&mut store, &package, "req-cap-install");
        let Some(record) = installed.record else {
            panic!(
                "fixture install failed: {:?} {:?}, reject: {:?}",
                installed.status, installed.reason, installed.reject,
            );
        };
        assert_eq!(
            activate(&mut store, "req-cap-active", &record.install_id)
                .status
                .as_str(),
            "active"
        );

        let mut table = None;
        let logs = crate::logging::collect_test_logs(|| {
            table = Some(build_table(&store, &catalog(), &mut || {}).0);
        });
        let table = table.unwrap();
        let mut retained = 0;
        for (id, tier) in keys {
            let BindingLookup::Variants { binding, .. } = table.lookup_binding(id, tier) else {
                panic!("the binding must remain playable: {id:?} {tier:?}");
            };
            let first = binding.variants[0]
                .media
                .as_ref()
                .expect("first variant must be prewarmed");
            assert_eq!(first.pcm.len(), SAMPLES);
            retained += binding
                .variants
                .iter()
                .filter_map(|variant| variant.media.as_ref().ok())
                .map(|clip| clip.pcm.len())
                .sum::<usize>();
        }
        assert!(retained <= super::MAX_PREWARMED_SAMPLES as usize);
        assert_eq!(retained, 55 * SAMPLES);
        let skipped: Vec<_> = logs
            .iter()
            .filter(|log| log.event == "clip_variant_skipped")
            .collect();
        assert_eq!(skipped.len(), 2);
        assert_eq!(skipped[0].fields["variant_id"], "clip-41");
        assert_eq!(skipped[1].fields["variant_id"], "clip-55");
        assert!(skipped.iter().all(|log| log.module == "audio"
            && log.fields["event_id"] == "multi_kill"
            && log.fields["reason"] == "prewarm_cap"));
        assert_eq!(skipped[0].fields["tier"], 4);
        assert_eq!(skipped[1].fields["tier"], 5);
    }

    /// Store a descriptor and distinct audio payloads in one fixture archive.
    fn fixture_zip(descriptor: &[u8], audio_files: &[(String, Vec<u8>)]) -> Vec<u8> {
        use flate2::{Compression, Crc, write::DeflateEncoder};
        use std::io::Write;

        let crc = |bytes: &[u8]| {
            let mut crc = Crc::new();
            crc.update(bytes);
            crc.sum()
        };
        let mut zip = Vec::new();
        let mut directory = Vec::new();
        let mut push_entry = |name: &str, raw_size: usize, bytes: &[u8], method: u16, crc: u32| {
            let offset = zip.len() as u32;
            let mut header = Vec::new();
            for word in [20_u16, 0, method, 0, 33] {
                header.extend(word.to_le_bytes());
            }
            for word in [crc, bytes.len() as u32, raw_size as u32] {
                header.extend(word.to_le_bytes());
            }
            header.extend((name.len() as u16).to_le_bytes());
            header.extend(0_u16.to_le_bytes());
            zip.extend(0x04034b50_u32.to_le_bytes());
            zip.extend(&header);
            zip.extend(name.as_bytes());
            zip.extend(bytes);
            directory.extend(0x02014b50_u32.to_le_bytes());
            directory.extend(0x0314_u16.to_le_bytes());
            directory.extend(header);
            directory.extend([0_u8; 6]);
            directory.extend((0o100644_u32 << 16).to_le_bytes());
            directory.extend(offset.to_le_bytes());
            directory.extend(name.as_bytes());
        };
        push_entry(
            "descriptor.json",
            descriptor.len(),
            descriptor,
            0,
            crc(descriptor),
        );
        // The container requires byte-sorted payload entries after the descriptor.
        let mut ordered_files: Vec<_> = audio_files.iter().collect();
        ordered_files.sort_unstable_by_key(|(name, _)| name.as_str());
        for (name, wav) in ordered_files {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(wav).unwrap();
            let compressed = encoder.finish().unwrap();
            push_entry(name, wav.len(), &compressed, 8, crc(wav));
        }
        let directory_start = zip.len() as u32;
        let count = (audio_files.len() + 1) as u16;
        zip.extend(&directory);
        zip.extend(0x06054b50_u32.to_le_bytes());
        for word in [0, 0, count, count] {
            zip.extend(word.to_le_bytes());
        }
        zip.extend((directory.len() as u32).to_le_bytes());
        zip.extend(directory_start.to_le_bytes());
        zip.extend(0_u16.to_le_bytes());
        zip
    }

    #[cfg(unix)]
    #[test]
    fn unrelated_revision_link_blocks_prewarm() {
        let temp = TempDir::new("prewarm-unrelated-link");
        let (store, install_id) = activated_store(
            &temp,
            "unrelated-link-pack",
            &[audio("health_danger", 480)],
            "req-link",
        );
        let revision = store.root().join("revisions").join(install_id);
        let outside = temp.path().join("outside");
        fs::write(&outside, b"unrelated").unwrap();
        std::os::unix::fs::symlink(outside, revision.join("unrelated")).unwrap();

        let (table, visuals) = build_table(&store, &catalog(), &mut || {});
        assert!(matches!(
            table.lookup(EventId::HealthDanger, None),
            Lookup::Invalid
        ));
        assert!(visuals.entries.is_empty());
    }

    #[test]
    fn key_space_count_matches_an_independent_catalog_calculation() {
        let catalog_json: Value =
            serde_json::from_str(include_str!("../../../../../contracts/event-catalog.json"))
                .expect("independent event catalog JSON parses");
        let events = catalog_json["events"]
            .as_array()
            .expect("catalog events exist");
        let voiced: Vec<_> = ALL_EVENTS
            .iter()
            .copied()
            .filter(|id| id.voiced())
            .collect();
        let mut expected = voiced.len();
        for id in &voiced {
            if let Some(event) = events.iter().find(|event| event["id"] == id.as_str()) {
                if let Some(range) = event.get("tier_range") {
                    let minimum = range["min"].as_u64().expect("tier minimum is numeric");
                    let maximum = range["max"].as_u64().expect("tier maximum is numeric");
                    expected += usize::try_from(maximum - minimum + 1)
                        .expect("catalog tier count fits usize");
                }
            }
        }
        assert_eq!(
            key_space(&catalog()).len(),
            expected,
            "keys follow the independent catalog count"
        );
        assert_eq!(expected, 26, "the independent count is 26 for this catalog");
    }

    #[test]
    fn key_space_contains_a_tierless_key_for_every_voiced_host_event() {
        let keys = key_space(&catalog());
        let actual: Vec<_> = keys
            .iter()
            .filter_map(|(id, tier)| tier.is_none().then_some(*id))
            .collect();
        let expected: Vec<_> = ALL_EVENTS
            .iter()
            .copied()
            .filter(|id| id.voiced())
            .collect();
        assert_eq!(
            actual, expected,
            "tierless keys preserve host inventory order"
        );
    }

    #[test]
    fn key_space_omits_unvoiced_hero_respawn() {
        assert!(
            !key_space(&catalog())
                .iter()
                .any(|(id, _)| *id == EventId::HeroRespawn),
            "hero_respawn has no prewarm key"
        );
    }

    #[test]
    fn key_space_derives_multikill_tiers_from_the_catalog() {
        let actual: Vec<_> = key_space(&catalog())
            .into_iter()
            .filter_map(|(id, tier)| (id == EventId::MultiKill).then_some(tier))
            .collect();
        assert_eq!(
            actual,
            vec![None, Some(2), Some(3), Some(4), Some(5)],
            "multi_kill range comes from event-catalog.json"
        );
    }

    #[test]
    fn key_space_derives_streak_tiers_from_the_catalog() {
        let actual: Vec<_> = key_space(&catalog())
            .into_iter()
            .filter_map(|(id, tier)| (id == EventId::Streak).then_some(tier))
            .collect();
        assert_eq!(
            actual,
            vec![
                None,
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                Some(9),
                Some(10)
            ],
            "streak range comes from event-catalog.json"
        );
    }

    #[test]
    fn key_space_has_no_duplicate_pairs() {
        let keys = key_space(&catalog());
        let unique: HashSet<_> = keys.iter().copied().collect();
        assert_eq!(
            unique.len(),
            keys.len(),
            "derived event and tier pairs are unique"
        );
    }

    #[test]
    fn cabinet_key_space_has_only_catalog_tiers_for_each_voiced_tile() {
        let keys = cabinet_key_space(&catalog());
        let unique: HashSet<_> = keys.iter().copied().collect();
        assert_eq!(
            keys.len(),
            24,
            "four multi-kill, eight streak and twelve untiered tiles"
        );
        assert_eq!(
            unique.len(),
            keys.len(),
            "catalog event/tier tiles are unique"
        );
        assert!(keys.iter().all(|(id, _)| id.voiced()));
        assert!(!keys.iter().any(|(id, _)| *id == EventId::HeroRespawn));
        assert_eq!(
            keys.iter()
                .filter_map(|(id, tier)| (*id == EventId::MultiKill).then_some(*tier))
                .collect::<Vec<_>>(),
            vec![Some(2), Some(3), Some(4), Some(5)]
        );
        assert_eq!(
            keys.iter()
                .filter_map(|(id, tier)| (*id == EventId::Streak).then_some(*tier))
                .collect::<Vec<_>>(),
            vec![
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                Some(9),
                Some(10)
            ]
        );
    }

    #[test]
    fn visual_index_handle_starts_without_install_or_entries() {
        let handle = new_visual_index_handle();
        let index = handle
            .lock()
            .expect("new visual index lock is available")
            .clone();
        assert!(
            index.install_id.is_none(),
            "new visual index has no active install"
        );
        assert!(
            index.entries.is_empty(),
            "new visual index has no visual files"
        );
    }

    #[test]
    fn no_active_store_returns_empty_table_and_visual_index() {
        let temp = TempDir::new("empty-table");
        let store = open_store(&temp);
        let (table, index) = build_table(&store, &catalog(), &mut || {});
        assert_eq!(table.fence, 0, "empty table fence is zero");
        assert!(
            table.install_id.is_none(),
            "empty table has no install identifier"
        );
        assert!(
            index.install_id.is_none(),
            "empty visual index has no install identifier"
        );
        assert!(index.entries.is_empty(), "empty visual index has no paths");
    }

    #[test]
    fn loader_pauses_for_each_catalog_key_and_decoded_variant() {
        let temp = TempDir::new("pause-count");
        let (store, _) = activated_store(
            &temp,
            "pause-count-pack",
            &[audio("health_danger", 100)],
            "req-pause",
        );
        let mut calls = 0;
        let _ = build_table(&store, &catalog(), &mut || calls += 1);
        assert_eq!(
            calls, 28,
            "pause callback runs once for 26 keys, the pack, and decoded variant"
        );
    }

    #[test]
    fn audio_only_revision_prewarms_multiple_events_with_install_fence() {
        let temp = TempDir::new("audio-table");
        let (store, install_id) = activated_store(
            &temp,
            "audio-table-pack",
            &[audio("health_danger", 481), audio("hero_kill", 960)],
            "req-audio",
        );
        let (table, index) = build_table(&store, &catalog(), &mut || {});
        assert_eq!(
            table.fence, 1,
            "first generated install number is the table fence"
        );
        assert_eq!(
            table.install_id.as_deref(),
            Some(install_id.as_str()),
            "table uses the active install id"
        );
        assert!(
            index.entries.is_empty(),
            "audio-only revision has no visual entries"
        );
        for (id, samples) in [
            (EventId::HealthDanger, 481_usize),
            (EventId::HeroKill, 960_usize),
        ] {
            match table.lookup(id, None) {
                Lookup::Ready(clip) => {
                    assert_eq!(
                        clip.pcm.len(),
                        samples,
                        "decoded PCM preserves generated sample count"
                    );
                    assert!(
                        clip.visual_url.is_none(),
                        "audio-only clip has no visual URL"
                    );
                }
                Lookup::Invalid | Lookup::Unmapped => {
                    panic!("generated voiced binding should be ready")
                }
            }
        }
    }

    #[test]
    fn schema_1_3_binding_stays_a_single_v1_variant_and_repeats_unchanged() {
        let temp = TempDir::new("legacy-single-variant");
        let (store, _) = activated_store(
            &temp,
            "legacy-single-variant-pack",
            &[audio("hero_kill", 960)],
            "req-legacy",
        );
        let resolved = store
            .resolve_variants("hero_kill", None)
            .expect("legacy binding resolves through the V1 variants API");
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].variant_id, "v1");
        assert_eq!(resolved[0].media.as_ref().unwrap().selection, None);

        let (table, _) = build_table(&store, &catalog(), &mut || {});
        let mut selector = VariantSelector::with_seed(0x130);
        for _ in 0..4 {
            let selected = selector
                .select(&table, EventId::HeroKill, None, 1)
                .expect("the legacy clip remains ready");
            assert_eq!(selected.variant_id, "v1");
            assert_eq!(selected.media.pcm.len(), 960);
            assert!(selected.media.pcm.iter().all(|sample| *sample == 0));
            assert_eq!(selected.selection, None);
            selector.record_played(&selected);
        }
    }

    #[test]
    fn audio_duration_uses_ceiling_sample_conversion() {
        let temp = TempDir::new("duration-ceil");
        let (store, _) = activated_store(
            &temp,
            "duration-ceil-pack",
            &[audio("health_danger", 49)],
            "req-duration",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        match table.lookup(EventId::HealthDanger, None) {
            Lookup::Ready(clip) => {
                assert_eq!(clip.audio_ms, 2, "49 samples round up to two milliseconds")
            }
            Lookup::Invalid | Lookup::Unmapped => {
                panic!("duration fixture binding should be ready")
            }
        }
    }

    #[test]
    fn absent_binding_remains_unmapped_in_the_table() {
        let temp = TempDir::new("unmapped");
        let (store, _) = activated_store(
            &temp,
            "unmapped-pack",
            &[audio("hero_kill", 480)],
            "req-unmapped",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        assert!(
            matches!(table.lookup(EventId::HealthDanger, None), Lookup::Unmapped),
            "unbound event has no placeholder entry"
        );
    }

    #[test]
    fn corrupt_audio_after_activation_becomes_invalid_and_blocks_readiness() {
        let temp = TempDir::new("corrupt-wav");
        let (store, install_id) = activated_store(
            &temp,
            "corrupt-wav-pack",
            &[audio("health_danger", 480)],
            "req-corrupt",
        );
        let revision = temp
            .path()
            .join("store")
            .join("revisions")
            .join(&install_id);
        let descriptor: Value = serde_json::from_slice(
            &fs::read(revision.join("descriptor.json")).expect("installed descriptor is readable"),
        )
        .expect("installed descriptor is valid JSON");
        assert_eq!(descriptor["schema_version"], "1.3.0");
        let relative = descriptor["bindings"][0]["mixed_audio_path"]
            .as_str()
            .expect("binding has mixed audio path");
        let path = revision.join(relative);
        let mut bytes = fs::read(&path).expect("verified WAV is readable");
        *bytes
            .get_mut(44)
            .expect("WAV sample follows its 44-byte header") ^= 0xff;
        fs::write(path, bytes).expect("flip one installed audio byte");
        assert_eq!(
            store.resolve("health_danger", None),
            Err(ResolveError::Invalid(RejectReason::DigestMismatch))
        );
        let Err(ResolveError::NoVerifiedVariants(variants)) =
            store.resolve_variants("health_danger", None)
        else {
            panic!("the corrupted legacy binding should have no verified variant");
        };
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].variant_id, "v1");
        assert_eq!(variants[0].media, Err(RejectReason::DigestMismatch));
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        assert!(
            matches!(table.lookup(EventId::HealthDanger, None), Lookup::Invalid),
            "digest recheck rejects modified installed WAV"
        );
        let mut selector = VariantSelector::with_seed(1);
        assert!(matches!(
            selector.select(&table, EventId::HealthDanger, None, 1),
            Err(crate::media::selection::SelectionFailure::InvalidMedia)
        ));
        let facts = facts_from_table(&table, &|_| Some(true), true);
        let shared = EngineShared::new();
        shared.heartbeat_ms.store(10, Ordering::Relaxed);
        shared.device_available.store(true, Ordering::Relaxed);
        assert_eq!(
            media_status(&facts, &shared, 10),
            ("not_ready", Some("MEDIA_INVALID")),
            "healthy engine reports invalid critical audio"
        );
    }

    #[test]
    fn audio_only_binding_has_no_visual_index_row_or_url() {
        let temp = TempDir::new("audio-only-index");
        let (store, _) = activated_store(
            &temp,
            "audio-only-index-pack",
            &[audio("health_danger", 480)],
            "req-audioonly",
        );
        let (table, index) = build_table(&store, &catalog(), &mut || {});
        assert!(
            index.entries.is_empty(),
            "audio-only revision has no visual index rows"
        );
        match table.lookup(EventId::HealthDanger, None) {
            Lookup::Ready(clip) => assert!(
                clip.visual_url.is_none(),
                "audio-only clip does not create a logical URL"
            ),
            Lookup::Invalid | Lookup::Unmapped => panic!("audio-only fixture should be ready"),
        }
    }

    #[test]
    fn tiered_rows_support_highest_bound_tier_lookup() {
        let temp = TempDir::new("tier-fallback");
        let bindings = [
            BindingSpec {
                event_id: "multi_kill",
                tier: Some(2),
                samples: 480,
                visual: false,
                visual_tail_ms: 0,
            },
            BindingSpec {
                event_id: "multi_kill",
                tier: Some(3),
                samples: 960,
                visual: false,
                visual_tail_ms: 0,
            },
        ];
        let (store, _) = activated_store(&temp, "tier-fallback-pack", &bindings, "req-tier");
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        match table.lookup(EventId::MultiKill, Some(4)) {
            Lookup::Ready(clip) => {
                assert_eq!(
                    clip.bound_tier,
                    Some(3),
                    "tier four falls back to the highest lower bound"
                );
                assert_eq!(
                    clip.pcm.len(),
                    960,
                    "fallback uses the tier three clip samples"
                );
            }
            Lookup::Invalid | Lookup::Unmapped => {
                panic!("tier three binding should serve tier four")
            }
        }
    }

    #[test]
    fn merged_loadout_obeys_tier_lock_critical_source_and_loadout_order() {
        let temp = TempDir::new("multi-pack-merge");
        let mut store = open_store(&temp);
        let pack_a = package(
            &temp,
            "voice.main",
            &[
                BindingSpec {
                    event_id: "multi_kill",
                    tier: Some(2),
                    samples: 480,
                    visual: false,
                    visual_tail_ms: 0,
                },
                BindingSpec {
                    event_id: "multi_kill",
                    tier: Some(3),
                    samples: 960,
                    visual: false,
                    visual_tail_ms: 0,
                },
                audio("health_danger", 480),
                audio("hero_kill", 480),
                audio("hero_death", 480),
            ],
        );
        let pack_b = package(
            &temp,
            "voice.special",
            &[
                BindingSpec {
                    event_id: "multi_kill",
                    tier: Some(2),
                    samples: 720,
                    visual: false,
                    visual_tail_ms: 0,
                },
                audio("health_danger", 720),
                audio("hero_death", 720),
            ],
        );
        let a = install(&mut store, &pack_a, "multi-a-install")
            .record
            .unwrap()
            .install_id;
        let b = install(&mut store, &pack_b, "multi-b-install")
            .record
            .unwrap()
            .install_id;
        let locks = [
            ("hero_death".to_owned(), "voice.special".to_owned()),
            ("hero_kill".to_owned(), "voice.special".to_owned()),
        ]
        .into_iter()
        .collect();
        let result = store
            .set_loadout(
                "multi-loadout",
                &LoadoutDesired {
                    install_ids: vec![a.clone(), b.clone()],
                    event_locks: locks,
                    match_consistency: false,
                },
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        assert_eq!(result.status.as_str(), "active");
        let (table, _) = build_table(&store, &catalog(), &mut || {});

        let BindingLookup::Variants {
            binding: tier_two, ..
        } = table.lookup_binding(EventId::MultiKill, Some(2))
        else {
            panic!("tier 2 pool exists")
        };
        assert_eq!(
            tier_two
                .variants
                .iter()
                .filter_map(|variant| variant.source.as_ref().map(|source| source.slot))
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        let BindingLookup::Variants {
            binding: tier_three,
            ..
        } = table.lookup_binding(EventId::MultiKill, Some(3))
        else {
            panic!("tier 3 resolves")
        };
        assert_eq!(
            tier_three.variants.len(),
            1,
            "tier 3 excludes another pack's tier-2 line"
        );
        assert_eq!(tier_three.variants[0].source.as_ref().unwrap().slot, 0);
        let BindingLookup::Variants {
            binding: critical, ..
        } = table.lookup_binding(EventId::HealthDanger, None)
        else {
            panic!("critical cue is ready")
        };
        assert_eq!(critical.variants.len(), 1, "critical cue has one source");
        assert_eq!(critical.variants[0].source.as_ref().unwrap().slot, 0);
        let BindingLookup::Variants {
            binding: locked, ..
        } = table.lookup_binding(EventId::HeroDeath, None)
        else {
            panic!("satisfied lock is ready")
        };
        assert_eq!(locked.variants.len(), 1);
        assert_eq!(
            locked.variants[0]
                .source
                .as_ref()
                .unwrap()
                .package_id
                .as_ref(),
            "voice.special"
        );
        let BindingLookup::Variants {
            binding: fallback, ..
        } = table.lookup_binding(EventId::HeroKill, None)
        else {
            panic!("unsatisfied lock falls back to the pool")
        };
        assert_eq!(fallback.variants.len(), 1);
        assert_eq!(
            fallback.variants[0]
                .source
                .as_ref()
                .unwrap()
                .package_id
                .as_ref(),
            "voice.main"
        );

        let reversed = store
            .set_loadout(
                "multi-reorder",
                &LoadoutDesired {
                    install_ids: vec![b, a],
                    event_locks: Default::default(),
                    match_consistency: false,
                },
                ActivationPolicy::NowIfIdle,
                &ActivationState::default(),
            )
            .unwrap();
        assert_eq!(reversed.status.as_str(), "active");
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        let BindingLookup::Variants {
            binding: critical, ..
        } = table.lookup_binding(EventId::HealthDanger, None)
        else {
            panic!("critical cue is ready after reorder")
        };
        assert_eq!(critical.variants[0].source.as_ref().unwrap().slot, 0);
        assert_eq!(
            critical.variants[0]
                .source
                .as_ref()
                .unwrap()
                .package_id
                .as_ref(),
            "voice.special"
        );
    }

    #[test]
    fn malformed_install_identifier_parses_defensively_as_zero() {
        for malformed in ["bad-id", "ins_000000000x", "ins_12345678901"] {
            assert_eq!(
                super::install_number(malformed),
                0,
                "malformed install id {malformed} has fence zero"
            );
        }
    }

    #[test]
    fn valid_install_identifier_parses_its_decimal_install_number() {
        assert_eq!(
            super::install_number("ins_0000000007"),
            7,
            "ten decimal suffix digits become fence seven"
        );
    }

    #[test]
    fn resolving_without_an_active_revision_is_unmapped() {
        let temp = TempDir::new("unmapped-resolve");
        let store = open_store(&temp);
        assert_eq!(
            store.resolve("health_danger", None),
            Err(ResolveError::Unmapped),
            "store leaves absent active revision unmapped"
        );
    }

    #[test]
    fn audio_only_visual_index_still_names_active_revision() {
        let temp = TempDir::new("visual-empty-revision");
        let (store, install_id) = activated_store(
            &temp,
            "visual-empty-revision-pack",
            &[audio("health_danger", 480)],
            "req-visual-empty",
        );
        let (_, index) = build_table(&store, &catalog(), &mut || {});
        assert_eq!(
            index.install_id.as_deref(),
            Some(install_id.as_str()),
            "empty visual index remains revision scoped"
        );
    }

    #[test]
    fn audio_only_critical_readiness_skips_the_visual_probe() {
        let temp = TempDir::new("critical-facts");
        let (store, _) = activated_store(
            &temp,
            "critical-facts-pack",
            &[audio("health_danger", 480)],
            "req-critical",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        let facts = facts_from_table(&table, &|_| Some(false), true);
        assert_eq!(
            facts.critical.first().map(|fact| fact.probe_ok),
            Some(Some(true)),
            "audio-only critical media is probe-safe"
        );
    }

    #[test]
    fn active_prewarmer_facts_include_expected_health_danger_record() {
        let temp = TempDir::new("critical-record");
        let (store, _) = activated_store(
            &temp,
            "critical-record-pack",
            &[audio("health_danger", 480)],
            "req-critical-record",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        let facts = facts_from_table(&table, &|_| None, true);
        let expected = CriticalMedia {
            event_id: "health_danger".to_owned(),
            mapped: true,
            verified: true,
            prewarmed: true,
            probe_ok: Some(true),
        };
        assert_eq!(
            facts.critical,
            vec![expected],
            "host critical inventory maps to the prewarmed audio clip"
        );
    }

    #[test]
    fn configured_catalog_order_is_reflected_in_every_prepared_entry() {
        let temp = TempDir::new("key-order");
        let (store, _) = activated_store(
            &temp,
            "key-order-pack",
            &[audio("health_danger", 480)],
            "req-key-order",
        );
        let (table, _) = build_table(&store, &catalog(), &mut || {});
        assert!(
            matches!(table.lookup(EventId::HealthDanger, None), Lookup::Ready(_)),
            "mapped key survives complete catalog traversal"
        );
        assert_eq!(
            key_space(&catalog()).first().copied(),
            Some((EventId::MultiKill, None)),
            "prewarm order starts with host inventory declaration order"
        );
    }
}
