import { useEffect, useState } from "react";
import { OBJECTIVE_COPY, type Locale } from "./copy";
import { objectivePrefsGet, objectivePrefsPatch } from "./runtime";
import {
  OBJECTIVE_PREF_KEYS,
  parseObjectivePrefs,
  validateLeadTimeInputs,
  type ObjectiveId,
  type ObjectivePrefs,
} from "./objectivePrefs";

type LeadInputState = Record<ObjectiveId, [string, string]>;
function leadInputState(prefs: ObjectivePrefs): LeadInputState {
  return Object.fromEntries(OBJECTIVE_PREF_KEYS.map((id) => {
    const values = prefs.objectives[id].lead_s;
    return [id, [values[0]?.toString() ?? "", values[1]?.toString() ?? ""]];
  })) as LeadInputState;
}

export function ObjectiveSettings({ locale }: { locale: Locale }) {
  const copy = OBJECTIVE_COPY[locale];
  const [prefs, setPrefs] = useState<ObjectivePrefs | null>(null);
  const [leadInputs, setLeadInputs] = useState<LeadInputState | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [leadErrors, setLeadErrors] = useState<Partial<Record<ObjectiveId, string>>>({});

  useEffect(() => {
    let active = true;
    void objectivePrefsGet()
      .then((value) => {
        if (!active) return;
        const parsed = parseObjectivePrefs(value);
        if (parsed) {
          setPrefs(parsed);
          setLeadInputs(leadInputState(parsed));
        } else setError(copy.loadFailed);
      })
      .catch(() => { if (active) setError(copy.loadFailed); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [copy.loadFailed]);

  async function applyPatch(request: Record<string, unknown>) {
    if (!prefs || saving) return;
    const previous = prefs;
    setSaving(true);
    setError(null);
    try {
      const updated = parseObjectivePrefs(await objectivePrefsPatch(request));
      if (!updated) throw new Error("OBJECTIVE_PREFS_INVALID");
      setPrefs(updated);
      setLeadInputs(leadInputState(updated));
    } catch {
      setPrefs(previous);
      setLeadInputs(leadInputState(previous));
      setError(copy.saveFailed);
    } finally {
      setSaving(false);
    }
  }

  async function changeEnabled(id: ObjectiveId, enabled: boolean) {
    setLeadErrors((current) => ({ ...current, [id]: undefined }));
    await applyPatch({ objectives: { [id]: { enabled } } });
  }

  async function changeDiagnostic(enabled: boolean) {
    await applyPatch({ diagnostic_map_keys: enabled });
  }

  async function saveLeadTimes(id: ObjectiveId) {
    if (!prefs || !leadInputs || saving) return;
    const result = validateLeadTimeInputs(leadInputs[id]);
    if (result.error) {
      setLeadErrors((current) => ({
        ...current,
        [id]: result.error === "whole_seconds" ? copy.leadTimeWholeError
          : result.error === "duplicate" ? copy.leadTimeDuplicateError : copy.leadTimeRangeError,
      }));
      return;
    }
    setLeadErrors((current) => ({ ...current, [id]: undefined }));
    await applyPatch({ objectives: { [id]: { lead_s: result.values } } });
  }

  function changeLeadInput(id: ObjectiveId, index: 0 | 1, value: string) {
    setLeadInputs((current) => {
      if (!current) return current;
      const values: [string, string] = [...current[id]];
      values[index] = value;
      return { ...current, [id]: values };
    });
    setLeadErrors((current) => ({ ...current, [id]: undefined }));
  }

  if (loading) return <p className="objective-settings-message" role="status">{copy.loading}</p>;

  return (
    <section className="objective-settings" aria-label={copy.title}>
      <h2>{copy.title}</h2>
      {prefs && leadInputs ? (
        <>
          <div className="objective-settings-list">
            {OBJECTIVE_PREF_KEYS.map((id) => (
              <fieldset className="objective-setting-card" key={id} disabled={saving}>
                <legend>
                  <label className="objective-setting-toggle">
                    <input type="checkbox" checked={prefs.objectives[id].enabled}
                      onChange={(event) => void changeEnabled(id, event.currentTarget.checked)} />
                    <span>{copy[id]}</span>
                  </label>
                </legend>
                <div className="objective-setting-leads">
                  <p>{copy.leadTimes}</p>
                  <div className="objective-setting-lead-inputs">
                    {([0, 1] as const).map((index) => (
                      <label key={index}>
                        <span>{copy.leadTimeSlot(index + 1)}</span>
                        <input type="number" inputMode="numeric" min={5} max={60} step={1}
                          aria-label={copy.leadTimeSlot(index + 1)}
                          value={leadInputs[id][index]}
                          onChange={(event) => changeLeadInput(id, index, event.currentTarget.value)} />
                      </label>
                    ))}
                  </div>
                  <small>{copy.leadTimeHelp}</small>
                  {leadErrors[id] && <p className="objective-settings-message" role="alert">{leadErrors[id]}</p>}
                  <button type="button" className="objective-setting-save" onClick={() => void saveLeadTimes(id)}>{copy.saveLeadTimes}</button>
                </div>
              </fieldset>
            ))}
          </div>
          <label className="objective-setting-toggle objective-diagnostic-toggle">
            <input type="checkbox" checked={prefs.diagnostic_map_keys} disabled={saving}
              onChange={(event) => void changeDiagnostic(event.currentTarget.checked)} />
            <span>{copy.diagnosticMapKeys}<small>{copy.diagnosticHelp}</small></span>
          </label>
        </>
      ) : null}
      {error && <p className="objective-settings-message" role="alert">{error}</p>}
    </section>
  );
}
