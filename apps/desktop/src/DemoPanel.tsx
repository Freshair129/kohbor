import { useEffect, useState } from "react";
import { demoAvailable, demoTrigger } from "./runtime";

const DEMO_ACTIONS = [
  { eventId: "multi_kill", tier: 2, label: "Multi Kill · Tier 2" },
  { eventId: "multi_kill", tier: 3, label: "Multi Kill · Tier 3" },
  { eventId: "first_blood", tier: null, label: "First Blood" },
  { eventId: "hero_kill", tier: null, label: "Hero Kill" },
  { eventId: "match_started", tier: null, label: "Match Started" },
  { eventId: "health_danger", tier: null, label: "Health Danger" },
] as const;

export function DemoPanel() {
  const [available, setAvailable] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void demoAvailable().then((value) => {
      if (active) setAvailable(value);
    });
    return () => { active = false; };
  }, []);

  if (!available) return null;

  return (
    <section className="demo-panel" aria-labelledby="demo-panel-title">
      <div className="card-head">
        <h2 id="demo-panel-title">DEMO</h2>
      </div>
      <p>DEMO: proves only the overlay and audio path, not event detection from a real match</p>
      <div className="demo-panel-actions">
        {DEMO_ACTIONS.map(({ eventId, tier, label }) => {
          const key = `${eventId}:${tier ?? "none"}`;
          return (
            <button
              key={key}
              type="button"
              disabled={pending !== null}
              onClick={() => {
                setPending(key);
                setError(null);
                void demoTrigger(eventId, tier).catch((reason: unknown) => {
                  setError(String(reason));
                }).finally(() => setPending(null));
              }}
            >
              {pending === key ? "กำลังส่ง DEMO…" : label}
            </button>
          );
        })}
      </div>
      {error && <p className="demo-panel-error" role="status">{error}</p>}
    </section>
  );
}
