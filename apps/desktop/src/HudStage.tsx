import { useEffect, useState, type ComponentType, type CSSProperties } from "react";
import { startLiveCollection, useRisk } from "./live";
import type { RiskView } from "./live/types";
import {
  formatChip,
  HUD_DEFAULT_LAYOUT,
  HUD_DEFAULT_LAYOUT_EVENT,
  hudCopy,
  hudModulePlacement,
  meterModel,
  applyHudLayoutEvent,
  parseHudState,
  parseHudLayoutEvent,
  renderWhenHudVisible,
  unknownHudState,
  visibleHudModules,
  type ChipKind,
  type HudLayoutEvent,
  type HudLayoutItem,
  type HudState,
} from "./hudModel";
import { subscribeHudLayout, subscribeHudState } from "./hudRuntime";
import { subscribeHudLocale, subscribeObjectiveAlerts } from "./runtime";
import {
  createObjectiveAlertState,
  objectiveAlertVisible,
  receiveObjectiveAlert,
  OBJECTIVE_ALERT_AUTO_HIDE_MS,
  type ObjectiveAlertState,
} from "./objectiveAlertModel";
import { ObjectiveAlert } from "./ObjectiveAlert";
import { HUD_LANGUAGE } from "./hudCopy";
import type { Locale } from "./copy";
import "./hud.css";

const CHIP_KINDS: readonly ChipKind[] = ["kda", "gold", "gpm", "nw", "xpm"];

function fieldFor(state: HudState, kind: ChipKind) {
  if (kind === "kda") {
    return {
      kills: state.fields.kills,
      deaths: state.fields.deaths,
      assists: state.fields.assists,
    };
  }
  return kind === "nw" ? state.net_worth : state.fields[kind];
}

function visibleLabel(kind: ChipKind): string {
  return hudCopy.chipLabels[kind];
}

function modulePosition(item: HudLayoutItem, stripHeightPercent: number): CSSProperties {
  const placement = hudModulePlacement(item, stripHeightPercent);
  return {
    left: `${placement.left_vw}vw`,
    // The module Y is screen-relative while this WebView covers only the native HUD strip.
    top: `${placement.top_vh}vh`,
    "--hud-scale": placement.scale,
  } as CSSProperties;
}

function GMeter({ view }: { view: RiskView | null }) {
  const meter = meterModel(view);
  return (
    <div className={`hud-gmeter hud-gmeter-${meter.tone}`} data-level={meter.level ?? "unknown"}>
      <span className="hud-gmeter-title">{hudCopy.meterTitle}</span>
      <span className="hud-meter-ticks" aria-hidden="true">
        {[0, 1, 2, 3].map((tick) => (
          <i
            key={tick}
            className={`hud-meter-tick${tick < meter.ticksLit ? " is-lit" : ""}`}
          />
        ))}
      </span>
      <span className="hud-gmeter-label">{meter.label}</span>
    </div>
  );
}

function StatChip({ state, kind }: { state: HudState; kind: ChipKind }) {
  const display = formatChip(kind, fieldFor(state, kind));
  return (
    <div className={`hud-chip hud-chip-${kind}`} data-stale={display.stale}>
      <div className="hud-chip-value">{display.text}</div>
      <div className="hud-chip-label">
        {visibleLabel(kind)}
        {display.stale ? <span className="hud-stale-flag">{hudCopy.staleFlag}</span> : null}
      </div>
    </div>
  );
}

/** The reference design is laid out on a 1920 px wide screen; the strip is as wide as the screen. */
function useOverlayScale(): number {
  const [scale, setScale] = useState(() => window.innerWidth / 1920);
  useEffect(() => {
    const resize = () => setScale(window.innerWidth / 1920);
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  return scale;
}

export function HudStage({
  layout = HUD_DEFAULT_LAYOUT,
  MockHarness,
}: {
  layout?: readonly HudLayoutItem[];
  MockHarness?: ComponentType;
}) {
  const [state, setState] = useState<HudState>(() => unknownHudState(false));
  const [hudLayout, setHudLayout] = useState<HudLayoutEvent>(() => ({ ...HUD_DEFAULT_LAYOUT_EVENT, modules: layout }));
  const [objectiveAlert, setObjectiveAlert] = useState<ObjectiveAlertState>(createObjectiveAlertState);
  const [locale, setLocale] = useState<Locale>(() => {
    const requested = new URLSearchParams(window.location.search).get("lang");
    return requested === "th" || requested === "en" ? requested : HUD_LANGUAGE;
  });
  const [nowMs, setNowMs] = useState(() => performance.now());
  const risk = useRisk();
  const overlayScale = useOverlayScale();

  useEffect(() => {
    void startLiveCollection().catch(() => {});
  }, []);

  useEffect(() => {
    let disposed = false;
    let stopAlert: (() => void) | undefined;
    let stopLocale: (() => void) | undefined;
    void Promise.all([
      subscribeObjectiveAlerts((payload) => {
        const at = performance.now();
        setNowMs(at);
        setObjectiveAlert((current) => receiveObjectiveAlert(current, payload, at));
      }),
      subscribeHudLocale(setLocale),
    ]).then(([unlistenAlert, unlistenLocale]) => {
      if (disposed) {
        unlistenAlert();
        unlistenLocale();
      } else {
        stopAlert = unlistenAlert;
        stopLocale = unlistenLocale;
      }
    }).catch(() => {});
    return () => {
      disposed = true;
      stopAlert?.();
      stopLocale?.();
    };
  }, []);

  useEffect(() => {
    if (objectiveAlert.shownUntilMs === null) return;
    const remaining = objectiveAlert.shownUntilMs - nowMs;
    if (remaining <= 0) return;
    const timer = window.setTimeout(() => setNowMs(performance.now()), Math.min(remaining, OBJECTIVE_ALERT_AUTO_HIDE_MS));
    return () => window.clearTimeout(timer);
  }, [objectiveAlert.shownUntilMs, nowMs]);

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void subscribeHudLayout((payload) => {
      if (parseHudLayoutEvent(payload) === null) return;
      setHudLayout((current) => applyHudLayoutEvent(current, payload));
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(() => {
        // The baked-in layout remains in use when the host event is unavailable.
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void subscribeHudState((payload) => setState(parseHudState(payload)))
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(() => {
        if (!disposed) setState(unknownHudState(false));
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  const content = renderWhenHudVisible(state.visible, () => (
    <div className="hud-stage" role="status" aria-live="off" aria-label={hudCopy.stripLabel} lang={locale} style={{ "--hud-overlay-scale": overlayScale } as CSSProperties}>
      {state.synthetic === true && <div className="hud-synthetic-badge" data-synthetic="true">{hudCopy.syntheticBadge}</div>}
      {visibleHudModules(hudLayout).map((item) => (
        <div className={`hud-module hud-module-${item.id}`} key={item.id} style={modulePosition(item, hudLayout.strip_height_percent)}>
          {item.id === "gmeter"
            ? <GMeter view={risk} />
            : <StatChip state={state} kind={item.id === "kda" ? "kda" : item.id} />}
        </div>
      ))}
      {objectiveAlertVisible(objectiveAlert, nowMs) ? (
        <div className="hud-module hud-objective-module" style={{
          left: `${hudLayout.objective_card.x_percent}vw`,
          top: `${(hudLayout.objective_card.y_percent / hudLayout.strip_height_percent) * 100}vh`,
          "--hud-scale": hudLayout.objective_card.scale,
        } as CSSProperties}>
          <ObjectiveAlert batch={objectiveAlert.batch} locale={objectiveAlert.batch?.locale ?? locale} visible />
        </div>
      ) : null}
    </div>
  ));

  // The hidden harness keeps publishing mock payloads while the stage itself stays mounted.
  return <>{MockHarness ? <MockHarness /> : null}{content}</>;
}

export { CHIP_KINDS };
