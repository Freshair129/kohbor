import { useEffect, useState } from "react";
import { OBJECTIVE_COPY, type Locale } from "./copy";
import { objectivePrefsGet, objectivePrefsPatch } from "./runtime";
import { parseObjectivePrefs, type ObjectivePrefKey, type ObjectivePrefs } from "./objectivePrefs";

const OBJECTIVE_KEYS: readonly Exclude<ObjectivePrefKey, "diagnostic_map_keys">[] = [
  "water_rune", "power_rune", "bounty_rune", "wisdom_rune", "tormentor",
];

export function ObjectiveSettings({ locale }: { locale: Locale }) {
  const copy = OBJECTIVE_COPY[locale];
  const [prefs, setPrefs] = useState<ObjectivePrefs | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void objectivePrefsGet()
      .then((value) => {
        if (!active) return;
        const parsed = parseObjectivePrefs(value);
        if (parsed) setPrefs(parsed);
        else setError(copy.loadFailed);
      })
      .catch(() => { if (active) setError(copy.loadFailed); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [copy.loadFailed]);

  async function change(key: ObjectivePrefKey, checked: boolean) {
    if (!prefs || saving) return;
    const previous = prefs;
    setSaving(true);
    setError(null);
    try {
      const updated = parseObjectivePrefs(await objectivePrefsPatch({ [key]: checked }));
      if (!updated) throw new Error("OBJECTIVE_PREFS_INVALID");
      setPrefs(updated);
    } catch {
      setPrefs(previous);
      setError(copy.saveFailed);
    } finally {
      setSaving(false);
    }
  }

  if (loading) return <p className="objective-settings-message" role="status">{copy.loading}</p>;

  return (
    <section className="objective-settings" aria-label={copy.title}>
      <h2>{copy.title}</h2>
      {prefs ? (
        <>
          <div className="objective-settings-list">
            {OBJECTIVE_KEYS.map((key) => (
              <label className="objective-setting-toggle" key={key}>
                <input type="checkbox" checked={prefs[key]} disabled={saving}
                  onChange={(event) => void change(key, event.currentTarget.checked)} />
                <span>{copy[key]}</span>
              </label>
            ))}
          </div>
          <label className="objective-setting-toggle objective-diagnostic-toggle">
            <input type="checkbox" checked={prefs.diagnostic_map_keys} disabled={saving}
              onChange={(event) => void change("diagnostic_map_keys", event.currentTarget.checked)} />
            <span>{copy.diagnosticMapKeys}<small>{copy.diagnosticHelp}</small></span>
          </label>
        </>
      ) : null}
      {error && <p className="objective-settings-message" role="alert">{error}</p>}
    </section>
  );
}
