import { useEffect, useRef, useState, type CSSProperties, type PointerEvent, type KeyboardEvent } from "react";
import { OBJECTIVE_COPY, type Locale } from "../copy";
import { ObjectiveAlert } from "../ObjectiveAlert";
import { layoutGet, layoutPatch, layoutReferenceImage, layoutReset } from "../runtime";
import { exitAnyFullscreen } from "./fullscreen";
import {
  clampObjectiveWidget,
  clampObjectiveWidgetWithReasons,
  LAYOUT_PRESETS,
  LAYOUT_REFERENCE_SCREEN,
  layoutReadoutPx,
  nudgeObjectiveWidget,
  OBJECTIVE_CARD_ANNOUNCEMENT_OVERLAP_PX_AT_1080,
  OBJECTIVE_CARD_BASE_HEIGHT_PX,
  OBJECTIVE_CARD_BASE_WIDTH_PX,
  OBJECTIVE_CARD_LAYOUT_DEFAULT,
  parseLayoutDocument,
  pointerToLayout,
  referenceBackground,
  type BlockingReason,
  type WidgetLayout,
} from "./model";

const EDITOR_VIEWPORT = LAYOUT_REFERENCE_SCREEN;
const KEY_SAVE_DELAY_MS = 350;
const BLOCKED_HINT_MS = 1500;
const PREVIEW_BATCH = {
  schema_version: 1 as const,
  observed_clock_seconds: 240,
  alerts: [{
    objective: "power_rune" as const,
    phase: "pre_warn" as const,
    lead_s: 30,
    occurrence_seconds: 270,
    due_seconds: 240,
  }],
};

/** Protected regions drawn over the preview; the CSS class names are kept from the first editor. */
const PROTECTED_OVERLAYS: readonly { reason: Exclude<BlockingReason, "edge">; className: string }[] = [
  { reason: "top_bar", className: "layout-protected-top" },
  { reason: "announcement", className: "layout-protected-announcement" },
  { reason: "minimap", className: "layout-protected-minimap" },
  { reason: "bottom_hud", className: "layout-protected-bottom" },
];

interface DragState {
  pointerId: number;
  offsetX: number;
  offsetY: number;
  layout: WidgetLayout;
}

export function LayoutEditor({ locale }: { locale: Locale }) {
  const copy = OBJECTIVE_COPY[locale];
  const stage = useRef<HTMLDivElement>(null);
  const preview = useRef<HTMLDivElement>(null);
  const card = useRef<HTMLDivElement>(null);
  const drag = useRef<DragState | null>(null);
  const keySave = useRef<number | null>(null);
  const blockedTimer = useRef<number | null>(null);
  const [layout, setLayout] = useState<WidgetLayout>({ ...OBJECTIVE_CARD_LAYOUT_DEFAULT });
  const [previewScale, setPreviewScale] = useState(1);
  const [reference, setReference] = useState<string | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [blockedBy, setBlockedBy] = useState<BlockingReason[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void layoutGet().then((value) => {
      if (!active) return;
      const document = parseLayoutDocument(value, EDITOR_VIEWPORT);
      if (!document) throw new Error("LAYOUT_INVALID");
      setLayout(document.widgets.objective_card);
    }).catch(() => {
      if (active) setMessage(copy.layoutFailed);
    }).finally(() => {
      if (active) setLoading(false);
    });
    void layoutReferenceImage().then((image) => { if (active) setReference(image); });
    return () => { active = false; };
  }, [copy.layoutFailed]);

  useEffect(() => {
    const element = preview.current;
    if (!element) return;
    const update = () => setPreviewScale(element.getBoundingClientRect().width / EDITOR_VIEWPORT.width);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!fullscreen) return;
    card.current?.focus();
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") leaveFullscreen();
    };
    const onChange = () => {
      if (!document.fullscreenElement) setFullscreen(false);
    };
    window.addEventListener("keydown", onKey);
    document.addEventListener("fullscreenchange", onChange);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("fullscreenchange", onChange);
    };
  }, [fullscreen]);

  useEffect(() => () => {
    if (keySave.current !== null) window.clearTimeout(keySave.current);
    if (blockedTimer.current !== null) window.clearTimeout(blockedTimer.current);
  }, []);

  function showBlocked(reasons: BlockingReason[], keep = false) {
    if (blockedTimer.current !== null) window.clearTimeout(blockedTimer.current);
    setBlockedBy(reasons);
    if (!keep && reasons.length > 0) blockedTimer.current = window.setTimeout(() => setBlockedBy([]), BLOCKED_HINT_MS);
  }

  async function persist(next: WidgetLayout) {
    setSaving(true);
    setMessage(null);
    try {
      const value = await layoutPatch({ widget: "objective_card", ...next });
      const document = parseLayoutDocument(value, EDITOR_VIEWPORT);
      if (!document) throw new Error("LAYOUT_INVALID");
      setLayout(document.widgets.objective_card);
    } catch {
      setMessage(copy.layoutFailed);
    } finally {
      setSaving(false);
    }
  }

  function enterFullscreen() {
    setFullscreen(true);
    // The fixed full-window stage works even when the browser refuses real full-screen mode.
    const target = stage.current;
    if (target && typeof target.requestFullscreen === "function" && !document.fullscreenElement) {
      void target.requestFullscreen().catch(() => {});
    }
  }

  function leaveFullscreen() {
    setFullscreen(false);
    void exitAnyFullscreen();
  }

  function onPointerDown(event: PointerEvent<HTMLDivElement>) {
    const rect = preview.current?.getBoundingClientRect();
    if (!rect) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      pointerId: event.pointerId,
      offsetX: event.clientX - (rect.left + rect.width * layout.x_percent / 100),
      offsetY: event.clientY - (rect.top + rect.height * layout.y_percent / 100),
      layout,
    };
  }

  function onPointerMove(event: PointerEvent<HTMLDivElement>) {
    const current = drag.current;
    const rect = preview.current?.getBoundingClientRect();
    if (!current || current.pointerId !== event.pointerId || !rect) return;
    const requested = pointerToLayout(event.clientX, event.clientY, current.offsetX, current.offsetY, rect, current.layout.scale);
    const result = clampObjectiveWidgetWithReasons(requested, EDITOR_VIEWPORT);
    current.layout = result.layout;
    setLayout(result.layout);
    showBlocked(result.blockedBy, true);
  }

  function onPointerUp(event: PointerEvent<HTMLDivElement>) {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;
    drag.current = null;
    showBlocked(blockedBy);
    void persist(current.layout);
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const result = nudgeObjectiveWidget(layout, event.key, event.shiftKey, EDITOR_VIEWPORT);
    if (!result) return;
    event.preventDefault();
    setLayout(result.layout);
    showBlocked(result.blockedBy);
    if (keySave.current !== null) window.clearTimeout(keySave.current);
    const next = result.layout;
    keySave.current = window.setTimeout(() => {
      keySave.current = null;
      void persist(next);
    }, KEY_SAVE_DELAY_MS);
  }

  async function reset() {
    setSaving(true);
    setMessage(null);
    try {
      const document = parseLayoutDocument(await layoutReset(), EDITOR_VIEWPORT);
      if (!document) throw new Error("LAYOUT_INVALID");
      setLayout(document.widgets.objective_card);
      showBlocked([]);
    } catch {
      setMessage(copy.layoutFailed);
    } finally {
      setSaving(false);
    }
  }

  function changeScale(scale: number, save = false) {
    const next = clampObjectiveWidget({ ...layout, scale }, EDITOR_VIEWPORT);
    setLayout(next);
    if (save) void persist(next);
  }

  const cardStyle = {
    left: `${layout.x_percent}%`,
    top: `${layout.y_percent}%`,
    "--preview-scale": previewScale * layout.scale,
    "--card-width-percent": `${OBJECTIVE_CARD_BASE_WIDTH_PX / EDITOR_VIEWPORT.width * 100}%`,
    "--card-height-percent": `${OBJECTIVE_CARD_BASE_HEIGHT_PX / EDITOR_VIEWPORT.height * 100}%`,
  } as CSSProperties;
  const allowanceStyle = {
    "--allowance-height-percent": `${OBJECTIVE_CARD_ANNOUNCEMENT_OVERLAP_PX_AT_1080 / 1080 * 100}%`,
  } as CSSProperties;
  const readout = layoutReadoutPx(layout, EDITOR_VIEWPORT);
  const readoutText = `${copy.layoutReadout}: x ${readout.x} px · y ${readout.y} px (${layout.x_percent.toFixed(2)}%, ${layout.y_percent.toFixed(2)}%) · ${layout.scale.toFixed(2)}×`;
  const blockedText = blockedBy.length > 0
    ? `${copy.layoutBlocked}: ${blockedBy.map((reason) => copy.layoutRegions[reason]).join(", ")}`
    : null;

  const scaleControls = (
    <div className="layout-editor-controls">
      <label className="layout-editor-scale">
        <span>{copy.layoutScale} <output>{layout.scale.toFixed(2)}×</output></span>
        <input type="range" min={0.75} max={2} step={0.05} value={layout.scale}
          disabled={loading || saving}
          onChange={(event) => changeScale(Number(event.currentTarget.value))}
          onPointerUp={() => void persist(layout)}
          onBlur={() => void persist(layout)} />
      </label>
      <div className="layout-editor-presets" role="group" aria-label={copy.layoutScale}>
        {LAYOUT_PRESETS.map((scale) => (
          <button key={scale} type="button" aria-pressed={layout.scale === scale}
            disabled={loading || saving} onClick={() => changeScale(scale, true)}>{scale}×</button>
        ))}
      </div>
    </div>
  );

  return (
    <section className="layout-editor" aria-label={copy.layoutTitle}>
      <header className="layout-editor-heading">
        <div><h2>{copy.layoutTitle}</h2><p>{copy.layoutHelp}</p></div>
        <div className="layout-editor-actions">
          <button type="button" onClick={enterFullscreen} disabled={loading}>{copy.layoutFullscreen}</button>
          <button type="button" onClick={() => void reset()} disabled={loading || saving}>{copy.layoutReset}</button>
        </div>
      </header>
      <div className={`layout-editor-stage${fullscreen ? " is-fullscreen" : ""}`} ref={stage}>
        <div
          className={`layout-editor-preview${reference ? " has-reference" : ""}`}
          ref={preview}
          style={referenceBackground(reference)}
          aria-label={copy.layoutPreview}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        >
          {PROTECTED_OVERLAYS.map(({ reason, className }) => (
            <div key={reason}
              className={`layout-protected ${className}${blockedBy.includes(reason) ? " is-blocking" : ""}`}
              data-region={reason}
              aria-hidden="true" />
          ))}
          <div className="layout-allowance" style={allowanceStyle} aria-hidden="true" />
          <div className={`layout-edge${blockedBy.includes("edge") ? " is-blocking" : ""}`} aria-hidden="true" />
          <div
            className="layout-editor-card"
            ref={card}
            role="button"
            tabIndex={0}
            aria-label={copy.layoutPosition}
            style={cardStyle}
            onPointerDown={onPointerDown}
            onKeyDown={onKeyDown}
          >
            <ObjectiveAlert batch={PREVIEW_BATCH} locale={locale} visible />
          </div>
        </div>
        {fullscreen && (
          <div className="layout-editor-fullbar" role="toolbar" aria-label={copy.layoutTitle}>
            <output className="layout-editor-readout">{readoutText}</output>
            {blockedText && <span className="layout-editor-blocked" role="status">{blockedText}</span>}
            {scaleControls}
            <span className="layout-editor-keys">{copy.layoutKeysHelp}</span>
            <button type="button" onClick={leaveFullscreen}>{copy.layoutExitFullscreen}</button>
          </div>
        )}
      </div>
      {!reference && <p className="layout-editor-hint" role="note">{copy.layoutNoReference}</p>}
      {scaleControls}
      <p className="layout-editor-position">{readoutText}</p>
      {blockedText && !fullscreen && <p className="layout-editor-blocked" role="status">{blockedText}</p>}
      <p className="layout-editor-keys">{copy.layoutKeysHelp}</p>
      {message && <p className="objective-settings-message" role="alert">{message}</p>}
    </section>
  );
}
