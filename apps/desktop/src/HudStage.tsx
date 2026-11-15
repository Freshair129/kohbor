import { useEffect, useState, type ComponentType, type CSSProperties } from "react";
import { startLiveCollection, useRisk } from "./live";
import type { RiskView } from "./live/types";
import {
  formatChip,
  HUD_DEFAULT_LAYOUT,
  hudCopy,
  meterModel,
  parseHudState,
  renderWhenHudVisible,
  unknownHudState,
  type ChipKind,
  type HudLayoutItem,
  type HudState,
} from "./hudModel";
import { subscribeHudState } from "./hudRuntime";
import "./hud.css";

const HUD_WINDOW_HEIGHT_SCREEN_PERCENT = 9;
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

function modulePosition(item: HudLayoutItem): CSSProperties {
  return {
    left: `${item.x_percent}vw`,
    // The native strip is 9% of the screen high and starts at y=0.
    top: `${(item.y_percent / HUD_WINDOW_HEIGHT_SCREEN_PERCENT) * 100}vh`,
    "--hud-scale": item.scale,
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
    <div className="hud-chip" data-stale={display.stale}>
      <span className="hud-chip-label">{visibleLabel(kind)}</span>
      <span className="hud-chip-value">{display.text}</span>
      {display.stale ? <span className="hud-stale-flag">{hudCopy.staleFlag}</span> : null}
    </div>
  );
}

export function HudStage({
  layout = HUD_DEFAULT_LAYOUT,
  MockHarness,
}: {
  layout?: readonly HudLayoutItem[];
  MockHarness?: ComponentType;
}) {
  const [state, setState] = useState<HudState>(() => unknownHudState(false));
  const risk = useRisk();

  useEffect(() => {
    void startLiveCollection().catch(() => {});
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
    <div className="hud-stage" role="status" aria-live="off" aria-label={hudCopy.stripLabel} lang="th">
      {layout.filter((item) => item.enabled).map((item) => (
        <div className={`hud-module hud-module-${item.id}`} key={item.id} style={modulePosition(item)}>
          {item.id === "gmeter"
            ? <GMeter view={risk} />
            : <StatChip state={state} kind={item.id === "kda" ? "kda" : item.id} />}
        </div>
      ))}
    </div>
  ));

  // The hidden harness keeps publishing mock payloads while the stage itself stays mounted.
  return <>{MockHarness ? <MockHarness /> : null}{content}</>;
}

export { CHIP_KINDS };
