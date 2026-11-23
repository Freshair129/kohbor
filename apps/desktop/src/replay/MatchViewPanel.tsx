import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Locale } from "../copy";
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

const REPLAY_MAX_BYTES = 400 * 1024 * 1024;

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
  const filePicker = useRef<HTMLInputElement>(null);
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
      ...(loaded.pack.ultimate_classification.available ? [] : [text.ultimateUnavailable]),
    ];
    return mountMatchView(root, loaded.bundle, {
      locale, markup, style,
      icon: iconResolver(BUNDLED.icons),
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

  function onPickedFile(file: File | undefined) {
    if (!file) return;
    if (!file.name.toLowerCase().endsWith(".dem") || file.size > REPLAY_MAX_BYTES) {
      setError(text.invalidDem);
      return;
    }
    const selectedPath = (file as File & { path?: string }).path;
    if (!selectedPath) {
      setError(text.pickerPathUnavailable);
      return;
    }
    void parsePath(selectedPath);
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
          <p className="replay-library__watch"><strong>{text.watchFolder}:</strong> {text.watchFolderUnknown}</p>
        </div>
        <div className="replay-library__actions">
          <button type="button" onClick={() => filePicker.current?.click()} disabled={busy !== null}>{text.addDem}</button>
          <button type="button" onClick={() => void clearLibrary()} disabled={busy !== null || !library?.matches.length}>{text.clear}</button>
          <input
            ref={filePicker}
            className="replay-library__hidden-picker"
            type="file"
            accept=".dem,application/octet-stream"
            aria-label={text.addDem}
            onChange={event => {
              onPickedFile(event.currentTarget.files?.[0]);
              event.currentTarget.value = "";
            }}
          />
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
