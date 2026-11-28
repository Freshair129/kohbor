import { useEffect, useRef, useState } from "react";
import type { Locale } from "../copy";
import { DEFAULT_HERO_CARD_DISPLAY, HERO_CARD_DISPLAY_EVENT, HERO_CARD_DISPLAY_KEY, readHeroCardDisplay, type HeroCardDisplay } from "../shared/heroCardDisplay.mjs";
import { STAT_ICON_CATALOG, type StatIconId } from "../live/statIcons/catalog";
import { statIconUrl } from "../live/statIcons/assets";
import { BUNDLED, replayIconsForPatch } from "./bundledIcons";
import { countFound, gameModeName, iconResolver, neededIconKeys, type MatchBundle } from "./matchBundle";
import { chooseMapCalibration, replayPackForPatch, type ReplayPatchData } from "./replayPack.mjs";
import { matchViewerCopy } from "./matchViewerCopy.mjs";
import { mountMatchView } from "./matchViewer.mjs";
import markup from "./matchViewer.html?raw";
import style from "./matchViewer.css?raw";
import sharedHeroCardStyle from "../live/heroCard.css?raw";
import libraryStyle from "./matchLibrary.css?raw";

type HeroCardLabelMode = "text" | "icon";

interface LoadedMatch {
  bundle: MatchBundle;
  pack: ReplayPatchData;
  calibration: { size: number; a: number; b: number; c: number; d: number };
  calibrationSource: "pack";
  icons: Map<string, string>;
  foundIcons: number;
  neededIcons: number;
  initialTime: number | null;
}

const clock = (seconds: number) => String(Math.floor(seconds / 60)) + ":" + String(Math.round(seconds % 60)).padStart(2, "0");

export function MatchViewPanel({ locale, initialBundle = null, initialMatchId = null, initialTime = null }: {
  locale: Locale;
  initialBundle?: MatchBundle | null;
  initialMatchId?: string | null;
  initialTime?: number | null;
}) {
  const text = matchViewerCopy[locale];
  const [loaded, setLoaded] = useState<LoadedMatch | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [heroCardDisplay, setHeroCardDisplay] = useState<HeroCardDisplay>(() => {
    try { return readHeroCardDisplay(window.localStorage); } catch { return DEFAULT_HERO_CARD_DISPLAY; }
  });
  const viewerTarget = useRef<HTMLDivElement>(null);
  const activeViewerRoot = useRef<ShadowRoot | null>(null);

  useEffect(() => {
    const syncStorage = (event: StorageEvent) => {
      if (event.key !== HERO_CARD_DISPLAY_KEY) return;
      try { setHeroCardDisplay(readHeroCardDisplay(window.localStorage)); } catch { setHeroCardDisplay(DEFAULT_HERO_CARD_DISPLAY); }
    };
    const syncSameWindow = (event: Event) => {
      const value = (event as CustomEvent<{ value?: HeroCardDisplay }>).detail?.value;
      if (value) setHeroCardDisplay(value);
    };
    window.addEventListener("storage", syncStorage);
    window.addEventListener(HERO_CARD_DISPLAY_EVENT, syncSameWindow);
    return () => {
      window.removeEventListener("storage", syncStorage);
      window.removeEventListener(HERO_CARD_DISPLAY_EVENT, syncSameWindow);
    };
  }, []);
  useEffect(() => {
    activeViewerRoot.current?.dispatchEvent(new CustomEvent("hero-card-display-change", {
      detail: { heroCardDisplay, density: "full", selectedHeroId: null, cardsCollapsed: false },
    }));
  }, [heroCardDisplay]);

  useEffect(() => {
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
    const icons = replayIconsForPatch(initialBundle.patch);
    setLoaded({
      bundle: initialBundle,
      pack: lookup.pack,
      calibration: map.calibration,
      calibrationSource: map.source,
      foundIcons: countFound(icons, keys),
      neededIcons: keys.size,
      icons,
      initialTime,
    });
    setError(null);
  }, [initialBundle, initialMatchId, initialTime, text.calibrationError, text.needsPack, text.openError]);

  useEffect(() => {
    const frame = viewerTarget.current;
    if (!loaded || !frame) return;
    const root = frame.shadowRoot ?? frame.attachShadow({ mode: "open" });
    activeViewerRoot.current = root;
    const mode = gameModeName(loaded.bundle.gameMode) ?? text.unknownMode;
    const patch = loaded.bundle.patch ?? text.unknownPatch;
    const length = loaded.bundle.durationS === null ? text.unknownLength : clock(loaded.bundle.durationS);
    const notes = [
      text.calibrationPack,
      text.abilityDataSource(loaded.pack.ability_data_source?.build ?? null),
      ...(loaded.pack.ultimate_classification.available ? [] : [text.ultimateUnavailable]),
    ];
    let unmount: (() => void) | undefined;
    try {
      unmount = mountMatchView(root, loaded.bundle, {
        locale, markup, style: style + "\n" + sharedHeroCardStyle + "\n" + replayHeroCardOverrides, statLabelMode: "text",
        heroCardDisplay, density: "full", selectedHeroId: null, cardsCollapsed: false,
        icon: iconResolver(loaded.icons),
        hasIcon: (kind: "ability" | "item", name: string) => loaded.icons.has(kind + ":" + name.toLowerCase()),
        statCatalog: STAT_ICON_CATALOG,
        statIcon: (id: StatIconId) => statIconUrl(id),
        hasMap: loaded.icons.has("map:"),
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
      setLoaded(null);
      setError(text.openError);
      return;
    }
    return () => {
      activeViewerRoot.current = null;
      unmount?.();
    };
  }, [loaded, locale, text]);

  return (
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
}

/** A library-selected viewer surface used by Analytics; it keeps the same validated bundle and viewer. */
export function MatchViewerSurface({
  locale, bundle, onHeroSelect, onTimeChange, onReady, initialTime = null, disableKeyboard = false, statLabelMode = "text",
  heroCardDisplay = DEFAULT_HERO_CARD_DISPLAY, selectedHeroId = null, density = "full", cardsCollapsed = false,
}: {
  locale: Locale;
  bundle: MatchBundle;
  onHeroSelect?: (hero: string) => void;
  onTimeChange?: (seconds: number) => void;
  onReady?: (root: ShadowRoot) => void;
  initialTime?: number | null;
  disableKeyboard?: boolean;
  statLabelMode?: HeroCardLabelMode;
  heroCardDisplay?: HeroCardDisplay;
  selectedHeroId?: string | null;
  density?: "full" | "compact" | "roster";
  cardsCollapsed?: boolean | { radiant: boolean; dire: boolean };
}) {
  const target = useRef<HTMLDivElement>(null);
  const viewerRoot = useRef<ShadowRoot | null>(null);
  useEffect(() => {
    const frame = target.current;
    if (!frame) return;
    const lookup = replayPackForPatch(bundle.patch);
    if (lookup.state !== "ready") return;
    const map = chooseMapCalibration(bundle.calibration, bundle.mapSize, lookup.pack.map);
    if (!map.calibration || map.source === "unknown") return;
    const root = frame.shadowRoot ?? frame.attachShadow({ mode: "open" });
    const icons = replayIconsForPatch(bundle.patch);
    viewerRoot.current = root;
    const patch = bundle.patch ?? matchViewerCopy[locale].unknownPatch;
    const mode = gameModeName(bundle.gameMode) ?? matchViewerCopy[locale].unknownMode;
    const length = bundle.durationS === null ? matchViewerCopy[locale].unknownLength : clock(bundle.durationS);
    const mounted = mountMatchView(root, bundle, {
      locale, markup, style: `${style}\n${sharedHeroCardStyle}\n${analyticsSurfaceOverrides}`, statLabelMode,
      heroCardDisplay, density, selectedHeroId, cardsCollapsed,
      icon: iconResolver(icons), hasMap: icons.has("map:"), patchData: lookup.pack,
      hasIcon: (kind: "ability" | "item", name: string) => icons.has(`${kind}:${name.toLowerCase()}`),
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
    root.addEventListener("click", click);
    const scrubber = root.querySelector<HTMLInputElement>("#rv-scrub");
    const scrub = () => { if (scrubber) { const time = Number(scrubber.value); if (Number.isFinite(time)) onTimeChange?.(time); } };
    scrubber?.addEventListener("input", scrub);
    onReady?.(root);
    return () => { root.removeEventListener("click", click); scrubber?.removeEventListener("input", scrub); viewerRoot.current = null; mounted(); };
  }, [bundle, locale, onHeroSelect, onTimeChange, onReady, initialTime, disableKeyboard, statLabelMode]);
  useEffect(() => {
    viewerRoot.current?.dispatchEvent(new CustomEvent("hero-card-display-change", {
      detail: { heroCardDisplay, density, selectedHeroId, cardsCollapsed },
    }));
  }, [heroCardDisplay, selectedHeroId, density, cardsCollapsed]);
  return <div ref={target} className="analytics-viewer-surface" />;
}

const analyticsSurfaceOverrides = `
#replay { gap: 8px; }
#mv-lede, #mv-proto, #rv-layers, .rv-lower, .dl { display: none !important; }
.rv-bar { display: none !important; }
#analytics-player-slot:empty { display: none; }
.rv-row.small { display: none; }
.rv-stage { width: 100%; height: 100%; min-height: 0; grid-template-columns: var(--an-r-card-w, var(--an-card-w, 344px)) minmax(300px, var(--analytics-map-side, 600px)) var(--an-d-card-w, var(--an-card-w, 344px)); grid-template-rows: minmax(0, 1fr); gap: 8px; align-items: start; }
.rv-team-column { height: 100%; min-width: 0; }
.team-head { min-height: 30px; }
.rv-col { height: 100%; min-height: 0; grid-template-rows: none; grid-auto-rows: max-content; gap: 6px; align-content: start; overflow-y: auto; }
.rv-center { height: 100%; min-height: 0; grid-template-rows: min-content; align-content: start; position: relative; }
.rv-score { display: none !important; }
.rv-mapcard { align-content: start; }
.rv-mapwrap { width: 100%; height: var(--analytics-map-side, 600px); max-height: none; min-height: 300px; aspect-ratio: 1 / 1; position: relative; }
.rv-mapcard { padding: 4px; gap: 6px; }
.rv-legend, .rv-keys, .rv-multi { display: none !important; }
.rv-col { overflow-x: hidden; }
.rv-team-column .rc { width: calc(var(--an-r-card-w, var(--an-card-w, 344px)) / var(--hc-scale, 1)); min-width: 0; grid-template-areas: var(--hc-grid-areas, "pt top" "pt bars" "ab ab" "it it" "st st" "dv dv"); }
.team-dire .rc { width: calc(var(--an-d-card-w, var(--an-card-w, 344px)) / var(--hc-scale, 1)); }
.rv-team-column .rc .row.dvs { grid-template-columns: repeat(var(--cells, 1), minmax(0, 1fr)); }
.rv-team-column .rc .row.st { flex-wrap: wrap; height: auto; overflow: visible; white-space: normal; }
.rv-team-column .rc .row.st .at, .rv-team-column .rc .row.dvs .dv { min-width: 0; }
.rv-team-column .rc .row.st .stat-glyph img, .rv-team-column .rc .row.dvs .stat-glyph img { width: 14px; height: 14px; }
.rv-team-column .rc .row.st .stat-glyph, .rv-team-column .rc .row.dvs .stat-glyph { width: 14px; height: 14px; font-size: 13px; }
.rv-team-column .rc .bar-reg { font-size: 9px; }
.rv-team-column .rc .mods { min-width: 0; overflow: visible; flex-wrap: wrap; align-content: flex-start; }
.rv-team-column .rc .status-effect { width: 22px; height: 22px; border-radius: 5px; overflow: visible; }
.rv-team-column .rc .status-effect.buff { border-radius: 50%; }
.rv-team-column .rc .status-effect.debuff { border-radius: 6px; border-color: color-mix(in srgb, var(--bad) 70%, var(--line)); }
.rv-team-column .rc .status-effect.control { border-radius: 6px; border-color: var(--bad); box-shadow: 0 0 0 1px color-mix(in srgb, var(--bad) 45%, transparent); }
.rv-team-column .rc .status-effect.control .status-ring { background: conic-gradient(var(--bad) var(--p), transparent 0); }
.rv-team-column .rc .status-effect.buff .status-ring { background: conic-gradient(var(--muted) var(--p), transparent 0); }
.rv-team-column .rc .status-effect.debuff .status-ring { background: conic-gradient(var(--bad) var(--p), transparent 0); }
.rv-team-column .rc .status-effect.control small { color: var(--bad); }
.rv-team-column .rc .status-divider { width: 1px; height: 19px; flex: none; margin: 0 1px; background: var(--line); }
.rv-team-column .rc .status-overflow { min-width: 25px; height: 22px; flex: none; }
.team-winner { flex: none; padding: 2px 5px; border-radius: 4px; background: color-mix(in srgb, var(--good) 18%, var(--surface)); color: var(--good); font: 700 9px/1.2 var(--f-body); }
:host([data-analytics-density="compact"]) .rc:not(.selected-card) .row.it,
:host([data-analytics-density="compact"]) .rc:not(.selected-card) .row.st,
:host([data-analytics-density="compact"]) .rc:not(.selected-card) .row.dvs { display: none !important; }
:host([data-analytics-density="compact"]) .rc.selected-card { grid-row: auto; min-height: 0; overflow: visible; }
:host([data-analytics-density="compact"]) .rc.selected-card .row { min-height: 0; }
:host([data-analytics-density="roster"]) .rc .row { display: none !important; }
:host([data-cards-radiant-collapsed="true"]) .team-radiant .rv-col .rc .row,
:host([data-cards-dire-collapsed="true"]) .team-dire .rv-col .rc .row { display: none !important; }
:host([data-cards-radiant-collapsed="true"]) .team-radiant .rv-col,
:host([data-cards-dire-collapsed="true"]) .team-dire .rv-col { grid-template-columns: minmax(0, 1fr); }
.rv-team-column .rc.collapsed-column { display: flex; flex-direction: column; align-items: center; justify-content: start; width: calc(56px / var(--hc-scale, 1)); height: calc(72px / var(--hc-scale, 1)); min-height: 0; gap: 2px; padding: 3px; overflow: hidden; cursor: pointer; }
.rv-team-column .rc.collapsed-column .top, .rv-team-column .rc.collapsed-column .ic.ab { display: none !important; }
.rv-team-column .rc.collapsed-column .pt { width: 34px; height: 34px; }
.rv-team-column .rc.collapsed-column .pt img { width: 34px; height: 34px; }
.rv-team-column .rc.collapsed-column .bars { display: grid; width: 100%; gap: 1px; }
.rv-team-column .rc.collapsed-column .bar { height: 4px; }
.rv-team-column .rc.collapsed-column .bar.mp { height: 4px; }
.rv-team-column .rc.collapsed-column .bar .num, .rv-team-column .rc.collapsed-column .bar-reg { display: none; }
.rv-team-column .rc.collapsed-column .row.ab { display: flex; grid-area: auto; min-height: 9px; max-width: 100%; }
.rv-team-column .rc.collapsed-column .mods { flex: none; gap: 1px; max-width: 100%; overflow: visible; margin: 0; }
.rv-team-column .rc.collapsed-column .status-effect { width: 8px; height: 8px; border-radius: 50%; overflow: hidden; }
.rv-team-column .rc.collapsed-column .status-effect > img:not(.status-source) { width: 100%; height: 100%; }
.rv-team-column .rc.collapsed-column .status-ring, .rv-team-column .rc.collapsed-column .status-effect small,
.rv-team-column .rc.collapsed-column .status-effect .status-source { display: none; }
.rv-team-column .rc.collapsed-column .row.ab { display: flex !important; }
.rv-team-column .rc.collapsed-column .status-overflow { min-width: 12px; height: 8px; padding: 0 2px; border: 0; font: 700 7px/8px var(--f-mono); }
`;

const replayHeroCardOverrides = `
.rc { grid-template-areas: var(--hc-grid-areas, "pt top" "pt bars" "ab ab" "it it" "st st" "dv dv"); }
.rc .row.dvs { grid-template-columns: repeat(var(--cells, 1), minmax(0, 1fr)); }
.rc .row.st { height: auto; flex-wrap: wrap; overflow: visible; white-space: normal; }
.rc .mods { min-width: 0; overflow: visible; flex-wrap: wrap; align-content: flex-start; }
.rc .status-effect { width: 22px; height: 22px; border-radius: 5px; overflow: visible; }
.rc .status-effect.buff { border-radius: 50%; }
.rc .status-effect.debuff { border-radius: 6px; }
.rc .status-effect.control { border-radius: 6px; border-color: var(--bad); box-shadow: 0 0 0 1px color-mix(in srgb, var(--bad) 45%, transparent); }
.rc .status-effect.control .status-ring, .rc .status-effect.debuff .status-ring { background: conic-gradient(var(--bad) var(--p), transparent 0); }
.rc .status-effect.buff .status-ring { background: conic-gradient(var(--muted) var(--p), transparent 0); }
.rc .status-divider { width: 1px; height: 19px; flex: none; margin: 0 1px; background: var(--line); }
.rc .status-overflow { min-width: 25px; height: 22px; flex: none; }
.team-winner { flex: none; padding: 2px 5px; border-radius: 4px; background: color-mix(in srgb, var(--good) 18%, var(--surface)); color: var(--good); font: 700 9px/1.2 var(--f-body); }
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
