import { useCallback, useEffect, useId, useRef, useState } from "react";
import { AudioMuteControl, type AudioMuteState } from "../AudioPanel";
import { auditionReason, copy, packsPageCopy, type Locale, type Phase } from "../copy";
import { errorCode, newRequestId, sortRecords } from "../packs";
import { toWebviewUrl } from "../mediaUrl";
import { matchRecapCommand, mediaAudition, mediaEventMap, subscribeAuditionMatch } from "../runtime";
import { useMediaListing } from "../useMediaListing";
import { VolumeControl } from "../VolumeControl";
import {
  auditionSound, beginEventMapRefresh, eventMapMatchesPack, eventMapPhaseBlocksAudition,
  eventMapPlayBlock, soundKey, type EventMapLoadResult, type SoundKey,
} from "./eventMap";
import { MatchHistory } from "./MatchHistory";
import { clearMatchRecap, loadMatchRecap, type MatchRecapSnapshot } from "./recapModel";
import { nextPacksTab, PACKS_TABS, type PacksTab, type PackSummary } from "./model";
import { PackDrawer, PackCard } from "./PackLibrary";
import { InstallDialog } from "./InstallDialog";
import { installDialogCopy } from "./InstallDialog.copy";
import { browseApi, type BrowseBinding, type BrowsePackDetail } from "./browseApi";
import { countPackOrigins, filterBrowsePacks, originFiltersAvailable, type BrowseFilters } from "./browseModel";
import { createBrowsePreviewState, holdBrowseVideo, playBrowseVideo, prepareBrowseVideo, reduceBrowsePreview } from "./previewModel";
import { StorePage } from "../store/StorePage";
import { CoverageMatrix } from "./CoverageMatrix";
import { cancelPackLoadout, getPackLoadout, setPackLoadout } from "./loadoutApi";
import { loadoutRequest, moveEquipped, toggleEquipped, type PackLoadoutState } from "./loadoutModel";

export function PacksPage({ locale, disabled, phase, mute, storeOnly = false, onStoreBack, onStoreOpen, developerMode = false }: {
  locale: Locale; disabled: boolean; phase: Phase | null; mute: AudioMuteState; storeOnly?: boolean;
  onStoreBack?: () => void; onStoreOpen?: () => void; developerMode?: boolean;
}) {
  const text = packsPageCopy(locale);
  const installText = installDialogCopy[locale];
  const id = useId();
  const tabs = useRef<Partial<Record<PacksTab, HTMLButtonElement>>>({});
  const [tab, setTab] = useState<PacksTab>(PACKS_TABS[0]);
  const [recap, setRecap] = useState<MatchRecapSnapshot | null>(null);
  const [clearingRecap, setClearingRecap] = useState(false);
  const [open, setOpen] = useState<{ id: string; from: HTMLButtonElement } | null>(null);
  const [refreshId, setRefreshId] = useState(0);
  const [summaries, setSummaries] = useState<PackSummary[] | null>(null);
  const [summariesFailed, setSummariesFailed] = useState(false);
  const [packLoadout, setPackLoadoutState] = useState<PackLoadoutState | null>(null);
  const [mapLoad, setMapLoad] = useState<EventMapLoadResult | null>(null);
  const [matchSignal, setMatchSignal] = useState<{ active: boolean | null } | null>(null);
  const [action, setAction] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [auditionNotice, setAuditionNotice] = useState<string | null>(null);
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
  const [officialFilter, setOfficialFilter] = useState(true);
  const [customFilter, setCustomFilter] = useState(true);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<BrowseFilters["sort"]>("active-first");
  const [packDetails, setPackDetails] = useState<Record<string, BrowsePackDetail | null>>({});
  const [detailLoadingId, setDetailLoadingId] = useState<string | null>(null);
  const [preview, setPreview] = useState(createBrowsePreviewState);
  const [previewNotice, setPreviewNotice] = useState<string | null>(null);
  const [overlayPreview, setOverlayPreview] = useState(false);
  const previewVideo = useRef<HTMLVideoElement>(null);
  const previewSubscription = useRef<Promise<boolean> | null>(null);
  const pendingPreview = useRef<{
    installId: string; variantId: string; soundKey: string; hasVisual: boolean; visualUrl: string | null;
  } | null>(null);
  const previewRef = useRef(preview);
  previewRef.current = preview;
  const openIdRef = useRef<string | null>(open?.id ?? null);
  openIdRef.current = open?.id ?? null;
  const commitPreview = useCallback((event: Parameters<typeof reduceBrowsePreview>[1]) => {
    const next = reduceBrowsePreview(previewRef.current, event);
    previewRef.current = next;
    setPreview(next);
    return next;
  }, []);
  const actionLock = useRef(false);
  const auditionLock = useRef(false);
  const listing = useMediaListing();
  const listingCurrent = !disabled && listing.state === "ok";
  const inventoryKey = listing.listing?.records.map(record => record.install_id).join(",") ?? null;
  const active = listing.listing?.active;
  const activeId = active?.install_id;
  const phaseLock = eventMapPhaseBlocksAudition(phase);
  const currentRecap = disabled ? { matches: [], skipped: 0, readable: false } : recap;

  useEffect(() => {
    if (tab !== "recap" || disabled || clearingRecap) return;
    let current = true;
    void loadMatchRecap(matchRecapCommand).then(snapshot => {
      if (current) setRecap(snapshot);
    });
    return () => { current = false; };
  }, [tab, disabled, clearingRecap]);

  const clearRecap = async () => {
    setClearingRecap(true);
    try {
      await clearMatchRecap(matchRecapCommand);
      setRecap(null);
    } finally {
      setClearingRecap(false);
    }
  };

  useEffect(() => {
    if (!listingCurrent || inventoryKey === null) return;
    let current = true;
    setSummariesFailed(false);
    void browseApi.summaries().then(result => {
      if (!current) return;
      setSummaries(result);
      setSummariesFailed(result === null);
    }, () => {
      if (current) { setSummaries(null); setSummariesFailed(true); }
    });
    return () => { current = false; };
  }, [inventoryKey, listingCurrent, refreshId]);

  useEffect(() => {
    if (!listingCurrent || inventoryKey === null) { setPackLoadoutState(null); return; }
    let current = true;
    void getPackLoadout().then(value => { if (current) setPackLoadoutState(value); }, () => { if (current) setPackLoadoutState(null); });
    return () => { current = false; };
  }, [inventoryKey, listingCurrent, refreshId]);

  useEffect(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    const subscription = browseApi.subscribe(event => {
      if (event.installId !== openIdRef.current) return;
      if ("startedMonotonicMs" in event) {
        const pending = pendingPreview.current;
        const key = soundKey({ event_id: event.eventId, tier: event.tier });
        if (!pending || pending.installId !== event.installId || pending.variantId !== event.variantId
          || pending.soundKey !== key) return;
        pendingPreview.current = null;
        commitPreview({ type: "started", playbackId: event.playbackId, installId: event.installId, variantId: event.variantId,
          durationMs: event.durationMs,
          soundKey: key, hasVisual: pending.hasVisual, visualUrl: pending.visualUrl });
        if (pending.visualUrl && previewVideo.current) {
          void playBrowseVideo(previewVideo.current).catch(() => {
            if (previewRef.current.playing) {
              commitPreview({ type: "visual-failed", playbackId: event.playbackId, reason: "VIDEO_START_FAILED" });
            }
          });
        }
        setPreviewNotice(text.listening);
        return;
      }
      const previous = previewRef.current;
      const next = commitPreview({ type: "ended", playbackId: event.playbackId, reason: event.reason });
      if (previous === next || previous.installId !== event.installId) return;
      if (previewVideo.current && previous.visualUrl) holdBrowseVideo(previewVideo.current, event.reason === "finished");
      setPreviewNotice(event.reason === "finished" ? text.listenAudio.finished
        : event.reason === "cut" ? text.listenAudio.cut : auditionReason(event.reason.toUpperCase(), locale));
    });
    const readiness = subscription.then(unsubscribe => {
      if (disposed) unsubscribe();
      else cleanup = unsubscribe;
      return !disposed;
    }, () => false);
    previewSubscription.current = readiness;
    return () => {
      disposed = true;
      if (previewSubscription.current === readiness) previewSubscription.current = null;
      cleanup?.();
    };
  }, [commitPreview, locale, text.listening, text.listenAudio.cut, text.listenAudio.finished]);

  useEffect(() => {
    let current = true;
    const subscription = subscribeAuditionMatch(active => { if (current) setMatchSignal({ active }); });
    void subscription.then(
      () => { if (current) setMatchSignal(signal => signal ?? { active: null }); },
      () => { if (current) setMatchSignal(signal => signal ?? { active: null }); },
    );
    return () => {
      current = false;
      void subscription.then(unlisten => unlisten(), () => {});
    };
  }, []);

  useEffect(() => {
    if (disabled || matchSignal === null || tab !== "events") return;
    setMapLoad(null);
    return beginEventMapRefresh(mediaEventMap, setMapLoad);
  }, [activeId, phaseLock, matchSignal, refreshId, disabled, tab]);

  const summariesById = new Map(summaries?.map(pack => [pack.installId, pack]));
  const cards = (listing.listing ? sortRecords(listing.listing.records) : []).map(record => {
    const summary = summariesById.get(record.install_id);
    const verified = summary?.packageId === record.package_id && summary.revision === record.revision;
    return {
      ...(verified ? summary : {
        installId: record.install_id, packageId: record.package_id, revision: record.revision,
        equippedSlot: null, packKind: "unknown" as const,
        name: null, problem: "metadata" as const, coverUrl: null, counts: null, bindings: [],
        authorityClass: "unknown" as const, publisherClaim: null, installedAt: null, sizeBytes: null, gameId: null,
        presentation: { description: null, author: null, languages: null, coverUrl: null },
      }),
      active: activeId === record.install_id,
      pending: listing.listing?.pending.includes(record.install_id) === true,
    };
  });
  const originCounts = countPackOrigins(cards);
  const sourceFiltersEnabled = originFiltersAvailable(cards);
  const visibleCards = filterBrowsePacks(cards, {
    official: officialFilter, custom: customFilter, query: search, sort,
  });
  const activeCard = cards.find(pack => pack.active);
  const detail = open && cards.find(pack => pack.installId === open.id);
  const cachedDetail = open ? packDetails[open.id] : null;
  const detailData = listingCurrent && cachedDetail && detail
    && cachedDetail.packageId === detail.packageId && cachedDetail.revision === detail.revision ? cachedDetail : null;
  const detailLoading = open !== null && detailLoadingId === open.id;

  useEffect(() => {
    if (!open || !detail || !listingCurrent) {
      setDetailLoadingId(null);
      return;
    }
    let current = true;
    setDetailLoadingId(open.id);
    void browseApi.detail(detail).then(result => {
      if (!current) return;
      setPackDetails(previous => ({ ...previous, [open.id]: result }));
      setDetailLoadingId(null);
    }, () => {
      if (!current) return;
      setPackDetails(previous => ({ ...previous, [open.id]: null }));
      setDetailLoadingId(null);
    });
    return () => { current = false; };
  }, [open?.id, detail?.installId, detail?.packageId, detail?.revision, detail?.active, refreshId, listingCurrent, matchSignal]);

  useEffect(() => {
    commitPreview({ type: "reset" });
    pendingPreview.current = null;
    previewVideo.current?.pause();
    setPreviewNotice(null);
    setOverlayPreview(false);
  }, [commitPreview, open?.id]);
  const loadedMap = mapLoad?.state === "ok" ? mapLoad.eventMap : null;
  const matchesActive = loadedMap !== null && eventMapMatchesPack(loadedMap, listingCurrent ? active : undefined);
  const currentMap = matchesActive && !disabled ? loadedMap : null;
  const locked = phaseLock || matchSignal?.active === true || currentMap?.match_active === true;
  const browseMatchLocked = locked || detailData?.matchActive === true;
  const currentBrowseBlock = useRef({ locked: browseMatchLocked, muted: mute.muted === true, disabled: !listingCurrent });
  currentBrowseBlock.current = { locked: browseMatchLocked, muted: mute.muted === true, disabled: !listingCurrent };
  const mappedSounds = new Map(currentMap?.bindings.map(row => [soundKey(row), row]));

  const refresh = () => {
    void listing.refresh();
    setRefreshId(value => value + 1);
  };

  const block = (row: SoundKey): string | null => {
    if (disabled) return "CAPABILITY_UNAVAILABLE";
    if (locked) return "AUDITION_MATCH_ACTIVE";
    if (mute.muted === true) return "AUDITION_MUTED";
    if (action || auditionLock.current) return "AUDITION_BUSY";
    if (!currentMap) return "AUDITION_MEDIA_STATE_UNKNOWN";
    const sound = mappedSounds.get(soundKey(row));
    return sound ? eventMapPlayBlock(currentMap, sound, false, busyKey !== null) : "AUDITION_UNMAPPED";
  };

  const play = async (row: SoundKey) => {
    const blocked = block(row);
    if (blocked || actionLock.current) return;
    auditionLock.current = true;
    setBusyKey(soundKey(row));
    setAuditionNotice(text.listening);
    try {
      const result = await auditionSound(mediaAudition, row, blocked);
      if (!result) return;
      const audio = !result.audio_started ? "unknown"
        : !result.audio_ended ? "started" : result.audio_end_reason === "finished" ? "finished" : "cut";
      const visual = text.listenVisual[result.visual_status] ?? text.listenVisual.unknown;
      setAuditionNotice(`${text.listenAudio[audio]} · ${visual}`);
    } catch (failure) {
      setAuditionNotice(auditionReason(errorCode(failure), locale));
    } finally {
      auditionLock.current = false;
      setBusyKey(null);
    }
  };

  const updateLoadout = async (installIds: string[], eventLocks?: Record<string, string>, matchConsistency?: boolean) => {
    const current = packLoadout?.loadout;
    if (!listingCurrent || !current || actionLock.current || auditionLock.current || locked) return;
    actionLock.current = true;
    setAction("loadout");
    setActionNotice(null);
    setAuditionNotice(null);
    try {
      const result = await setPackLoadout(loadoutRequest(newRequestId(), installIds, current,
        eventLocks ?? current.event_locks, matchConsistency ?? current.match_consistency));
      const payload = typeof result === "object" && result !== null ? result as Record<string, unknown> : null;
      const status = payload?.status;
      setActionNotice(status === "active" ? (locale === "th" ? "ใช้ชุดแพ็กเสียงแล้ว" : "Loadout applied")
        : status === "deferred" ? (locale === "th" ? "บันทึกแล้ว จะใช้หลังจบแมตช์" : "Saved; applies after the match")
        : status === "rejected" ? `${locale === "th" ? "ใช้ชุดแพ็กไม่ได้" : "Loadout rejected"} · ${String(payload?.reason ?? "UNKNOWN")}`
        : (locale === "th" ? "รับคำขอชุดแพ็กเสียงแล้ว" : "Loadout request accepted"));
    } catch (failure) {
      const message = `${locale === "th" ? "เปลี่ยนชุดแพ็กไม่สำเร็จ" : "Could not change the loadout"} · ${errorCode(failure)}`;
      setActionNotice(message);
    } finally {
      setMapLoad(null);
      await listing.refresh();
      setRefreshId(value => value + 1);
      void getPackLoadout().then(value => setPackLoadoutState(value));
      setAction(null);
      actionLock.current = false;
    }
  };

  const togglePack = (pack: PackSummary) => {
    if (!packLoadout) return;
    const next = toggleEquipped(packLoadout.loadout, { installId: pack.installId, packageId: pack.packageId });
    if (!next) {
      setActionNotice(locale === "th" ? "ติดตั้งได้ไม่เกิน 4 แพ็ก และห้ามซ้ำแพ็กเดียวกัน" : "Equip up to four packs; only one revision per package is allowed");
      return;
    }
    void updateLoadout(next);
  };
  const movePack = (installId: string, delta: -1 | 1) => {
    if (!packLoadout) return;
    const next = moveEquipped(packLoadout.loadout, installId, delta);
    if (next) void updateLoadout(next);
  };
  const setEventLock = (row: import("./eventMap").EventSound, packageId: string | null) => {
    if (!packLoadout) return;
    const locks = { ...packLoadout.loadout.event_locks };
    if (packageId === null) delete locks[row.event_id]; else locks[row.event_id] = packageId;
    void updateLoadout(packLoadout.loadout.equipped.map(pack => pack.install_id), locks);
  };
  const cancelPendingLoadout = async () => {
    if (!packLoadout?.pending || actionLock.current) return;
    actionLock.current = true;
    setAction("loadout");
    try {
      await cancelPackLoadout();
      setActionNotice(locale === "th" ? "ยกเลิกชุดแพ็กที่รออยู่แล้ว" : "Pending loadout cancelled");
      setPackLoadoutState(await getPackLoadout());
      await listing.refresh();
      setRefreshId(value => value + 1);
    } catch (failure) {
      setActionNotice(`${locale === "th" ? "ยกเลิกไม่สำเร็จ" : "Could not cancel"} · ${errorCode(failure)}`);
    } finally {
      setAction(null);
      actionLock.current = false;
    }
  };
  const auditionBrowseVariant = async (binding: BrowseBinding, variantId: string) => {
    if (!detail || !detailData || disabled) throw new Error("AUDITION_MEDIA_STATE_UNKNOWN");
    if (browseMatchLocked) throw new Error("AUDITION_MATCH_ACTIVE");
    if (mute.muted === true) throw new Error("AUDITION_MUTED");
    if (actionLock.current || auditionLock.current) throw new Error("AUDITION_BUSY");
    const variant = binding.variants.find(candidate => candidate.variant_id === variantId);
    if (!variant) throw new Error("AUDITION_REQUEST_INVALID");
    auditionLock.current = true;
    setBusyKey(soundKey(binding));
    commitPreview({ type: "reset" });
    pendingPreview.current = null;
    setPreviewNotice(text.previewLoading);
    try {
      const video = previewVideo.current;
      let visualUrl: string | null = null;
      if (variant.has_visual && video) {
        const logical = await browseApi.visualUrl(detail.installId, binding.event_id, binding.tier, variantId).catch(() => null);
        const platform = navigator.userAgent.includes("Windows") ? "windows" : "other";
        const url = logical && toWebviewUrl(logical, platform);
        if (url && video === previewVideo.current && openIdRef.current === detail.installId
          && await prepareBrowseVideo(video, url)) visualUrl = url;
      }
      if (!await previewSubscription.current) throw new Error("CAPABILITY_UNAVAILABLE");
      if (openIdRef.current !== detail.installId || video !== previewVideo.current || currentBrowseBlock.current.disabled) {
        throw new Error("AUDITION_MEDIA_STATE_UNKNOWN");
      }
      if (currentBrowseBlock.current.locked) throw new Error("AUDITION_MATCH_ACTIVE");
      if (currentBrowseBlock.current.muted) throw new Error("AUDITION_MUTED");
      pendingPreview.current = { installId: detail.installId, variantId, soundKey: soundKey(binding),
        hasVisual: variant.has_visual, visualUrl };
      const result = await browseApi.audition({
        pack: detail, eventId: binding.event_id, tier: binding.tier, variantId,
        overlay: detail.active && overlayPreview,
      });
      const audio = !result.audio_started ? "unknown"
        : !result.audio_ended ? "started" : result.audio_end_reason === "finished" ? "finished" : "cut";
      if (openIdRef.current === detail.installId) setPreviewNotice(text.listenAudio[audio]);
      if (!result.audio_started) pendingPreview.current = null;
      return result;
    } catch (failure) {
      pendingPreview.current = null;
      if (openIdRef.current === detail.installId) setPreviewNotice(auditionReason(errorCode(failure), locale));
      throw failure;
    } finally {
      auditionLock.current = false;
      setBusyKey(null);
    }
  };
  const playCategory = async (bindings: readonly BrowseBinding[]) => {
    const selectedId = open?.id;
    for (const binding of bindings) {
      for (const variant of binding.variants) {
        if (!selectedId || openIdRef.current !== selectedId || currentBrowseBlock.current.locked
          || currentBrowseBlock.current.muted || currentBrowseBlock.current.disabled) return;
        try { await auditionBrowseVariant(binding, variant.variant_id); }
        catch { return; }
      }
    }
  };
  const reportPreviewVisualError = useCallback((playbackId: string, reason: string) => {
    commitPreview({ type: "visual-failed", playbackId, reason });
  }, [commitPreview]);
  const mapError = !!mapLoad && !currentMap;
  const changeTab = (name: PacksTab) => {
    setAuditionNotice(null);
    setTab(name);
  };

  return <section className="shell-screen packs-page" aria-labelledby="shell-page-heading">
    {storeOnly ? (
      developerMode ? (
        <StorePage locale={locale} disabled={disabled} phase={phase} onBackToPacks={onStoreBack} />
      ) : (
        <div className="packs-scroll packs-store-screen">
          <div className="packs-store-empty">
            <h2>{text.storeHeading}</h2>
            <p>{text.storeBody}</p>
            <div className="packs-store-actions">
              <button type="button" onClick={onStoreBack}>{text.storeBack}</button>
              <button type="button" className="primary" disabled={disabled || locked}
                onClick={() => setInstallDialogOpen(true)}>{text.storeInstall}</button>
            </div>
          </div>
        </div>
      )
    ) : <>
      <div className="packs-audio-bar">
        <div className="packs-audio-pack">
          <p className="muted">{text.intro}</p>
          <p>{text.activePack} <strong dir="auto">{listingCurrent
            ? activeCard?.name ?? (active ? text.unnamed : text.noActive)
            : text.unknownActive}</strong></p>
          {mute.muted === true && <span className="packs-problem" role="status">{text.muted}</span>}
        </div>
        <VolumeControl compact locale={locale} />
        <AudioMuteControl control={mute} locale={locale} />
      </div>
      <div className="packs-tabs" role="tablist" aria-label={text.tabsLabel}>
        {PACKS_TABS.map(name => <button key={name} type="button" id={`${id}-tab-${name}`}
          role="tab" aria-controls={`${id}-panel-${name}`} aria-selected={tab === name} tabIndex={tab === name ? 0 : -1}
          ref={node => { if (node) tabs.current[name] = node; }} onClick={() => changeTab(name)}
          onKeyDown={event => {
            const next = nextPacksTab(name, event.key);
            if (!next) return;
            event.preventDefault();
            changeTab(next);
            tabs.current[next]?.focus();
          }}>{text.tabs[name]}</button>)}
      </div>
      {PACKS_TABS.map(name => <div key={name} id={`${id}-panel-${name}`} role="tabpanel" aria-labelledby={`${id}-tab-${name}`}
        hidden={tab !== name} className="packs-scroll" tabIndex={0}>
        {tab === name && name === "installed" && <>
          <div className="packs-section-heading">
            <h2>{text.tabs.installed}</h2>
            <button type="button" className="packs-store-button" disabled={!onStoreOpen}
              onClick={onStoreOpen}>{text.storeLabel}</button>
            <button type="button" className="primary packs-install-trigger" disabled={disabled || action !== null}
              aria-haspopup="dialog" onClick={() => setInstallDialogOpen(true)}>{installText.title}</button>
            <button type="button" disabled={disabled || listing.busy} aria-busy={listing.busy} onClick={refresh}>{text.retry}</button>
          </div>
          {packLoadout && <section className="packs-equipped" aria-label={locale === "th" ? "ชุดแพ็กเสียงที่ติดตั้ง" : "Equipped announcer packs"}>
            <header><h3>{locale === "th" ? `ติดตั้งแล้ว (${packLoadout.loadout.equipped.length} จาก ${packLoadout.max_equipped})` : `Equipped (${packLoadout.loadout.equipped.length} of ${packLoadout.max_equipped})`}</h3>
              <label><input type="checkbox" checked={packLoadout.loadout.match_consistency} disabled={locked || action !== null}
                onChange={event => void updateLoadout(packLoadout.loadout.equipped.map(pack => pack.install_id), undefined, event.currentTarget.checked)} />
                {locale === "th" ? "ใช้เสียงแพ็กเดิมต่อเหตุการณ์เดิมตลอดแมตช์" : "Same voice per event for the whole match"}</label>
            </header>
            <div className="packs-equipped-slots">{Array.from({ length: packLoadout.max_equipped }, (_, slot) => {
              const equipped = packLoadout.loadout.equipped[slot];
              const card = equipped && cards.find(pack => pack.installId === equipped.install_id);
              return <div className="packs-equipped-slot" key={slot}>
                <strong>{slot + 1}</strong>{card ? <><span dir="auto">{card.name ?? text.unnamed}</span>
                  <small>{card.packKind === "unknown" ? (locale === "th" ? "ชนิดแพ็กไม่ทราบ" : "Pack kind unknown") : card.packKind}</small>
                  <div><button type="button" disabled={locked || action !== null || slot === 0} aria-label={locale === "th" ? "เลื่อนแพ็กไปทางซ้าย" : "Move pack left"} onClick={() => movePack(card.installId, -1)}>←</button>
                    <button type="button" disabled={locked || action !== null || slot >= packLoadout.loadout.equipped.length - 1} aria-label={locale === "th" ? "เลื่อนแพ็กไปทางขวา" : "Move pack right"} onClick={() => movePack(card.installId, 1)}>→</button>
                    <button type="button" disabled={locked || action !== null} onClick={() => togglePack(card)}>{locale === "th" ? "ถอดออก" : "Unequip"}</button></div>
                </> : <span className="muted">{locale === "th" ? "ว่าง" : "Empty"}</span>}
              </div>;
            })}</div>
            {packLoadout.pending && <p role="status">{locale === "th" ? "ชุดแพ็กใหม่จะเริ่มใช้หลังจบแมตช์: " : "Pending loadout applies after the match: "}
              {packLoadout.pending.equipped.map(pack => cards.find(item => item.installId === pack.install_id)?.name ?? text.unnamed).join(" + ") || (locale === "th" ? "ไม่มีแพ็ก" : "No packs")}
              <button type="button" disabled={action !== null} onClick={() => void cancelPendingLoadout()}>{locale === "th" ? "ยกเลิก" : "Cancel"}</button></p>}
          </section>}
          <div className="packs-toolbar">
            <fieldset className="packs-origin-filters" disabled={!sourceFiltersEnabled}>
              <legend>{text.filterLegend}</legend>
              <label><input type="checkbox" checked={officialFilter} onChange={event => setOfficialFilter(event.currentTarget.checked)} />
                {text.official}<output>{originCounts.official}</output></label>
              <label><input type="checkbox" checked={customFilter} onChange={event => setCustomFilter(event.currentTarget.checked)} />
                {text.custom}<output>{originCounts.custom}</output></label>
            </fieldset>
            <label className="packs-toolbar-search"><span className="sr-only">{text.searchPacks}</span>
              <input type="search" value={search} placeholder={text.searchPacks} onChange={event => setSearch(event.currentTarget.value)} />
            </label>
            <label><span className="sr-only">{text.sortLabel}</span><select aria-label={text.sortLabel} value={sort}
              onChange={event => setSort(event.currentTarget.value as BrowseFilters["sort"])}>
              <option value="active-first">{text.sortActive}</option><option value="name">{text.sortName}</option>
            </select></label>
            <span className="packs-count">{cards.length} · {text.originOfficial} {originCounts.official} · {text.originCustom} {originCounts.custom}</span>
            {!sourceFiltersEnabled && <p className="packs-toolbar-note">{text.unknownOrigin} · {originCounts.unknown}</p>}
          </div>
          <p role="status">{actionNotice}</p>
          {locked && <p className="packs-lock">{text.matchLock}</p>}
          {(disabled || listing.state === "failed" || listing.state === "unreadable" || summariesFailed)
            && <p role="status">{text.listError}</p>}
          {!listingCurrent && cards.length > 0 && <p className="muted">{text.retainedPacks}</p>}
          {!disabled && (listing.state === "loading" || (cards.length > 0 && !summaries && !summariesFailed))
            && <p role="status">{text.loading}</p>}
          {listingCurrent && cards.length === 0 && <p className="packs-empty">{text.empty}</p>}
          {listingCurrent && cards.length > 0 && visibleCards.length === 0 && <div className="packs-empty">
            <p>{text.emptyFilter}</p><button type="button" onClick={() => { setOfficialFilter(true); setCustomFilter(true); setSearch(""); }}>{text.showAll}</button>
          </div>}
          {(summaries || summariesFailed || !listingCurrent) && <div className="packs-grid">
            {visibleCards.map(pack => <PackCard key={pack.installId} pack={pack} locale={locale}
              unavailable={!listingCurrent || action !== null || busyKey !== null} busy={action === pack.installId}
              onActivate={() => togglePack(pack)}
              onDetails={from => { setAuditionNotice(null); setOpen({ id: pack.installId, from }); }} />)}
          </div>}
        </>}
        {tab === name && name === "events" && <>
          <div className="packs-section-heading packs-sound-heading">
            <div className="packs-sound-heading-copy">
              <h2>{text.tabs.events}</h2>
          {listingCurrent && <p className="packs-sound-active-pack" dir="auto">{locale === "th" ? "ติดตั้งแล้ว: " : "Equipped: "}{cards.filter(pack => pack.equippedSlot !== null).map(pack => pack.name ?? text.unnamed).join(" + ") || (activeCard?.name ?? text.noActive)}</p>}
            </div>
            <button type="button" className="packs-sound-retry" aria-label={text.retry}
              disabled={disabled || listing.busy} onClick={refresh}>
              <span aria-hidden="true">↻</span>{text.retry}
            </button>
          </div>
          {locked && <p className="packs-lock">{text.matchLock}</p>}
          <p role="status" className="packs-audition-result">{auditionNotice}</p>
          {(disabled || mapError) && <p role="status">{text.eventLoadError}</p>}
            {!disabled && !mapLoad && <p role="status">{text.loading}</p>}
          {currentMap?.loadout && <CoverageMatrix rows={currentMap.bindings} loadout={currentMap.loadout} locale={locale}
              pending={currentMap.pending_loadout} packs={cards} prewarm={currentMap.prewarm}
            disabled={disabled || locked || mute.muted === true || action !== null || busyKey !== null}
            onLock={setEventLock} onResult={row => void play(row)} />}
        </>}
        {tab === name && name === "recap" && (
          currentRecap
            ? <MatchHistory locale={locale} recap={currentRecap} onClear={clearRecap} clearDisabled={disabled || clearingRecap} />
            : <p role="status">{copy(locale).history.loading}</p>
        )}
      </div>)}
      {detail && open && <PackDrawer key={detail.installId} pack={detail} packs={visibleCards} detail={detailData} loading={detailLoading} locale={locale}
        preview={preview} previewNotice={previewNotice} matchLocked={browseMatchLocked || mute.muted === true} overlayPreview={overlayPreview}
        previewVideo={previewVideo}
        onOverlayPreview={value => setOverlayPreview(detail.active && value)} returnFocus={open.from}
        onClose={() => { setOpen(null); commitPreview({ type: "reset" }); }}
        onStep={pack => setOpen(current => current ? { ...current, id: pack.installId } : current)}
        onEquip={() => togglePack(detail)} actionBusy={action !== null || busyKey !== null}
        onAuditionVariant={auditionBrowseVariant} onPlayCategory={bindings => void playCategory(bindings)}
        onPreviewVisualError={reportPreviewVisualError} />}
    </>}
    {installDialogOpen && <InstallDialog locale={locale} matchInProgress={locked}
      onClose={() => setInstallDialogOpen(false)} onInstalled={refresh} />}
  </section>;
}
