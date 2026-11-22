import { useEffect, useState, type KeyboardEvent } from "react";
import type { Locale } from "./copy";
import { shellHotkeyGet, shellHotkeySet } from "./runtime";
import { captureKey, describeError, parseShowHotkey, SHOW_HOTKEY_COPY, type ShowHotkeyState } from "./showHotkey";

const DEFAULT_BINDING = "Ctrl+Alt+X";

/** UI-013 Settings section: switch, current key, remap by capture, honest status and limits. */
export function ShowHotkeySettings({ locale }: { locale: Locale }) {
  const copy = SHOW_HOTKEY_COPY[locale];
  const [state, setState] = useState<ShowHotkeyState | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    const parsed = parseShowHotkey(await shellHotkeyGet());
    if (!parsed) throw new Error("SHOW_HOTKEY_INVALID");
    setState(parsed);
    return parsed;
  }

  useEffect(() => {
    let active = true;
    void shellHotkeyGet()
      .then((value) => {
        if (!active) return;
        const parsed = parseShowHotkey(value);
        if (parsed) setState(parsed);
        else setError(copy.loadFailed);
      })
      .catch(() => { if (active) setError(copy.loadFailed); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [copy.loadFailed]);

  async function apply(enabled: boolean, binding: string) {
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      const updated = parseShowHotkey(await shellHotkeySet(enabled, binding));
      if (!updated) throw new Error("SHOW_HOTKEY_INVALID");
      setState(updated);
    } catch (reason) {
      const code = typeof reason === "string" ? reason : reason instanceof Error ? reason.message : "";
      setError(code ? `${copy.saveFailed} ${describeError(code, locale)}` : copy.saveFailed);
      try { await refresh(); } catch { setState(null); }
    } finally {
      setSaving(false);
    }
  }

  function onCaptureKey(event: KeyboardEvent<HTMLButtonElement>) {
    if (!capturing || !state) return;
    event.preventDefault();
    const result = captureKey(event);
    if (result.kind === "pending") return;
    setCapturing(false);
    if (result.kind === "cancel") return;
    if (result.kind === "invalid") { setError(copy.invalidKey); return; }
    void apply(state.enabled, result.label);
  }

  if (loading) return <p className="objective-settings-message" role="status">…</p>;

  const status = !state ? copy.unknown : !state.enabled ? copy.off : state.registered ? copy.registered : copy.notRegistered;

  return (
    <section className="objective-settings show-hotkey-settings" aria-label={copy.title}>
      <h2>{copy.title}</h2>
      {state ? (
        <>
          <label className="objective-setting-toggle">
            <input type="checkbox" checked={state.enabled} disabled={saving}
              onChange={(event) => void apply(event.currentTarget.checked, state.binding)} />
            <span>{copy.toggle}<small>{copy.help}</small></span>
          </label>
          <p className="objective-settings-message" role="status">
            {copy.current}: <kbd>{state.binding}</kbd> · {status}
          </p>
          <div className="show-hotkey-actions">
            <button type="button" disabled={saving} aria-pressed={capturing}
              onClick={() => { setError(null); setCapturing(!capturing); }}
              onKeyDown={onCaptureKey} onBlur={() => setCapturing(false)}>
              {capturing ? copy.capturing : copy.change}
            </button>
            {state.binding !== DEFAULT_BINDING && (
              <button type="button" disabled={saving} onClick={() => void apply(state.enabled, DEFAULT_BINDING)}>
                {copy.reset}
              </button>
            )}
          </div>
          {state.dota_conflict_risk && <p className="objective-settings-message" role="note">{copy.dotaRisk}</p>}
          {state.error_code && !error && (
            <p className="objective-settings-message" role="alert">{describeError(state.error_code, locale)}</p>
          )}
          <p className="objective-settings-message">{copy.fullscreen}</p>
        </>
      ) : null}
      {error && <p className="objective-settings-message" role="alert">{error}</p>}
    </section>
  );
}
