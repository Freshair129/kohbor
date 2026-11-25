import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { copy, type Locale } from "../copy";
import { ProviderControls, type ProviderPreferences } from "./ProviderControls";
import "./analyticsSettings.css";

type LedgerRow = {
  time_unix_ms: number;
  provider: string;
  endpoint_class: string;
  model: string;
  bytes_sent: number;
  bytes_received: number;
  input_tokens: number | null;
  cache_read_tokens: number | null;
  cache_creation_tokens: number | null;
  output_tokens: number | null;
  status: string;
};
const word = (locale: Locale, th: string, en: string) => locale === "th" ? th : en;

export function AnalyticsSettings({ locale }: { locale: Locale }) {
  const text = copy(locale).shell;
  const [settings, setSettings] = useState<ProviderPreferences | null>(null);
  const [ledger, setLedger] = useState<LedgerRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    try {
      const [next, rows] = await Promise.all([
        invoke<ProviderPreferences>("analytics_settings_get"),
        invoke<LedgerRow[]>("analytics_ledger_list"),
      ]);
      setSettings(next);
      setLedger(rows);
    } catch {
      setError(word(locale, "อ่านการตั้งค่าวิเคราะห์ไม่ได้", "Could not read Analytics settings"));
    }
  }

  useEffect(() => { void refresh(); }, [locale]);

  async function patch(values: Partial<ProviderPreferences>) {
    if (!settings) return;
    setBusy(true);
    setError(null);
    try {
      setSettings(await invoke<ProviderPreferences>("analytics_settings_patch", { patch: values }));
    } catch {
      setError(word(locale, "บันทึกการตั้งค่าไม่ได้", "Could not save settings"));
    } finally {
      setBusy(false);
    }
  }

  return <section className="card analytics-settings" aria-labelledby="analytics-settings-title">
    <h2 id="analytics-settings-title">{word(locale, "รีเพลย์และการวิเคราะห์", "Replay & analysis")}</h2>
    {settings ? <div className="analytics-settings-grid">
      <section><h3>{word(locale, "คลังรีเพลย์", "Replay storage")}</h3>
        <label><input type="checkbox" checked={settings.auto_analyse} disabled={busy} onChange={event => void patch({ auto_analyse: event.currentTarget.checked })} /> {text.replayAutoParseLabel}</label>
        <p>{text.replayAutoParseHelp}</p>
        <label>{word(locale, "จำนวนรีเพลย์ล่าสุด", "Newest replay count")} <input type="number" min="5" max="100" value={settings.keep_newest} disabled={busy} onChange={event => { const value = Number(event.currentTarget.value); if (Number.isInteger(value) && value >= 5 && value <= 100) void patch({ keep_newest: value }); }} /></label>
        <label>{word(locale, "ขนาดสูงสุด (GB)", "Size cap (GB)")} <input type="number" min="0.001" step="0.25" value={settings.max_bytes / 1024 ** 3} disabled={busy} onChange={event => { const value = Number(event.currentTarget.value); if (Number.isFinite(value) && value > 0) void patch({ max_bytes: Math.round(value * 1024 ** 3) }); }} /></label>
      </section>
      <ProviderControls locale={locale} settings={settings} onSettingsChange={setSettings} />
    </div> : <p>{word(locale, "กำลังอ่านการตั้งค่า…", "Loading settings…")}</p>}
    <section><h3>{word(locale, "ประวัติคำขอในเครื่อง", "Local request ledger")}</h3><div className="analytics-ledger-wrap"><table><thead><tr>
      {[word(locale,"เวลา","Time"),word(locale,"ผู้ให้บริการ","Provider"),word(locale,"ปลายทาง","Endpoint"),word(locale,"โมเดล","Model"),word(locale,"ส่ง","Sent"),word(locale,"รับ","Received"),"Input","Cache read","Cache created","Output",word(locale,"สถานะ","Status")].map(label => <th key={label}>{label}</th>)}
    </tr></thead><tbody>{ledger.map((row,index) => <tr key={`${row.time_unix_ms}-${index}`}><td>{new Date(row.time_unix_ms).toLocaleString(locale === "th" ? "th-TH" : "en-GB", { timeZone: "Asia/Bangkok" })}</td><td>{row.provider}</td><td>{row.endpoint_class}</td><td>{row.model}</td><td>{row.bytes_sent}</td><td>{row.bytes_received}</td><td>{row.input_tokens ?? "—"}</td><td>{row.cache_read_tokens ?? "—"}</td><td>{row.cache_creation_tokens ?? "—"}</td><td>{row.output_tokens ?? "—"}</td><td>{row.status}</td></tr>)}</tbody></table></div></section>
    {error && <p role="alert">{error}</p>}
  </section>;
}
