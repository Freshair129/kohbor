import { useCallback, useEffect, useMemo, useState } from "react";
import {
  anyChanged,
  blockingGuards,
  draftKey,
  editsFromDrafts,
  formatValue,
  guardStatus,
  safetyReport,
  withDraft,
  type Description,
  type DraftEdit,
  type EventTrace,
  type Trace,
} from "./labModel";
import { labApplyEdits, labExport, labLastTrace, labStatus } from "./labRuntime";
import { OverlayEditor } from "./OverlayEditor";
import "./lab.css";

const POLL_MS = 1000;

/** Dev-only studio (docs/design/KOHBOR-LAB.md). Renders nothing unless the host says Lab is available. */
export function Lab() {
  const [description, setDescription] = useState<Description | null>(null);
  const [trace, setTrace] = useState<Trace | null>(null);
  const [selected, setSelected] = useState<string>("health_danger");
  const [drafts, setDrafts] = useState<Record<string, DraftEdit>>({});
  const [message, setMessage] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void labStatus().then((value) => {
      if (active) setDescription(value);
    });
    return () => {
      active = false;
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

  if (description === null) return null;
  const safety = safetyReport(description);
  const entry = description.events[selected];

  return (
    <section className="lab" aria-labelledby="lab-title">
      <header className="lab-head">
        <h2 id="lab-title">Kohbor Lab</h2>
        <span className="lab-dev">DEV</span>
        <span>
          profile: {description.source}
          {description.overrides.length > 0 ? ` (${description.overrides.length} overrides)` : ""}
        </span>
        <button
          type="button"
          onClick={() => {
            void labExport().then(setExported).catch((reason: unknown) => setMessage(String(reason)));
          }}
        >
          Export profile
        </button>
      </header>

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

      {exported !== null && (
        <textarea className="lab-export" readOnly value={exported} aria-label="Exported profile" />
      )}

      <OverlayEditor />
    </section>
  );
}
