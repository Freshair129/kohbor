import { useEffect, useRef, useState, type CSSProperties, type PointerEvent, type KeyboardEvent } from "react";
import { OBJECTIVE_COPY, type Locale } from "../copy";
import { ObjectiveAlert } from "../ObjectiveAlert";
import { layoutGet, layoutPatch, layoutReset } from "../runtime";
import {
  clampObjectiveWidget,
  LAYOUT_PRESETS,
  OBJECTIVE_CARD_BASE_HEIGHT_PX,
  OBJECTIVE_CARD_BASE_WIDTH_PX,
  OBJECTIVE_CARD_LAYOUT_DEFAULT,
  parseLayoutDocument,
  type WidgetLayout,
} from "./model";

const EDITOR_VIEWPORT = { width: 1920, height: 1080 } as const;
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

interface DragState {
  pointerId: number;
  offsetX: number;
  offsetY: number;
  layout: WidgetLayout;
}

export function LayoutEditor({ locale }: { locale: Locale }) {
  const copy = OBJECTIVE_COPY[locale];
  const preview = useRef<HTMLDivElement>(null);
  const drag = useRef<DragState | null>(null);
  const [layout, setLayout] = useState<WidgetLayout>({ ...OBJECTIVE_CARD_LAYOUT_DEFAULT });
  const [previewScale, setPreviewScale] = useState(1);
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
    const next = clampObjectiveWidget({
      ...current.layout,
      x_percent: (event.clientX - current.offsetX - rect.left) / rect.width * 100,
      y_percent: (event.clientY - current.offsetY - rect.top) / rect.height * 100,
    }, EDITOR_VIEWPORT);
    current.layout = next;
    setLayout(next);
  }

  function onPointerUp(event: PointerEvent<HTMLDivElement>) {
    const current = drag.current;
    if (!current || current.pointerId !== event.pointerId) return;
    drag.current = null;
    void persist(current.layout);
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const delta = event.shiftKey ? 5 : 1;
    const keyChanges: Record<string, Partial<WidgetLayout>> = {
      ArrowLeft: { x_percent: layout.x_percent - delta },
      ArrowRight: { x_percent: layout.x_percent + delta },
      ArrowUp: { y_percent: layout.y_percent - delta },
      ArrowDown: { y_percent: layout.y_percent + delta },
    };
    const changes = keyChanges[event.key];
    if (!changes) return;
    event.preventDefault();
    const next = clampObjectiveWidget({ ...layout, ...changes }, EDITOR_VIEWPORT);
    setLayout(next);
    void persist(next);
  }

  async function reset() {
    setSaving(true);
    setMessage(null);
    try {
      const document = parseLayoutDocument(await layoutReset(), EDITOR_VIEWPORT);
      if (!document) throw new Error("LAYOUT_INVALID");
      setLayout(document.widgets.objective_card);
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

  return (
    <section className="layout-editor" aria-label={copy.layoutTitle}>
      <header className="layout-editor-heading">
        <div><h2>{copy.layoutTitle}</h2><p>{copy.layoutHelp}</p></div>
        <button type="button" onClick={() => void reset()} disabled={loading || saving}>{copy.layoutReset}</button>
      </header>
      <div
        className="layout-editor-preview"
        ref={preview}
        aria-label={copy.layoutPreview}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
      >
        <div className="layout-protected layout-protected-top" aria-hidden="true" />
        <div className="layout-protected layout-protected-announcement" aria-hidden="true" />
        <div className="layout-protected layout-protected-minimap" aria-hidden="true" />
        <div className="layout-protected layout-protected-bottom" aria-hidden="true" />
        <div
          className="layout-editor-card"
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
      <p className="layout-editor-position">{copy.layoutPosition}: {layout.x_percent.toFixed(2)}%, {layout.y_percent.toFixed(2)}%</p>
      {message && <p className="objective-settings-message" role="alert">{message}</p>}
    </section>
  );
}

