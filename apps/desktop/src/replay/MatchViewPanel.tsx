import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Locale } from "../copy";
import { STAT_ICON_CATALOG, type StatIconId } from "../live/statIcons/catalog";
import { statIconUrl } from "../live/statIcons/assets";
import { BUNDLED } from "./bundledIcons";
import {
  admitLibraryBundle, countFound, gameModeName, iconResolver, neededIconKeys,
  type MatchBundle,
} from "./matchBundle";
import {
  chooseMapCalibration, replayPackForPatch,
  type ReplayPatchData,
} from "./replayPack.mjs";
import {
  replayListViewModel, replayMetadataFromBundle,
  type ReplayMetadata,
} from "./replayLibraryModel";
import { createReplayLibraryClient, type ReplayLibraryList, type ReplayStatus } from "./replayLibraryClient";
import { matchViewerCopy } from "./matchViewerCopy.mjs";
import { mountMatchView } from "./matchViewer.mjs";
import markup from "./matchViewer.html?raw";
import style from "./matchViewer.css?raw";
import sharedHeroCardStyle from "../live/heroCard.css?raw";
import libraryStyle from "./matchLibrary.css?raw";

type HeroCardLabelMode = "text" | "icon";

const replayLibrary = createReplayLibraryClient({
  invoke: <T,>(command: string, args?: Record<string, unknown>) => invoke<T>(command, args),
  listen: async <T,>(event: string, handler: (payload: T) => void) =>
    listen<T>(event, message => handler(message.payload)),
});

interface LoadedMatch {
  matchId: string;
  bundle: MatchBundle;
  pack: ReplayPatchData;
  calibration: { size: number; a: number; b: number; c: number; d: number };
  calibrationSource: "manifest" | "pack";
  foundIcons: number;
  neededIcons: number;
  initialTime: number | null;
}

const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.round(seconds % 60)).padStart(2, "0")}`;

export function MatchViewPanel({ locale, initialBundle = null, initialMatchId = null, initialTime = null, detailOnly = false }: {
  locale: Locale;
  initialBundle?: MatchBundle | null;
  initialMatchId?: string | null;
  initialTime?: number | null;
  detailOnly?: boolean;
}) {
  const text = matchViewerCopy[locale];
  const [library, setLibrary] = useState<ReplayLibraryList | null>(null);
  const [metadata, setMetadata] = useState<Record<string, ReplayMetadata>>({});
  const [statusOverrides, setStatusOverrides] = useState<Record<string, ReplayStatus>>({});
  const [loaded, setLoaded] = useState<LoadedMatch | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const listRequest = useRef(0);
  const dropTarget = useRef<HTMLElement>(null);
  const viewerTarget = useRef<HTMLDivElement>(null);

  const refresh = useCallback(async (showLoading = false) => {
    const ticket = ++listRequest.current;
    if (showLoading) setLoading(true);
    try {
      const next = await replayLibrary.list();
      if (ticket !== listRequest.current) return;
      setLibrary(next);
      setError(null);
    } catch {
      if (ticket === listRequest.current) setError(text.loadError);
    } finally {
      if (ticket === listRequest.current) setLoading(false);
    }
  }, [text.loadError]);

  useEffect(() => {
    if (detailOnly) { setLoading(false); return; }
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void refresh(true);
    void replayLibrary.onStateChanged(() => {
      if (!disposed) void refresh();
    }).then(stop => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => {
      if (!disposed) setError(text.loadError);
    });
    return () => {
      disposed = true;
      listRequest.current++;
      unlisten?.();
    };
  }, [detailOnly, refresh, text.loadError]);

  useEffect(() => {
    if (!detailOnly) return;
    setLoading(false);
    if (!initialBundle || !initialMatchId) {
      setLoaded(null);
      setError(text.openError);
      return;
    }
    const lookup = replayPackForPatch(initialBundle.patch);
    if (lookup.state === "needs_pack") {
      setLoaded(null);
      setError(text.needsPack(lookup.patch));
      return;
    }
    const map = chooseMapCalibration(initialBundle.calibration, initialBundle.mapSize, lookup.pack.map);
    if (!map.calibration || map.source === "unknown") {
      setLoaded(null);
      setError(text.calibrationError);
      return;
    }
    const keys = neededIconKeys(initialBundle);
    setLoaded({
      matchId: initialMatchId,
      bundle: initialBundle,
      pack: lookup.pack,
      calibration: map.calibration,
      calibrationSource: map.source,
      foundIcons: countFound(BUNDLED.icons, keys),
      neededIcons: keys.size,
      initialTime,
    });
    setError(null);
  }, [detailOnly, initialBundle, initialMatchId, initialTime, text.calibrationError, text.needsPack, text.openError]);

  useEffect(() => {
    const frame = viewerTarget.current;
    if (!loaded || !frame) return;
    const root = frame.shadowRoot ?? frame.attachShadow({ mode: "open" });
    const mode = gameModeName(loaded.bundle.gameMode) ?? text.unknownMode;
    const patch = loaded.bundle.patch ?? text.unknownPatch;
    const length = loaded.bundle.durationS === null ? text.unknownLength : clock(loaded.bundle.durationS);
    const notes = [
      loaded.calibrationSource === "manifest" ? text.calibrationManifest : text.calibrationPack,
      text.abilityDataSource(loaded.pack.ability_data_source?.build ?? null),
      ...(loaded.pack.ultimate_classification.available ? [] : [text.ultimateUnavailable]),
    ];
    let unmount: (() => void) | undefined;
    try {
      unmount = mountMatchView(root, loaded.bundle, {
      locale, markup, style: `${style}\n${sharedHeroCardStyle}`, statLabelMode: "text",
      icon: iconResolver(BUNDLED.icons),
      statCatalog: STAT_ICON_CATALOG,
      statIcon: (id: StatIconId) => statIconUrl(id),
      hasMap: BUNDLED.icons.has("map:"),
      patchData: loaded.pack,
      mapCalibration: loaded.calibration,
        initialTime: loaded.initialTime,
      header: {
        "mv-lede": text.lede(patch, mode, length),
        "mv-sub": text.sub(patch, mode),
        "mv-proto": notes.join(" "),
        "al-ult-copy": loaded.pack.ultimate_classification.available ? text.ultimateLabel : text.bkbOnlyLabel,
      },
      });
    } catch {
      // A failed mount must not look like "nothing happened": say so and return to the list.
      setLoaded(null);
      setError(text.openError);
      return;
    }
    // The viewer sits under the library list; bring it into view so Open visibly does something.
    if (!detailOnly) frame.closest("section")?.scrollIntoView({ behavior: "smooth", block: "start" });
    return unmount;
  }, [detailOnly, loaded, locale, text]);

  const viewRows = useMemo(() => library ? replayListViewModel(library.matches, metadata, {
    states: text.states,
    source: text.source,
    unknown: text.unknown,
    noDate: text.unknownMatchDate,
    dateAdded: text.dateAdded,
    duration: text.duration,
    heroes: text.heroList,
  }, locale, statusOverrides) : [], [library, metadata, locale, statusOverrides, text]);

  async function openMatch(matchId: string) {
    setBusy(`open:${matchId}`);
    setError(null);
    try {
      const opened = await replayLibrary.open(matchId);
      const admitted = admitLibraryBundle(opened);
      if (!admitted.ok) {
        setLoaded(null);
        setError(text.openError);
        return;
      }
      const bundle = admitted.bundle;
      const lookup = replayPackForPatch(bundle.patch);
      if (lookup.state === "needs_pack") {
        setMetadata(current => ({ ...current, [matchId]: replayMetadataFromBundle(bundle, null) }));
        setStatusOverrides(current => ({ ...current, [matchId]: { state: "needs_pack" } }));
        setLoaded(null);
        setError(text.needsPack(lookup.patch));
        return;
      }
      setStatusOverrides(current => {
        if (!current[matchId]) return current;
        const next = { ...current };
        delete next[matchId];
        return next;
      });
      const map = chooseMapCalibration(bundle.calibration, bundle.mapSize, lookup.pack.map);
      if (!map.calibration || map.source === "unknown") {
        setLoaded(null);
        setError(text.calibrationError);
        return;
      }
      const keys = neededIconKeys(bundle);
      setMetadata(current => ({ ...current, [matchId]: replayMetadataFromBundle(bundle, lookup.pack) }));
      setLoaded({
        matchId,
        bundle,
        pack: lookup.pack,
        calibration: map.calibration,
        calibrationSource: map.source,
        foundIcons: countFound(BUNDLED.icons, keys),
        neededIcons: keys.size,
        initialTime: null,
      });
      setError(null);
      void refresh();
    } catch {
      setLoaded(null);
      setError(text.openError);
    } finally {
      setBusy(null);
    }
  }

  async function parsePath(path: string) {
    const normalized = path.trim();
    if (!normalized.toLowerCase().endsWith(".dem")) {
      setError(text.invalidDem);
      return;
    }
    setBusy(`parse:${normalized}`);
    setError(null);
    try {
      await replayLibrary.parse(normalized);
      await refresh();
    } catch {
      setError(text.parseError);
    } finally {
      setBusy(null);
    }
  }

  async function parseEntry(matchId: string) {
    setBusy(`parse:${matchId}`);
    setError(null);
    try {
      await replayLibrary.parse(matchId);
      await refresh();
    } catch {
      setError(text.parseError);
    } finally {
      setBusy(null);
    }
  }

  async function chooseDemFile() {
    setBusy("pick");
    setError(null);
    try {
      const selectedPath = await replayLibrary.pickDemFile();
      if (selectedPath) await parsePath(selectedPath);
    } catch {
      setError(text.parseError);
    } finally {
      setBusy(null);
    }
  }

  async function removeEntry(matchId: string) {
    setBusy(`remove:${matchId}`);
    setError(null);
    try {
      await replayLibrary.remove(matchId);
      setMetadata(current => {
        const next = { ...current };
        delete next[matchId];
        return next;
      });
      setStatusOverrides(current => {
        const next = { ...current };
        delete next[matchId];
        return next;
      });
      if (loaded?.matchId === matchId) setLoaded(null);
      await refresh();
    } catch {
      setError(text.removeError);
    } finally {
      setBusy(null);
    }
  }

  async function clearLibrary() {
    if (!window.confirm(text.clearConfirm)) return;
    setBusy("clear");
    setError(null);
    try {
      await replayLibrary.clear();
      setLoaded(null);
      setMetadata({});
      setStatusOverrides({});
      await refresh();
    } catch {
      setError(text.clearError);
    } finally {
      setBusy(null);
    }
  }

  useEffect(() => {
    if (detailOnly) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview().onDragDropEvent(event => {
      if (event.payload.type !== "drop") return;
      const candidates = event.payload.paths.filter(path => path.toLowerCase().endsWith(".dem"));
      if (candidates.length === 1) void parsePath(candidates[0]!);
      else if (candidates.length > 1) setError(text.invalidDem);
    }).then(stop => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [detailOnly, text.invalidDem]);

  if (detailOnly) return (
    <section className="card replay-library replay-detail" data-replay-detail="true"
      style={{ height: "100%", minHeight: 0, overflow: "auto", alignContent: "start" }}>
      {loading && <p role="status">{text.loading}</p>}
      {error && <p role="alert">{error}</p>}
      {loaded && <section className="replay-library__viewer">
        <p role="status">{text.iconCount(loaded.foundIcons, loaded.neededIcons, BUNDLED.build)}</p>
        <div ref={viewerTarget} />
      </section>}
      <style>{libraryStyle}</style>
    </section>
  );

  return (
    <section ref={dropTarget} className="card replay-library">
      <header className="replay-library__header">
        <div>
          <h2>{text.libraryTitle}</h2>
          <p className="replay-library__watch"><strong>{text.watchFolder}:</strong> {library?.watched_replay_folder_path ?? text.watchFolderUnknown}</p>
        </div>
        <div className="replay-library__actions">
          <button type="button" onClick={() => void chooseDemFile()} disabled={busy !== null}>{text.addDem}</button>
          <button type="button" onClick={() => void clearLibrary()} disabled={busy !== null || !library?.matches.length}>{text.clear}</button>
        </div>
      </header>
      <p>{text.addDemHelp} {text.dropHelp}</p>
      <p className="replay-library__privacy">{text.libraryPrivacy}</p>
      {loading && <p role="status">{text.loading}</p>}
      {error && <p role="alert">{error}</p>}
      {!loading && viewRows.length === 0 && <p>{text.empty}</p>}
      {viewRows.length > 0 && (
        <ul className="replay-library__list">
          {viewRows.map(row => (
            <li key={row.matchId} className="replay-library__item">
              <div className="replay-library__details">
                <span className={`replay-library__state replay-library__state--${row.status.state}`} role="status">{row.statusLabel}</span>
                <strong>{text.date}: {row.matchDateLabel === text.unknown ? text.unknownMatchDate : row.matchDateLabel}</strong>
                <span>{text.hero}: {row.heroesLabel === text.unknown ? text.unknownHero : row.heroesLabel}</span>
                <span>{row.durationLabel === text.unknown ? text.unknownDuration : row.durationLabel}</span>
                <small>{row.sourceLabel} · {row.addedDateLabel}</small>
              </div>
              <div className="replay-library__row-actions">
                {row.canOpen && <button type="button" onClick={() => void openMatch(row.matchId)} disabled={busy !== null}>{text.open}</button>}
                {row.canParse && <button type="button" onClick={() => void parseEntry(row.matchId)} disabled={busy !== null}>{text.parse}</button>}
                <button type="button" onClick={() => void removeEntry(row.matchId)} disabled={busy !== null}>{text.remove}</button>
              </div>
            </li>
          ))}
        </ul>
      )}
      {loaded && (
        <section className="replay-library__viewer">
          <button type="button" onClick={() => setLoaded(null)}>{text.back}</button>
          <p role="status">{text.iconCount(loaded.foundIcons, loaded.neededIcons, BUNDLED.build)}</p>
          <div ref={viewerTarget} />
        </section>
      )}
      <style>{libraryStyle}</style>
    </section>
  );
}

/** A library-selected viewer surface used by Analytics; it keeps the same validated bundle and viewer. */
export function MatchViewerSurface({
  locale, bundle, onHeroSelect, onHeroIdentify, onTimeChange, onReady, initialTime = null, disableKeyboard = false, statLabelMode = "text",
}: {
  locale: Locale;
  bundle: MatchBundle;
  onHeroSelect?: (hero: string) => void;
  onHeroIdentify?: (hero: string) => void;
  onTimeChange?: (seconds: number) => void;
  onReady?: (root: ShadowRoot) => void;
  initialTime?: number | null;
  disableKeyboard?: boolean;
  statLabelMode?: HeroCardLabelMode;
}) {
  const target = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const frame = target.current;
    if (!frame) return;
    const lookup = replayPackForPatch(bundle.patch);
    if (lookup.state !== "ready") return;
    const map = chooseMapCalibration(bundle.calibration, bundle.mapSize, lookup.pack.map);
    if (!map.calibration || map.source === "unknown") return;
    const root = frame.shadowRoot ?? frame.attachShadow({ mode: "open" });
    const patch = bundle.patch ?? matchViewerCopy[locale].unknownPatch;
    const mode = gameModeName(bundle.gameMode) ?? matchViewerCopy[locale].unknownMode;
    const length = bundle.durationS === null ? matchViewerCopy[locale].unknownLength : clock(bundle.durationS);
    const mounted = mountMatchView(root, bundle, {
      locale, markup, style: `${style}\n${sharedHeroCardStyle}\n${analyticsSurfaceOverrides}`, statLabelMode,
      icon: iconResolver(BUNDLED.icons), hasMap: BUNDLED.icons.has("map:"), patchData: lookup.pack,
      statCatalog: STAT_ICON_CATALOG,
      statIcon: (id: StatIconId) => statIconUrl(id),
      mapCalibration: map.calibration,
      initialTime,
      disableKeyboard,
      header: {
        "mv-lede": matchViewerCopy[locale].lede(patch, mode, length),
        "mv-sub": matchViewerCopy[locale].sub(patch, mode),
        "mv-proto": "",
        "al-ult-copy": lookup.pack.ultimate_classification.available ? matchViewerCopy[locale].ultimateLabel : matchViewerCopy[locale].bkbOnlyLabel,
      },
    });
    const heroes = Array.isArray(bundle.match.heroes) ? bundle.match.heroes as { pid?: unknown; id?: unknown }[] : [];
    const heroFor = (event: Event) => {
      const card = (event.target as Element | null)?.closest<HTMLElement>(".rc[id^='card-']");
      if (!card) return null;
      const pid = Number(card.id.slice("card-".length));
      const hero = heroes.find(item => item.pid === pid && typeof item.id === "string");
      return typeof hero?.id === "string" ? hero.id : null;
    };
    const click = (event: Event) => { const hero = heroFor(event); if (hero) onHeroSelect?.(hero); };
    const doubleClick = (event: Event) => { const hero = heroFor(event); if (hero) onHeroIdentify?.(hero); };
    root.addEventListener("click", click);
    root.addEventListener("dblclick", doubleClick);
    const scrubber = root.querySelector<HTMLInputElement>("#rv-scrub");
    const scrub = () => { if (scrubber) { const time = Number(scrubber.value); if (Number.isFinite(time)) onTimeChange?.(time); } };
    scrubber?.addEventListener("input", scrub);
    onReady?.(root);
    return () => { root.removeEventListener("click", click); root.removeEventListener("dblclick", doubleClick); scrubber?.removeEventListener("input", scrub); mounted(); };
  }, [bundle, locale, onHeroSelect, onHeroIdentify, onTimeChange, onReady, initialTime, disableKeyboard, statLabelMode]);
  return <div ref={target} className="analytics-viewer-surface" />;
}

const analyticsSurfaceOverrides = `
#replay { gap: 8px; }
#mv-lede, #mv-proto, #rv-layers, .rv-lower, .dl { display: none !important; }
.rv-bar { display: none !important; }
#analytics-player-slot:empty { display: none; }
.rv-row.small { display: none; }
.rv-stage { height: auto; min-height: 560px; grid-template-columns: minmax(344px, 1fr) minmax(300px, 1.35fr) minmax(344px, 1fr); gap: 8px; align-items: stretch; }
.rv-col { height: 100%; grid-template-rows: repeat(5, minmax(104px, 1fr)); gap: 6px; align-content: stretch; overflow-y: auto; }
.rv-center { height: 100%; grid-template-rows: auto minmax(300px, 1fr); }
.rv-center { height: auto; grid-template-rows: auto min-content; align-content: start; }
.rv-mapcard { align-content: start; }
.rv-mapwrap { height: auto; max-height: none; min-height: 300px; aspect-ratio: 1 / 1; }
@media (max-width: 1500px) { .rv-stage { grid-template-columns: minmax(302px, 1fr) minmax(300px, 1.35fr) minmax(302px, 1fr); } }
`;

export const analyticsPlayerShadowCss = `
.analytics-player { display: grid; gap: 6px; margin-top: 8px; padding: 8px 10px 7px; border: 1px solid var(--line); border-radius: 10px; background: var(--bg); color: var(--ink); font: 12px/1.3 var(--f-body); }
.analytics-player-controls { display: grid; grid-template-columns: 34px 64px minmax(72px, 1fr) auto; align-items: center; gap: 6px; min-width: 0; }
.analytics-player-controls button, .analytics-player-speed select { min-width: 0; min-height: 32px; padding: 4px 6px; border: 1px solid var(--line); border-radius: 7px; background: var(--surface); color: var(--ink); font: 600 12px/1.2 var(--f-body); white-space: nowrap; }
.analytics-player-play { width: 34px; padding: 4px 0 !important; }
.analytics-player-speed select { width: 64px; height: 32px; }
.analytics-player-clock { min-width: 0; font: 12px var(--f-mono); text-align: center; white-space: nowrap; font-variant-numeric: tabular-nums; }
.analytics-player-details { border-color: var(--rad) !important; color: var(--ink) !important; }
.analytics-player-track { position: relative; height: 26px; }
.analytics-player-track input[type=range] { position: absolute; z-index: 1; inset: 0; width: 100%; height: 26px; margin: 0; appearance: none; background: transparent; cursor: pointer; }
.analytics-player-track input[type=range]::-webkit-slider-runnable-track { height: 6px; border-radius: 999px; background: linear-gradient(90deg, var(--rad) var(--played), var(--line) var(--played)); }
.analytics-player-track input[type=range]::-moz-range-track { height: 6px; border-radius: 999px; background: var(--line); }
.analytics-player-track input[type=range]::-moz-range-progress { height: 6px; border-radius: 999px; background: var(--rad); }
.analytics-player-track input[type=range]::-webkit-slider-thumb { width: 14px; height: 14px; margin-top: -4px; appearance: none; border: 2px solid var(--bg); border-radius: 50%; background: var(--rad); box-shadow: 0 0 0 1px var(--rad); }
.analytics-player-track input[type=range]::-moz-range-thumb { width: 10px; height: 10px; border: 2px solid var(--bg); border-radius: 50%; background: var(--rad); box-shadow: 0 0 0 1px var(--rad); }
.analytics-player-ticks { position: absolute; z-index: 2; inset: 0; pointer-events: none; }
.analytics-player-ticks button { position: absolute; top: 0; transform: translateX(-50%); padding: 0 2px; border: 0; background: transparent; color: var(--ink-2); font: 12px/1 var(--f-mono); pointer-events: auto; }
.analytics-player-ticks .analytics-player-team-2, .analytics-player-team-2 { color: var(--rad); }
.analytics-player-ticks .analytics-player-team-3, .analytics-player-team-3 { color: var(--dire); }
.analytics-player-citation { color: var(--warn) !important; }
.analytics-player-hover-time { position: absolute; z-index: 3; top: -4px; transform: translate(-50%, -100%); padding: 2px 5px; border: 1px solid var(--line); border-radius: 4px; background: var(--surface); color: var(--ink); font: 12px var(--f-mono); pointer-events: none; white-space: nowrap; }
.analytics-player-death-strip { display: flex; align-items: center; gap: 4px; min-width: 0; min-height: 24px; overflow-x: auto; color: var(--ink-2); }
.analytics-player-death-strip button { flex: none; min-height: 24px; padding: 2px 6px; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--ink-2); font: 12px var(--f-mono); white-space: nowrap; }
.analytics-player-visually-hidden { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0,0,0,0); white-space: nowrap; }
`;
