import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { demoAvailable, demoTrigger } from "../runtime";
import {
  clampLayout,
  dragLayout,
  nudgeLayout,
  overlappingZones,
  sameLayout,
  type BannerLayout,
  type LayoutView,
  type Preview,
} from "./labLayout";
import {
  labLayoutApply,
  labLayoutExport,
  labLayoutGet,
  labLayoutPreview,
  labReferenceImage,
} from "./labRuntime";

/** The editor draws a 16:9 monitor; the host computes the real placement for it. */
const MONITOR = { w: 1920, h: 1080 };

interface DragState {
  pointerId: number;
  startX: number;
  startY: number;
  start: BannerLayout;
  width: number;
  height: number;
}

const pct = (value: number, total: number) => `${(value / total) * 100}%`;

/** Dev-only banner placement editor (docs/design/KOHBOR-LAB.md, stage L3). */
export function OverlayEditor() {
  const [view, setView] = useState<LayoutView | null>(null);
  const [draft, setDraft] = useState<BannerLayout | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [reference, setReference] = useState<string | null>(null);
  const [showZones, setShowZones] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);
  const [canTest, setCanTest] = useState(false);
  const canvas = useRef<HTMLDivElement | null>(null);
  const drag = useRef<DragState | null>(null);
  const previewSeq = useRef(0);

  useEffect(() => {
    let active = true;
    void labLayoutGet()
      .then((next) => {
        if (!active) return;
        setView(next);
        setDraft(next.banner);
      })
      .catch((reason: unknown) => active && setMessage(String(reason)));
    void labReferenceImage()
      .then((image) => active && setReference(image))
      .catch(() => active && setReference(null));
    void demoAvailable().then((value) => active && setCanTest(value));
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (draft === null) return;
    const seq = ++previewSeq.current;
    void labLayoutPreview(draft, MONITOR.w, MONITOR.h)
      .then((next) => {
        if (seq === previewSeq.current) setPreview(next);
      })
      .catch((reason: unknown) => {
        if (seq === previewSeq.current) setMessage(String(reason));
      });
  }, [draft]);

  const update = useCallback(
    (next: BannerLayout) => {
      if (view !== null) setDraft(clampLayout(next, view.bounds));
    },
    [view],
  );

  if (view === null || draft === null) return message ? <p role="status">{message}</p> : null;

  const stage = preview?.stage ?? null;
  const windowRect = preview?.window ?? null;
  const overlaps = stage ? overlappingZones(stage, MONITOR, view.hudZones) : [];
  const unsaved = !sameLayout(draft, view.banner);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const box = canvas.current?.getBoundingClientRect();
    if (!box) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      start: draft,
      width: box.width,
      height: box.height,
    };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const state = drag.current;
    if (!state || state.pointerId !== event.pointerId) return;
    setDraft(
      dragLayout(state.start, event.clientX - state.startX, event.clientY - state.startY, state.width, state.height, view.bounds),
    );
  };
  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current?.pointerId === event.pointerId) drag.current = null;
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const next = nudgeLayout(draft, event.key, event.shiftKey, view.bounds);
    if (next === null) return;
    event.preventDefault();
    setDraft(next);
  };

  const numberField = (label: string, key: "x_percent" | "y_percent" | "scale", step: number) => (
    <label>
      {label} ({view.bounds[key][0]}..{view.bounds[key][1]})
      <input
        type="number"
        step={step}
        min={view.bounds[key][0]}
        max={view.bounds[key][1]}
        value={draft[key]}
        onChange={(event) => {
          const value = event.currentTarget.valueAsNumber;
          if (Number.isFinite(value)) update({ ...draft, [key]: value });
        }}
      />
    </label>
  );

  return (
    <section className="lab-overlay" aria-labelledby="lab-overlay-title">
      <h3 id="lab-overlay-title">Overlay editor (banner)</h3>
      <p>
        Placement is percent of the screen, centred on the banner art. The rectangle drawn is the one the host would
        really use (it is pushed inwards near an edge). Drag it or focus it and use the arrow keys (Shift = 1%).
      </p>
      <div className="lab-overlay-layout">
        <div className="lab-canvas-wrap">
          <div
            ref={canvas}
            className={`lab-canvas${reference ? " lab-canvas-ref" : ""}`}
            style={reference ? { backgroundImage: `url(${reference})` } : undefined}
            aria-label="16:9 screen preview"
          >
            {showZones &&
              view.hudZones.map((zone) => (
                <div
                  key={zone.name}
                  className="lab-zone"
                  title={zone.name}
                  style={{
                    left: `${zone.x0 * 100}%`,
                    top: `${zone.y0 * 100}%`,
                    width: `${(zone.x1 - zone.x0) * 100}%`,
                    height: `${(zone.y1 - zone.y0) * 100}%`,
                  }}
                >
                  <span>{zone.name}</span>
                </div>
              ))}
            {windowRect && stage && !preview?.disabled && (
              <div
                className="lab-window"
                style={{
                  left: pct(windowRect.x, MONITOR.w),
                  top: pct(windowRect.y, MONITOR.h),
                  width: pct(windowRect.w, MONITOR.w),
                  height: pct(windowRect.h, MONITOR.h),
                }}
              >
                <div
                  className={`lab-stage${overlaps.length > 0 ? " lab-stage-overlap" : ""}`}
                  role="slider"
                  tabIndex={0}
                  aria-label="Banner position"
                  aria-valuetext={`x ${draft.x_percent}%, y ${draft.y_percent}%`}
                  style={{ height: pct(stage.h, windowRect.h) }}
                  onPointerDown={onPointerDown}
                  onPointerMove={onPointerMove}
                  onPointerUp={onPointerUp}
                  onPointerCancel={onPointerUp}
                  onKeyDown={onKeyDown}
                >
                  Banner
                </div>
                <div className="lab-strip">status strip</div>
              </div>
            )}
          </div>
          {!reference && (
            <p className="lab-note">
              No reference image. Optional: copy a Dota screenshot (JPEG, 8 MiB max) to
              {" <app data dir>/lab/assets/dota-overlay-reference.jpg"}; it is never part of the repository or installer.
            </p>
          )}
        </div>

        <div className="lab-controls">
          {numberField("x %", "x_percent", 0.1)}
          {numberField("y %", "y_percent", 0.1)}
          {numberField("scale", "scale", 0.01)}
          <label>
            <input
              type="checkbox"
              checked={draft.enabled}
              onChange={(event) => update({ ...draft, enabled: event.currentTarget.checked })}
            />
            enabled
          </label>
          <label>
            <input type="checkbox" checked={showZones} onChange={(event) => setShowZones(event.currentTarget.checked)} />
            HUD zones
          </label>
          {preview?.disabled && <p className="lab-warn">Disabled: the overlay window stays hidden.</p>}
          {overlaps.length > 0 && <p className="lab-warn">Overlaps protected HUD: {overlaps.join(", ")}</p>}
          {unsaved && <p>Unsaved changes.</p>}
          <div className="lab-buttons">
            <button
              type="button"
              disabled={!unsaved}
              onClick={() => {
                setMessage(null);
                void labLayoutApply(draft)
                  .then((next) => {
                    setView(next);
                    setDraft(next.banner);
                    setMessage(`Saved. Applies ${next.applies}.`);
                  })
                  .catch((reason: unknown) => setMessage(String(reason)));
              }}
            >
              Save
            </button>
            <button type="button" disabled={!unsaved} onClick={() => setDraft(view.banner)}>
              Revert
            </button>
            <button type="button" onClick={() => setDraft(view.default)}>
              Reset to default
            </button>
            <button
              type="button"
              disabled={!canTest}
              title={canTest ? "Fires the dev demo trigger so the real overlay window shows" : "Needs KOHBOR_DEMO=1"}
              onClick={() => {
                void demoTrigger("hero_kill", null).catch((reason: unknown) => setMessage(String(reason)));
              }}
            >
              Test banner
            </button>
            <button
              type="button"
              onClick={() => {
                void labLayoutExport().then(setExported).catch((reason: unknown) => setMessage(String(reason)));
              }}
            >
              Export layout
            </button>
            <button
              type="button"
              onClick={() => {
                void canvas.current?.parentElement?.requestFullscreen?.().catch(() => {});
              }}
            >
              Fullscreen
            </button>
          </div>
          {message && <p role="status">{message}</p>}
        </div>
      </div>
      {exported !== null && <textarea className="lab-export" readOnly value={exported} aria-label="Exported layout" />}
    </section>
  );
}
