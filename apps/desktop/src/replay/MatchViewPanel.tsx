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
import libraryStyle from "./matchLibrary.css?raw";

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
}

const clock = (seconds: number) => `${Math.floor(seconds / 60)}:${String(Math.round(seconds % 60)).padStart(2, "0")}`;

export function MatchViewPanel({ locale }: { locale: Locale }) {
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
  }, [refresh, text.loadError]);

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
      locale, markup, style,
      icon: iconResolver(BUNDLED.icons),
      statCatalog: STAT_ICON_CATALOG,
      statIcon: (id: StatIconId) => statIconUrl(id),
      hasMap: BUNDLED.icons.has("map:"),
      patchData: loaded.pack,
      mapCalibration: loaded.calibration,
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
    frame.closest("section")?.scrollIntoView({ behavior: "smooth", block: "start" });
    return unmount;
  }, [loaded, locale, text]);

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
  }, [text.invalidDem]);

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
  locale, bundle, onHeroSelect, onHeroIdentify, onTimeChange, onReady,
}: {
  locale: Locale;
  bundle: MatchBundle;
  onHeroSelect?: (hero: string) => void;
  onHeroIdentify?: (hero: string) => void;
  onTimeChange?: (seconds: number) => void;
  onReady?: (root: ShadowRoot) => void;
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
      locale, markup, style: `${style}\n${analyticsSurfaceOverrides}`,
      icon: iconResolver(BUNDLED.icons), hasMap: BUNDLED.icons.has("map:"), patchData: lookup.pack,
      statCatalog: STAT_ICON_CATALOG,
      statIcon: (id: StatIconId) => statIconUrl(id),
      mapCalibration: map.calibration,
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
  }, [bundle, locale, onHeroSelect, onHeroIdentify, onTimeChange, onReady]);
  return <div ref={target} className="analytics-viewer-surface" />;
}

const analyticsSurfaceOverrides = `
#replay { gap: 8px; }
#mv-lede, #mv-proto, #rv-layers, .rv-lower, .dl { display: none !important; }
.rv-bar { padding: 8px; gap: 6px; }
.rv-row.small { display: none; }
.rv-stage { height: 100%; min-height: 400px; grid-template-columns: minmax(140px, 1fr) minmax(300px, 1.35fr) minmax(140px, 1fr); gap: 8px; align-items: stretch; }
.rv-col { height: 100%; grid-template-rows: repeat(5, minmax(82px, 1fr)); gap: 6px; align-content: stretch; }
.rv-center { height: 100%; grid-template-rows: auto minmax(300px, 1fr); }
.rc { height: 100%; min-height: 82px; grid-template-columns: 34px minmax(0, 1fr); padding: 5px; gap: 2px 6px; }
.rc .pt, .rc .pt img { width: 34px; height: 34px; }
.rc .row.ab, .rc .row.it, .rc .row.st { display: none; }
.rc .row.dvs { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.rv-center { align-content: start; }
.rv-mapwrap { height: 100%; max-height: 500px; min-height: 300px; }
@media (max-width: 1280px) { .rv-stage { grid-template-columns: minmax(110px, .8fr) minmax(300px, 1.6fr) minmax(110px, .8fr); } }
`;
