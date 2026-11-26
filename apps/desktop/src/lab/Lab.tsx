import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  anyChanged,
  blockingGuards,
  draftKey,
  editsFromDrafts,
  formatValue,
  guardStatus,
  profileLabel,
  safetyReport,
  withDraft,
  type Description,
  type DraftEdit,
  type EventTrace,
  type Trace,
} from "./labModel";
import { labApplyEdits, labExport, labLastTrace, labStatus, stopLabHudMockEvents } from "./labRuntime";
import { OverlayEditor } from "./OverlayEditor";
import { HudLayoutEditor } from "./HudLayoutEditor";
import { MinimapRecorderPanel } from "./MinimapRecorderPanel";
import { LaneReviewPage } from "../analytics/LaneReviewPage";
import type { Locale } from "../copy";
import "./lab.css";

const POLL_MS = 1000;

type ProfileExport =
  | { status: "exporting" }
  | { status: "exported"; profile: string }
  | { status: "failed"; error: string };

function LabStudio({
  onClose,
  hudUnsaved,
  onHudUnsaved,
  closePending,
  onConfirmClose,
  onCancelClose,
}: {
  onClose: () => void;
  hudUnsaved: boolean;
  onHudUnsaved: (unsaved: boolean) => void;
  closePending: boolean;
  onConfirmClose: () => void;
  onCancelClose: () => void;
}) {
  // The HUD editor stays mounted once visited so switching tabs never drops its unsaved draft.
  const [hudVisited, setHudVisited] = useState(false);
  const [description, setDescription] = useState<Description | null>(null);
  const [trace, setTrace] = useState<Trace | null>(null);
  const [selected, setSelected] = useState<string>("health_danger");
  const [drafts, setDrafts] = useState<Record<string, DraftEdit>>({});
  const [message, setMessage] = useState<string | null>(null);
  const [profileExport, setProfileExport] = useState<ProfileExport | null>(null);
  const exportResult = useRef<HTMLDivElement>(null);
  const [tab, setTab] = useState<"events" | "overlay" | "hud" | "coach">("events");

  useEffect(() => {
    let active = true;
    void labStatus().then((value) => {
      if (active) setDescription(value);
    });
    return () => {
      active = false;
      stopLabHudMockEvents();
    };
  }, []);

  const available = description !== null;
  useEffect(() => {
    if (!available) return;
    let active = true;
    const poll = () => {
      void labLastTrace()
        .then((value) => {
          if (active) setTrace(value);
        })
        .catch(() => {});
    };
    poll();
    const timer = window.setInterval(poll, POLL_MS);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [available]);

  const row: EventTrace | undefined = useMemo(
    () => trace?.events.find((event) => event.id === selected),
    [trace, selected],
  );

  const apply = useCallback(() => {
    const edits = editsFromDrafts(drafts);
    if (edits.length === 0) return;
    setMessage(null);
    void labApplyEdits(edits)
      .then((next) => {
        setDescription(next);
        setDrafts({});
        setMessage("Saved to the dev override file. Applies from the next session.");
      })
      .catch((reason: unknown) => setMessage(String(reason)));
  }, [drafts]);

  useEffect(() => {
    exportResult.current?.scrollIntoView({ block: "nearest" });
  }, [profileExport]);

  if (description === null) return null;
  const safety = safetyReport(description);
  const entry = description.events[selected];

  return (
    <section className="lab-panel" aria-labelledby="lab-title">
      <header className="lab-head">
        <h2 id="lab-title">Kohbor Lab</h2>
        <span className="lab-dev">DEV</span>
        <span>{profileLabel(description)}</span>
        <div className="lab-tabs" role="tablist" aria-label="Lab sections">
          <button type="button" role="tab" aria-selected={tab === "events"} onClick={() => setTab("events")}>
            Events
          </button>
          <button type="button" role="tab" aria-selected={tab === "overlay"} onClick={() => setTab("overlay")}>
            Overlay editor
          </button>
          <button type="button" role="tab" aria-selected={tab === "hud"} onClick={() => { setTab("hud"); setHudVisited(true); }}>
            HUD layout{hudUnsaved ? " •" : ""}
          </button>
          <button type="button" role="tab" aria-selected={tab === "coach"} onClick={() => setTab("coach")}>
            ทบทวนโค้ช
          </button>
        </div>
        <button
          type="button"
          disabled={profileExport?.status === "exporting"}
          aria-busy={profileExport?.status === "exporting"}
          onClick={() => {
            setProfileExport({ status: "exporting" });
            void labExport()
              .then((profile) => setProfileExport({ status: "exported", profile }))
              .catch((reason: unknown) => setProfileExport({ status: "failed", error: String(reason) }));
          }}
        >
          Export profile
        </button>
        <button type="button" onClick={onClose}>
          Close
        </button>
      </header>

      <div className="lab-scroll" tabIndex={0} aria-label="Kohbor Lab content">
      <MinimapRecorderPanel />
      {profileExport !== null && profileExport.status !== "exporting" && (
        <div ref={exportResult}>
          {profileExport.status === "exported" ? (
            <textarea className="lab-export" readOnly value={profileExport.profile} aria-label="Exported profile" />
          ) : (
            <p className="lab-warn" role="alert">Could not export profile. {profileExport.error}</p>
          )}
        </div>
      )}
      {description.rejected && (
        <p className="lab-warn" role="status">
          Override rejected ({description.rejected.code}): {description.rejected.detail}. The embedded default is in force.
        </p>
      )}

      <ul className="lab-safety" aria-label="Safety readiness">
        {safety.map((item) => (
          <li key={item.event} className={item.status === "overridden" ? "lab-bad" : "lab-ok"}>
            {item.event}: {item.status === "default" ? "default parameters" : `overridden: ${item.changed.join(", ")}`}
          </li>
        ))}
      </ul>

      {tab === "events" ? (
        <>
      <div className="lab-body">
        <nav className="lab-events" aria-label="Events">
          {Object.entries(description.events).map(([id, eventDescription]) => {
            const fired = trace?.events.find((event) => event.id === id)?.fired === true;
            return (
              <button
                key={id}
                type="button"
                aria-pressed={id === selected}
                className={fired ? "lab-fired" : undefined}
                onClick={() => setSelected(id)}
              >
                {id}
                {anyChanged(eventDescription) ? " *" : ""}
              </button>
            );
          })}
        </nav>

        <div className="lab-detail">
          <h3>{selected}</h3>
          {trace === null ? (
            <p>No tick explained yet. Waiting for admitted game state.</p>
          ) : (
            <>
              <p>
                Last tick {trace.atMs} ms: {trace.status}
              </p>
              {row && row.guards.length > 0 ? (
                <table className="lab-guards">
                  <thead>
                    <tr>
                      <th>Guard</th>
                      <th>Now</th>
                      <th>Needs</th>
                      <th>Result</th>
                    </tr>
                  </thead>
                  <tbody>
                    {row.guards.map((guard) => (
                      <tr key={guard.name} className={`lab-${guardStatus(guard.pass)}`}>
                        <td>{guard.name}</td>
                        <td>{formatValue(guard.lhs, guard.unit)}</td>
                        <td>
                          {guard.op} {formatValue(guard.rhs, guard.unit)}
                        </td>
                        <td>{guardStatus(guard.pass)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <p>{row?.note ?? "No guard rows for this event."}</p>
              )}
              {row && !row.fired && blockingGuards(row).length > 0 && (
                <p>Blocked by: {blockingGuards(row).map((guard) => guard.name).join(", ")}</p>
              )}
              {row?.fired && <p>Fired on this tick.</p>}
            </>
          )}

          {entry && Object.keys(entry.params).length > 0 && (
            <fieldset className="lab-params">
              <legend>Parameters {entry.critical ? "(safety event: bounded, cannot be disabled)" : ""}</legend>
              {Object.entries(entry.params).map(([name, param]) => {
                const key = draftKey(selected, name);
                const current = typeof param.value === "number" ? param.value : null;
                return (
                  <label key={name}>
                    {name} ({param.unit}) default {String(param.default)}, allowed {param.min}..{param.max}
                    {current !== null ? (
                      <input
                        type="number"
                        min={param.min}
                        max={param.max}
                        value={drafts[key]?.value ?? current}
                        onChange={(event) => {
                          const next = Number(event.currentTarget.value);
                          if (Number.isInteger(next)) {
                            setDrafts(withDraft(drafts, { event: selected, name, value: next }, current));
                          }
                        }}
                      />
                    ) : (
                      <code>{JSON.stringify(param.value)} (edit via the profile file)</code>
                    )}
                  </label>
                );
              })}
              <button type="button" disabled={Object.keys(drafts).length === 0} onClick={apply}>
                Apply edits
              </button>
            </fieldset>
          )}
          {message && <p role="status">{message}</p>}
        </div>
      </div>

      {closePending && (
        <div className="lab-warn" role="alertdialog" aria-label="ยืนยันการปิด Lab">
          <p>HUD layout มีการเปลี่ยนแปลงที่ยังไม่บันทึก การปิด Lab จะทิ้งการแก้ไขเหล่านี้</p>
          <button type="button" onClick={onConfirmClose}>ปิดและทิ้ง</button>
          <button type="button" onClick={onCancelClose}>กลับไปแก้</button>
        </div>
      )}
        </>
      ) : tab === "overlay" ? (
        <OverlayEditor />
      ) : tab === "coach" ? (
        <LaneReviewPage locale={(document.querySelector<HTMLElement>(".app")?.lang === "th" ? "th" : "en") as Locale} />
      ) : null}
      {(hudVisited || tab === "hud") && (
        <div hidden={tab !== "hud"}>
          <HudLayoutEditor onUnsavedChange={onHudUnsaved} />
        </div>
      )}
      </div>
    </section>
  );
}

/**
 * Dev-only studio (docs/design/KOHBOR-LAB.md). Renders nothing unless the host says Lab is
 * available. It is a fixed full-window panel with its own scroll container, opened from a small
 * launcher, so it does not depend on (or change) the shell's no-page-scroll layout.
 */
export function Lab() {
  const [available, setAvailable] = useState(false);
  const [open, setOpen] = useState(false);
  const [hudUnsaved, setHudUnsaved] = useState(false);
  const [closePending, setClosePending] = useState(false);
  const requestClose = () => {
    if (hudUnsaved) setClosePending(true);
    else setOpen(false);
  };

  useEffect(() => {
    let active = true;
    void labStatus().then((value) => {
      if (active) setAvailable(value !== null);
    });
    return () => {
      active = false;
      stopLabHudMockEvents();
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && document.fullscreenElement === null) {
        if (hudUnsaved) setClosePending(true);
        else setOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, hudUnsaved]);

  if (!available) return null;
  return open ? (
    <div className="lab-overlay-root">
      <LabStudio
        onClose={requestClose}
        hudUnsaved={hudUnsaved}
        onHudUnsaved={setHudUnsaved}
        closePending={closePending}
        onConfirmClose={() => { setClosePending(false); setHudUnsaved(false); setOpen(false); }}
        onCancelClose={() => setClosePending(false)}
      />
    </div>
  ) : (
    <button type="button" className="lab-launcher" onClick={() => setOpen(true)}>
      Kohbor Lab (DEV)
    </button>
  );
}
