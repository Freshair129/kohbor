import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import type { Locale } from "../copy";
import "./analyticsSettings.css";

type Settings = { ai_on: boolean; consent: boolean; model: "claude-opus-5-5" | "claude-sonnet-5-5" | "claude-haiku-5-5"; keep_newest: number; max_bytes: number; auto_analyse: boolean };
type KeyStatus = { set: boolean; last4: string | null };
type LedgerRow = { time_unix_ms: number; model: string; bytes_sent: number; bytes_received: number; input_tokens: number | null; cache_read_tokens: number | null; cache_creation_tokens: number | null; output_tokens: number | null; status: string };
const word = (locale: Locale, th: string, en: string) => locale === "th" ? th : en;

export function AnalyticsSettings({ locale }: { locale: Locale }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [key, setKey] = useState<KeyStatus>({ set: false, last4: null });
  const [ledger, setLedger] = useState<LedgerRow[]>([]);
  const [secret, setSecret] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const password = useRef<HTMLInputElement>(null);
  const refresh = async () => {
    try {
      const [next, keyState, rows] = await Promise.all([
        invoke<Settings>("analytics_settings_get"), invoke<KeyStatus>("analytics_key_status"), invoke<LedgerRow[]>("analytics_ledger_list"),
      ]);
      setSettings(next); setKey(keyState); setLedger(rows);
    } catch { setError(word(locale, "อ่านการตั้งค่าวิเคราะห์ไม่ได้", "Could not read Analytics settings")); }
  };
  useEffect(() => { void refresh(); }, [locale]);
  async function patch(values: Partial<Settings>) {
    if (!settings) return;
    setBusy(true); setError(null);
    try { setSettings(await invoke<Settings>("analytics_settings_patch", { patch: values })); }
    catch { setError(word(locale, "บันทึกการตั้งค่าไม่ได้", "Could not save settings")); }
    finally { setBusy(false); }
  }
  async function saveKey() {
    if (!secret) return; setBusy(true); setError(null);
    try { await invoke("analytics_key_set", { key: secret }); setSecret(""); if (password.current) password.current.value = ""; await refresh(); }
    catch { setError(word(locale, "บันทึก API key ไม่สำเร็จ", "Could not save the API key")); }
    finally { setBusy(false); }
  }
  async function deleteKey() {
    if (!window.confirm(word(locale, "ลบ API key ที่บันทึกไว้หรือไม่?", "Delete the saved API key?"))) return;
    setBusy(true); setError(null);
    try { await invoke("analytics_key_delete"); await refresh(); }
    catch { setError(word(locale, "ลบ API key ไม่สำเร็จ", "Could not delete the API key")); }
    finally { setBusy(false); }
  }
  async function revokeConsent() {
    setBusy(true); setError(null);
    try { await invoke("analytics_consent_set", { consent: false }); await refresh(); }
    catch { setError(word(locale, "ถอนความยินยอมไม่สำเร็จ", "Could not revoke consent")); }
    finally { setBusy(false); }
  }
  return <section className="card analytics-settings" aria-labelledby="analytics-settings-title">
    <h2 id="analytics-settings-title">{word(locale, "รีเพลย์และการวิเคราะห์", "Replay & analysis")}</h2>
    <div className="analytics-settings-grid">
      <section><h3>{word(locale, "คลังรีเพลย์", "Replay storage")}</h3>
        <label><input type="checkbox" checked={settings?.auto_analyse ?? true} disabled={!settings || busy} onChange={event => void patch({ auto_analyse: event.currentTarget.checked })} /> {word(locale, "วิเคราะห์อัตโนมัติเมื่อไม่อยู่ในแมตช์", "Auto-analyse outside matches")}</label>
        <label>{word(locale, "จำนวนรีเพลย์ล่าสุด", "Newest replay count")} <input type="number" min="5" max="100" value={settings?.keep_newest ?? 20} disabled={!settings || busy} onChange={event => void patch({ keep_newest: Number(event.currentTarget.value) })} /></label>
        <label>{word(locale, "ขนาดสูงสุด (GB)", "Size cap (GB)")} <input type="number" min="0.001" step="0.25" value={settings ? settings.max_bytes / 1024 ** 3 : 2} disabled={!settings || busy} onChange={event => { const value = Number(event.currentTarget.value); if (Number.isFinite(value) && value > 0) void patch({ max_bytes: Math.round(value * 1024 ** 3) }); }} /></label>
      </section>
      <section><h3>{word(locale, "Claude API", "Claude API")}</h3>
        <label>{word(locale, "API key ของคุณ", "Your API key")} <input ref={password} type="password" autoComplete="off" value={secret} onChange={event => setSecret(event.currentTarget.value)} placeholder={key.set ? `${word(locale, "ตั้งแล้ว · ลงท้าย", "Set · ending")}: ${key.last4 ?? "••••"}` : word(locale, "ยังไม่ได้ตั้ง", "Not set")} /></label>
        <div><button type="button" disabled={!secret || busy} onClick={() => void saveKey()}>{word(locale, "บันทึก key", "Save key")}</button><button type="button" disabled={!key.set || busy} onClick={() => void deleteKey()}>{word(locale, "ลบ key", "Delete key")}</button></div>
        <label>{word(locale, "โมเดล", "Model")} <select value={settings?.model ?? "claude-opus-5-5"} disabled={!settings || busy} onChange={event => void patch({ model: event.currentTarget.value as Settings["model"] })}>
          <option value="claude-opus-5-5">claude-opus-5-5</option><option value="claude-sonnet-5-5">claude-sonnet-5-5</option><option value="claude-haiku-5-5">claude-haiku-5-5</option>
        </select></label><small>{word(locale, "ระดับ effort ใช้ medium", "Effort is fixed at medium")}</small>
        <label><input type="checkbox" checked={settings?.ai_on ?? false} disabled={!settings || busy || !key.set || !settings.consent} onChange={event => void patch({ ai_on: event.currentTarget.checked })} /> {word(locale, "เปิด AI", "AI on")}</label>
        <p>{word(locale, "ความยินยอม", "Consent")}: {settings?.consent ? word(locale, "ให้แล้ว", "Granted") : word(locale, "ยังไม่ให้", "Not granted")}</p>
        <button type="button" disabled={!settings?.consent || busy} onClick={() => void revokeConsent()}>{word(locale, "ถอนความยินยอมและหยุดคำขอ", "Revoke consent and stop requests")}</button>
      </section>
    </div>
    <section><h3>{word(locale, "ประวัติคำขอในเครื่อง", "Local request ledger")}</h3><div className="analytics-ledger-wrap"><table><thead><tr>
      {[word(locale,"เวลา","Time"),word(locale,"โมเดล","Model"),word(locale,"ส่ง","Sent"),word(locale,"รับ","Received"),"Input","Cache read","Cache created","Output",word(locale,"สถานะ","Status")].map(label => <th key={label}>{label}</th>)}
    </tr></thead><tbody>{ledger.map((row,index) => <tr key={`${row.time_unix_ms}-${index}`}><td>{new Date(row.time_unix_ms).toLocaleString(locale === "th" ? "th-TH" : "en-GB", { timeZone: "Asia/Bangkok" })}</td><td>{row.model}</td><td>{row.bytes_sent}</td><td>{row.bytes_received}</td><td>{row.input_tokens ?? "—"}</td><td>{row.cache_read_tokens ?? "—"}</td><td>{row.cache_creation_tokens ?? "—"}</td><td>{row.output_tokens ?? "—"}</td><td>{row.status}</td></tr>)}</tbody></table></div></section>
    {error && <p role="alert">{error}</p>}
  </section>;
}
