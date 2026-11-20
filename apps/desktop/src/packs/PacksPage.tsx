import { useEffect, useId, useRef, useState } from "react";
import { AudioMuteControl, type AudioMuteState } from "../AudioPanel";
import { auditionReason, copy, packReason, packsPageCopy, type Locale, type Phase } from "../copy";
import { errorCode, newRequestId, parseInstallResult, sortRecords } from "../packs";
import { matchRecapCommand, mediaActivate, mediaAudition, mediaAuditionVariant, mediaEventMap, mediaPackSummaries, subscribeAuditionMatch } from "../runtime";
import { useMediaListing } from "../useMediaListing";
import { VolumeControl } from "../VolumeControl";
import {
  auditionSound, beginEventMapRefresh, eventMapMatchesPack, eventMapPhaseBlocksAudition,
  eventMapPlayBlock, soundKey, type EventMapLoadResult, type SoundKey,
} from "./eventMap";
import { MatchHistory } from "./MatchHistory";
import { clearMatchRecap, loadMatchRecap, type MatchRecapSnapshot } from "./recapModel";
import { nextPacksTab, PACKS_TABS, parsePackSummaries, type PacksTab, type PackSummary } from "./model";
import { PackDrawer, PackCard } from "./PackLibrary";
import { SoundRows } from "./SoundRows";
import { InstallDialog } from "./InstallDialog";
import { installDialogCopy } from "./InstallDialog.copy";

export function PacksPage({ locale, disabled, phase, mute }: {
  locale: Locale; disabled: boolean; phase: Phase | null; mute: AudioMuteState;
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
  const [mapLoad, setMapLoad] = useState<EventMapLoadResult | null>(null);
  const [matchSignal, setMatchSignal] = useState<{ active: boolean | null } | null>(null);
  const [action, setAction] = useState<string | null>(null);
  const [activationErrors, setActivationErrors] = useState<Record<string, string>>({});
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [auditionNotice, setAuditionNotice] = useState<string | null>(null);
  const [installDialogOpen, setInstallDialogOpen] = useState(false);
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
    void mediaPackSummaries().then(raw => {
      if (!current) return;
      const result = parsePackSummaries(raw);
      setSummaries(result);
      setSummariesFailed(result === null);
    }, () => {
      if (current) { setSummaries(null); setSummariesFailed(true); }
    });
    return () => { current = false; };
  }, [inventoryKey, listingCurrent, refreshId]);

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
    if (disabled || matchSignal === null || (tab !== "events" && !open)) return;
    setMapLoad(null);
    return beginEventMapRefresh(mediaEventMap, setMapLoad);
  }, [activeId, phaseLock, matchSignal, refreshId, disabled, tab, open]);

  const summariesById = new Map(summaries?.map(pack => [pack.installId, pack]));
  const cards = (listing.listing ? sortRecords(listing.listing.records) : []).map(record => {
    const summary = summariesById.get(record.install_id);
    const verified = summary?.packageId === record.package_id && summary.revision === record.revision;
    return {
      ...(verified ? summary : {
        installId: record.install_id, packageId: record.package_id, revision: record.revision,
        name: null, problem: "metadata" as const, coverUrl: null, counts: null, bindings: [],
      }),
      active: activeId === record.install_id,
      pending: listing.listing?.pending.includes(record.install_id) === true,
    };
  });
  const activeCard = cards.find(pack => pack.active);
  const detail = open && cards.find(pack => pack.installId === open.id);
  const loadedMap = mapLoad?.state === "ok" ? mapLoad.eventMap : null;
  const matchesActive = loadedMap !== null && eventMapMatchesPack(loadedMap, listingCurrent ? active : undefined);
  const currentMap = matchesActive && !disabled ? loadedMap : null;
  const locked = phaseLock || matchSignal?.active === true || currentMap?.match_active === true;
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

  const activate = async (pack: PackSummary) => {
    if (!listingCurrent || pack.active || pack.pending || actionLock.current || auditionLock.current) return;
    actionLock.current = true;
    setAction(pack.installId);
    setActionNotice(null);
    setAuditionNotice(null);
    setActivationErrors(errors => {
      const next = { ...errors };
      delete next[pack.installId];
      return next;
    });
    try {
      const result = parseInstallResult(await mediaActivate(newRequestId(), pack.installId, "after_match"));
      if (!result) throw new Error("UNKNOWN");
      if (result.status === "rejected" || result.status === "activation_error") {
        const message = `${text.outcomes[result.status]} · ${packReason(result.reason, locale)}`;
        setActivationErrors(errors => ({ ...errors, [pack.installId]: message }));
        setActionNotice(message);
      } else {
        setActionNotice(text.outcomes[result.status]);
      }
    } catch (failure) {
      const message = `${text.outcomes.activation_error} · ${packReason(errorCode(failure), locale)}`;
      setActivationErrors(errors => ({ ...errors, [pack.installId]: message }));
      setActionNotice(message);
    } finally {
      setMapLoad(null);
      await listing.refresh();
      setRefreshId(value => value + 1);
      setAction(null);
      actionLock.current = false;
    }
  };
  const listen = { block, play: (row: SoundKey) => void play(row), busyKey };
  const detailHasCurrentPack = !!detail && currentMap?.active?.install_id === detail.installId
    && currentMap.active.revision === detail.revision && currentMap.active.package_id === detail.packageId;
  const detailListen = {
    ...listen,
    block: (row: SoundKey) => block(row) ?? (detailHasCurrentPack ? null : "AUDITION_MEDIA_STATE_UNKNOWN"),
    play: (row: SoundKey) => { if (detailHasCurrentPack) void play(row); },
  };
  const mapError = !!mapLoad && !currentMap;
  const changeTab = (name: PacksTab) => {
    setAuditionNotice(null);
    setTab(name);
  };

  return <section className="shell-screen packs-page" aria-labelledby="shell-page-heading">
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
          <button type="button" className="primary packs-install-trigger" disabled={disabled || action !== null}
            aria-haspopup="dialog" onClick={() => setInstallDialogOpen(true)}>{installText.title}</button>
          <button type="button" disabled={disabled || listing.busy} aria-busy={listing.busy} onClick={refresh}>{text.retry}</button>
        </div>
        <p role="status">{actionNotice}</p>
        {locked && <p className="packs-lock">{text.matchLock}</p>}
        {(disabled || listing.state === "failed" || listing.state === "unreadable" || summariesFailed)
          && <p role="status">{text.listError}</p>}
        {!listingCurrent && cards.length > 0 && <p className="muted">{text.retainedPacks}</p>}
        {!disabled && (listing.state === "loading" || (cards.length > 0 && !summaries && !summariesFailed))
          && <p role="status">{text.loading}</p>}
        {listingCurrent && cards.length === 0 && <p className="packs-empty">{text.empty}</p>}
        {(summaries || summariesFailed || !listingCurrent) && <div className="packs-grid">
          {cards.map(pack => <PackCard key={pack.installId} pack={pack} locale={locale}
            unavailable={!listingCurrent || action !== null || busyKey !== null} busy={action === pack.installId}
            activationError={activationErrors[pack.installId]} onActivate={() => void activate(pack)}
            onDetails={from => { setAuditionNotice(null); setOpen({ id: pack.installId, from }); }} />)}
        </div>}
      </>}
      {tab === name && name === "events" && <>
        <div className="packs-section-heading">
          <h2>{text.tabs.events}</h2>
          <button type="button" disabled={disabled || listing.busy} onClick={refresh}>{text.retry}</button>
        </div>
        {locked && <p className="packs-lock">{text.matchLock}</p>}
        <p role="status" className="packs-audition-result">{auditionNotice}</p>
        {(disabled || mapError) && <p role="status">{text.eventLoadError}</p>}
        {!disabled && !mapLoad && <p role="status">{text.loading}</p>}
        {currentMap && <SoundRows rows={currentMap.bindings} locale={locale} listen={listen}
          disabled={disabled || locked || mute.muted === true || action !== null || busyKey !== null}
          onAuditionVariant={(row, variantId) => mediaAuditionVariant(row.event_id, row.tier, variantId)} />}
      </>}
      {tab === name && name === "recap" && (
        currentRecap
          ? <MatchHistory locale={locale} recap={currentRecap} onClear={clearRecap} clearDisabled={disabled || clearingRecap} />
          : <p role="status">{copy(locale).history.loading}</p>
      )}
    </div>)}
    {detail && open && <PackDrawer pack={detail} locale={locale} listen={detailListen} returnFocus={open.from}
      onClose={() => setOpen(null)} auditionNotice={auditionNotice} matchLocked={locked}
      variants={detailHasCurrentPack ? currentMap?.bindings ?? [] : []}
      onAuditionVariant={(row, variantId) => mediaAuditionVariant(row.event_id, row.tier, variantId)} />}
    {installDialogOpen && <InstallDialog locale={locale} matchInProgress={locked}
      onClose={() => setInstallDialogOpen(false)} onInstalled={refresh} />}
  </section>;
}
