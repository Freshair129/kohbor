import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import {
  changeHudLayoutDraft,
  computeHudStripHeightPercent,
  confirmHudLayout,
  createHudLayoutEditorState,
  failHudLayoutAction,
  hudLayoutHasUnsavedChanges,
  commitHudNumber,
  hudModuleBox,
  modulesOutsideHudStrip,
  nudgeHudModule,
  overlappingHudModules,
  updateHudModule,
  dragHudModule,
  HUD_REFERENCE_SCREEN,
  type HudLayoutEditorState,
  type LabHudModule,
} from "./labHudLayout";
import { labHudLayoutApply, labHudLayoutGet, labHudLayoutReset, labHudPreview, labReferenceImage } from "./labRuntime";
import type { HudModuleId } from "../hudModel";

const MODULE_LABELS: Record<HudModuleId, string> = {
  gmeter: "G-Meter",
  kda: "K/D/A",
  gold: "Gold",
  gpm: "GPM",
  nw: "Net Worth",
  xpm: "XPM",
};

interface DragState {
  pointerId: number;
  id: HudModuleId;
  startX: number;
  startY: number;
  width: number;
  height: number;
  modules: readonly LabHudModule[];
}

interface NumberFieldProps {
  label: string;
  bounds: readonly [number, number];
  step: number;
  value: number;
  onCommit: (value: number) => void;
}

/** Keeps the typed text locally; the value is clamped and committed on blur / Enter, never per keystroke. */
function HudNumberField({ label, bounds, step, value, onCommit }: NumberFieldProps) {
  const [text, setText] = useState<string | null>(null);
  const commit = () => {
    if (text === null) return;
    const next = commitHudNumber(text, bounds);
    if (next !== null) onCommit(next);
    setText(null);
  };
  return (
    <label className="lab-hud-field">
      {label} ({bounds[0]}–{bounds[1]})
      <input
        type="number"
        min={bounds[0]}
        max={bounds[1]}
        step={step}
        value={text ?? String(value)}
        onChange={(event) => setText(event.currentTarget.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") commit();
          if (event.key === "Escape") setText(null);
        }}
      />
    </label>
  );
}

export function HudLayoutEditor({ onUnsavedChange }: { onUnsavedChange?: (unsaved: boolean) => void } = {}) {
  const [editor, setEditor] = useState<HudLayoutEditorState | null>(null);
  const [selected, setSelected] = useState<HudModuleId>("gmeter");
  const [reference, setReference] = useState<string | null>(null);
  const [previewRequested, setPreviewRequested] = useState(false);
  const [loadingError, setLoadingError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const canvas = useRef<HTMLDivElement | null>(null);
  const canvasWrap = useRef<HTMLDivElement | null>(null);
  const drag = useRef<DragState | null>(null);

  useEffect(() => {
    let active = true;
    void labHudLayoutGet()
      .then((reply) => {
        if (active) {
          setEditor(createHudLayoutEditorState(reply));
          setLoadingError(null);
        }
      })
      .catch((reason: unknown) => active && setLoadingError(String(reason)));
    void labReferenceImage().then((image) => active && setReference(image)).catch(() => active && setReference(null));
    return () => {
      active = false;
    };
  }, []);

  const unsavedNow = editor !== null && hudLayoutHasUnsavedChanges(editor);
  useEffect(() => {
    onUnsavedChange?.(unsavedNow);
  }, [unsavedNow, onUnsavedChange]);

  if (editor === null) {
    return <section className="lab-hud-layout" aria-labelledby="lab-hud-layout-title">
      <h3 id="lab-hud-layout-title">HUD layout</h3>
      {loadingError ? <p className="lab-warn" role="alert">โหลดตำแหน่ง HUD ไม่สำเร็จ: {loadingError}</p> : <p role="status">กำลังโหลดตำแหน่ง HUD…</p>}
    </section>;
  }

  const selectedModule = editor.draft.find((module) => module.id === selected) ?? editor.draft[0]!;
  const stripHeight = computeHudStripHeightPercent(editor.draft);
  const overlaps = overlappingHudModules(editor.draft);
  const outsideStrip = modulesOutsideHudStrip(editor.draft, stripHeight);
  const unsaved = hudLayoutHasUnsavedChanges(editor);

  const updateSelected = (update: Partial<LabHudModule>) => {
    setEditor((state) => state === null ? state : changeHudLayoutDraft(
      state,
      updateHudModule(state.draft, selected, update, state.confirmed.bounds),
    ));
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>, id: HudModuleId) => {
    const box = canvas.current?.getBoundingClientRect();
    if (!box || editor === null) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    setSelected(id);
    drag.current = {
      pointerId: event.pointerId,
      id,
      startX: event.clientX,
      startY: event.clientY,
      width: box.width,
      height: box.height,
      modules: editor.draft,
    };
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const state = drag.current;
    if (!state || state.pointerId !== event.pointerId) return;
    setEditor((current) => current === null ? current : changeHudLayoutDraft(
      current,
      dragHudModule(
        state.modules,
        state.id,
        event.clientX - state.startX,
        event.clientY - state.startY,
        state.width,
        state.height,
        current.confirmed.bounds,
      ),
    ));
  };

  const onPointerUp = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current?.pointerId === event.pointerId) drag.current = null;
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>, id: HudModuleId) => {
    const moved = nudgeHudModule(editor.draft, id, event.key, event.shiftKey, editor.confirmed.bounds);
    if (moved === null) return;
    event.preventDefault();
    setSelected(id);
    setEditor((state) => state === null ? state : changeHudLayoutDraft(state, moved));
  };

  const numberField = (label: string, key: "x_percent" | "y_percent" | "scale", step: number) => (
    <HudNumberField
      key={`${selectedModule.id}-${key}`}
      label={label}
      bounds={editor.confirmed.bounds[key]}
      step={step}
      value={selectedModule[key]}
      onCommit={(value) => updateSelected({ [key]: value })}
    />
  );

  const runAction = <T,>(action: () => Promise<T>, onSuccess: (value: T) => void) => {
    if (busy) return;
    setBusy(true);
    setEditor((state) => state === null ? state : failHudLayoutAction(state, ""));
    void action()
      .then(onSuccess)
      .catch((reason: unknown) => {
        setEditor((state) => state === null ? state : failHudLayoutAction(state, String(reason)));
      })
      .finally(() => setBusy(false));
  };

  const sendSave = () => {
    const sent = editor.draft.map((module) => ({ ...module }));
    runAction(
      () => labHudLayoutApply(sent),
      (reply) => setEditor((state) => state === null ? state : confirmHudLayout(state, reply, sent)),
    );
  };

  const sendReset = () => {
    setConfirmReset(false);
    runAction(
      () => labHudLayoutReset(),
      (reply) => setEditor((state) => state === null ? state : confirmHudLayout(state, reply)),
    );
  };

  const toggleFullscreen = () => {
    const target = canvasWrap.current;
    if (!target) return;
    const task = document.fullscreenElement === target ? document.exitFullscreen() : target.requestFullscreen();
    void task.catch((reason: unknown) => {
      setEditor((state) => state === null ? state : failHudLayoutAction(state, String(reason)));
    });
  };

  const referenceStyle = reference ? { backgroundImage: `url(${reference})` } : undefined;
  return (
    <section className="lab-hud-layout" aria-labelledby="lab-hud-layout-title">
      <div className="lab-hud-heading">
        <h3 id="lab-hud-layout-title">HUD layout</h3>
        <span>แหล่งข้อมูล: {editor.confirmed.source === "embedded" ? "ค่าเริ่มต้น" : "ไฟล์ปรับแต่ง"}</span>
        {unsaved && <strong role="status">มีการเปลี่ยนแปลงที่ยังไม่บันทึก</strong>}
      </div>

      <div className="lab-hud-editor-grid">
        <div className="lab-canvas-wrap" ref={canvasWrap}>
          <div
            className={`lab-canvas lab-hud-canvas${reference ? " lab-canvas-ref" : ""}`}
            ref={canvas}
            style={referenceStyle}
            aria-label="ตัวอย่างหน้าจอ 16:9"
          >
            <div className="lab-hud-strip" style={{ height: `${stripHeight}%` }} aria-label={`แถบ HUD ${stripHeight.toFixed(1)} เปอร์เซ็นต์`}>
              <span>HUD STRIP · {stripHeight.toFixed(1)}%</span>
            </div>
            {editor.draft.map((module) => {
              const box = hudModuleBox(module);
              const isSelected = selected === module.id;
              return (
                <div
                  key={module.id}
                  className={`lab-hud-module${isSelected ? " is-selected" : ""}${module.enabled ? "" : " is-disabled"}${outsideStrip.includes(module.id) ? " is-outside-strip" : ""}`}
                  style={{
                    left: `${module.x_percent}%`,
                    top: `${module.y_percent}%`,
                    width: `${(box.width / HUD_REFERENCE_SCREEN.width) * 100}%`,
                    height: `${(box.height / HUD_REFERENCE_SCREEN.height) * 100}%`,
                  }}
                  role="slider"
                  tabIndex={0}
                  aria-label={`${MODULE_LABELS[module.id]} position`}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={Math.round(module.x_percent * 100) / 100}
                  aria-valuetext={`x ${module.x_percent}%, y ${module.y_percent}%, scale ${module.scale}`}
                  onPointerDown={(event) => onPointerDown(event, module.id)}
                  onPointerMove={onPointerMove}
                  onPointerUp={onPointerUp}
                  onPointerCancel={onPointerUp}
                  onKeyDown={(event) => onKeyDown(event, module.id)}
                  onFocus={() => setSelected(module.id)}
                >
                  {MODULE_LABELS[module.id]}
                </div>
              );
            })}
          </div>
          {!reference && <p className="lab-note">ไม่พบภาพอ้างอิง ใช้กริดเปล่าแทนได้ หรือวาง dota-overlay-reference.jpg ไว้ในพื้นที่ Lab ของแอป</p>}
          <button type="button" onClick={toggleFullscreen}>เต็มจอ</button>
        </div>

        <aside className="lab-hud-controls">
          <ul className="lab-hud-module-list" aria-label="รายการโมดูล HUD">
            {editor.draft.map((module) => (
              <li key={module.id}>
                <button type="button" aria-pressed={selected === module.id} onClick={() => setSelected(module.id)}>
                  {MODULE_LABELS[module.id]}
                </button>
                <label>
                  <input
                    type="checkbox"
                    aria-label={`เปิด/ปิดโมดูล ${MODULE_LABELS[module.id]}`}
                    checked={module.enabled}
                    onChange={(event) => setEditor((state) => state === null ? state : changeHudLayoutDraft(
                      state,
                      updateHudModule(state.draft, module.id, { enabled: event.currentTarget.checked }, state.confirmed.bounds),
                    ))}
                  />
                  เปิด/ปิดโมดูล
                </label>
              </li>
            ))}
          </ul>

          <div className="lab-hud-selected-fields">
            <h4>{MODULE_LABELS[selectedModule.id]}</h4>
            {numberField("X (%)", "x_percent", 0.1)}
            {numberField("Y (%)", "y_percent", 0.1)}
            {numberField("ขนาด", "scale", 0.01)}
            <label className="lab-hud-field">
              ขนาดโมดูล ({editor.confirmed.bounds.scale[0]}–{editor.confirmed.bounds.scale[1]})
              <input
                type="range"
                min={editor.confirmed.bounds.scale[0]}
                max={editor.confirmed.bounds.scale[1]}
                step={0.01}
                value={selectedModule.scale}
                onChange={(event) => { const value = event.currentTarget.valueAsNumber; if (Number.isFinite(value)) updateSelected({ scale: value }); }}
              />
            </label>
          </div>

          <p role="status">แถบ HUD สูง {stripHeight.toFixed(1)}% ตามกฎปรับความสูงของโฮสต์</p>
          {outsideStrip.length > 0 && <p className="lab-warn" role="alert">ขอบล่างของโมดูล {outsideStrip.map((id) => MODULE_LABELS[id]).join(", ")} อยู่นอกแถบ HUD</p>}
          {overlaps.length > 0 && <p className="lab-warn" role="alert">โมดูลที่เปิดอยู่ซ้อนกัน: {overlaps.map(({ first, second }) => `${MODULE_LABELS[first]} / ${MODULE_LABELS[second]}`).join(", ")}</p>}
          <p className="lab-note">ลากกรอบเพื่อย้าย หรือใช้ปุ่มลูกศร (Shift = 1%) · ตำแหน่งเป็นจุดกึ่งกลางของหน้าจอ</p>

          <div className="lab-buttons">
            <button type="button" disabled={!unsaved || busy} onClick={sendSave}>บันทึก</button>
            <button
              type="button"
              disabled={busy}
              onClick={() => { if (unsaved) setConfirmReset(true); else sendReset(); }}
            >รีเซ็ตเป็นค่าเริ่มต้น</button>
            <button
              type="button"
              disabled={busy}
              onClick={() => runAction(
                () => labHudPreview(10),
                () => setPreviewRequested(true),
              )}
            >ดู HUD จริง 10 วินาที</button>
          </div>
          {confirmReset && (
            <div className="lab-warn" role="alertdialog" aria-label="ยืนยันการรีเซ็ต">
              <p>มีการเปลี่ยนแปลงที่ยังไม่บันทึก การรีเซ็ตจะทิ้งการแก้ไขเหล่านี้</p>
              <button type="button" onClick={sendReset}>รีเซ็ตและทิ้งการแก้ไข</button>
              <button type="button" onClick={() => setConfirmReset(false)}>กลับไปแก้ต่อ</button>
            </div>
          )}
          <p>ตัวอย่าง HUD ใช้ข้อมูลสังเคราะห์ (SYNTHETIC) ไม่ได้อ่านข้อมูลเกมจริง หน้าต่าง HUD จะมีป้ายตัวอย่างสังเคราะห์ และใช้ไม่ได้ระหว่างแมตช์จริง</p>
          {previewRequested && <p role="status">ส่งคำขอแสดง HUD จริงด้วยข้อมูลสังเคราะห์ 10 วินาทีแล้ว</p>}
          {editor.error && <p className="lab-warn" role="alert">ดำเนินการกับโฮสต์ไม่สำเร็จ: {editor.error}</p>}
        </aside>
      </div>
    </section>
  );
}
