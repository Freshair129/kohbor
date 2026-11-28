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
type HeroBenchmarksSettingsView = { enabled: boolean; budget_used: number };
type UserSettingsView = { replay_auto_fetch_enabled?: unknown };
const word = (locale: Locale, th: string, en: string) => locale === "th" ? th : en;

export function AnalyticsSettings({ locale }: { locale: Locale }) {
  const text = copy(locale).shell;
  const [settings, setSettings] = useState<ProviderPreferences | null>(null);
  const [ledger, setLedger] = useState<LedgerRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [keepNewestDraft, setKeepNewestDraft] = useState(20);
  const [maxBytesDraft, setMaxBytesDraft] = useState(2 * 1024 ** 3);
  const [retentionPreview, setRetentionPreview] = useState<number | null>(null);
  const [retentionBusy, setRetentionBusy] = useState(false);
  const [heroBenchmarks, setHeroBenchmarks] = useState<HeroBenchmarksSettingsView>({ enabled: false, budget_used: 0 });
  const [heroBenchmarksLoaded, setHeroBenchmarksLoaded] = useState(false);
  const [heroBenchmarksBusy, setHeroBenchmarksBusy] = useState(false);
  const [replayAutoFetch, setReplayAutoFetch] = useState(false);
  const [replayAutoFetchBusy, setReplayAutoFetchBusy] = useState(false);
  const [replayAutoFetchLoaded, setReplayAutoFetchLoaded] = useState(false);
  const [replayFetchConsentOpen, setReplayFetchConsentOpen] = useState(false);

  async function refresh() {
    try {
      const [next, rows, userSettings] = await Promise.all([
        invoke<ProviderPreferences>("analytics_settings_get"),
        invoke<LedgerRow[]>("analytics_ledger_list"),
        invoke<UserSettingsView>("settings_get"),
      ]);
      setSettings(next);
      setLedger(rows);
      setReplayAutoFetch(userSettings.replay_auto_fetch_enabled === true);
      setKeepNewestDraft(next.keep_newest);
      setMaxBytesDraft(next.max_bytes);
    } catch {
      setReplayAutoFetch(false);
      setError(word(locale, "อ่านการตั้งค่าวิเคราะห์ไม่ได้", "Could not read Analytics settings"));
    } finally {
      setReplayAutoFetchLoaded(true);
    }
    try {
      setHeroBenchmarks(await invoke<HeroBenchmarksSettingsView>("hero_benchmarks_state_get"));
    } catch {
      setError(word(locale, "อ่านสถานะข้อมูลเทียบฮีโร่ไม่ได้", "Could not read hero benchmark settings"));
    } finally {
      setHeroBenchmarksLoaded(true);
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

  async function changeHeroBenchmarks(enabled: boolean) {
    setHeroBenchmarksBusy(true);
    setError(null);
    try {
      setHeroBenchmarks(await invoke<HeroBenchmarksSettingsView>("hero_benchmarks_set", { enabled }));
    } catch {
      setError(word(locale, "บันทึกความยินยอมข้อมูลเทียบฮีโร่ไม่ได้", "Could not save hero benchmark consent"));
      if (!enabled) setHeroBenchmarks((current) => ({ ...current, enabled: false }));
    } finally {
      setHeroBenchmarksBusy(false);
    }
  }

  async function saveReplayAutoFetch(enabled: boolean, consentAccepted: boolean) {
    setReplayAutoFetchBusy(true);
    setError(null);
    try {
      const saved = await invoke<boolean>("replay_auto_fetch_set", {
        enabled,
        consentAccepted,
      });
      setReplayAutoFetch(saved === true);
      setReplayFetchConsentOpen(false);
    } catch {
      setError(word(locale, "บันทึกการดึงรีเพลย์อัตโนมัติไม่ได้", "Could not save automatic replay fetch"));
      if (!enabled) setReplayAutoFetch(false);
    } finally {
      setReplayAutoFetchBusy(false);
    }
  }

  async function saveRetention() {
    if (!settings || !Number.isInteger(keepNewestDraft) || keepNewestDraft < 5 || keepNewestDraft > 100 || !Number.isSafeInteger(maxBytesDraft) || maxBytesDraft <= 0) return;
    setRetentionBusy(true);
    setError(null);
    try {
      if (retentionPreview === null) {
        const preview = await invoke<{ evicted_matches: number }>("replay_library_retention_preview", {
          keepNewest: keepNewestDraft,
          maxBytes: maxBytesDraft,
        });
        setRetentionPreview(preview.evicted_matches);
      } else {
        setSettings(await invoke<ProviderPreferences>("replay_library_retention_save", {
          keepNewest: keepNewestDraft,
          maxBytes: maxBytesDraft,
          expectedEvictions: retentionPreview,
        }));
        setRetentionPreview(null);
      }
    } catch (cause) {
      setRetentionPreview(null);
      setError(String(cause).includes("retention_preview_stale")
        ? word(locale, "รายการเปลี่ยนไปแล้ว กรุณาดูตัวอย่างการนำออกอีกครั้ง", "The library changed. Preview removals again.")
        : word(locale, "บันทึกการเก็บรีเพลย์ไม่ได้", "Could not save replay retention"));
    } finally {
      setRetentionBusy(false);
    }
  }

  return <section className="card analytics-settings" aria-labelledby="analytics-settings-title">
    <h2 id="analytics-settings-title">{word(locale, "รีเพลย์และการวิเคราะห์", "Replay & analysis")}</h2>
    {settings ? <div className="analytics-settings-grid">
      <section><h3>{word(locale, "คลังรีเพลย์", "Replay storage")}</h3>
        <label><input type="checkbox" checked={settings.auto_analyse} disabled={busy} onChange={event => void patch({ auto_analyse: event.currentTarget.checked })} /> {text.replayAutoParseLabel}</label>
        <p>{text.replayAutoParseHelp}</p>
        <label><input type="checkbox" checked={replayAutoFetch} disabled={replayAutoFetchBusy || !replayAutoFetchLoaded} onChange={event => {
          if (event.currentTarget.checked) setReplayFetchConsentOpen(true);
          else void saveReplayAutoFetch(false, false);
        }} /> {text.replayAutoFetchLabel}</label>
        <p>{text.replayAutoFetchHelp}</p>
        <label>{word(locale, "จำนวนรีเพลย์ล่าสุด", "Newest replay count")} <input type="number" min="5" max="100" value={keepNewestDraft} disabled={busy || retentionBusy} onChange={event => { setKeepNewestDraft(Number(event.currentTarget.value)); setRetentionPreview(null); }} /></label>
        <label>{word(locale, "ขนาดสูงสุด (GB)", "Size cap (GB)")} <input type="number" min="0.001" step="0.25" value={maxBytesDraft / 1024 ** 3} disabled={busy || retentionBusy} onChange={event => { const value = Number(event.currentTarget.value); setMaxBytesDraft(Number.isFinite(value) ? Math.round(value * 1024 ** 3) : 0); setRetentionPreview(null); }} /></label>
        <button type="button" disabled={busy || retentionBusy || !Number.isInteger(keepNewestDraft) || keepNewestDraft < 5 || keepNewestDraft > 100 || maxBytesDraft <= 0 || (retentionPreview === null && keepNewestDraft === settings.keep_newest && maxBytesDraft === settings.max_bytes)} onClick={() => void saveRetention()}>
          {retentionPreview === null ? word(locale, "บันทึก", "Save") : word(locale, "ยืนยันการบันทึก", "Confirm save")}
        </button>
        {retentionPreview !== null && <p role="status">{word(locale, `จะนำรีเพลย์ออก ${retentionPreview} แมตช์ โดยแถวและโน้ตยังอยู่`, `${retentionPreview} replay bundles will be removed; rows and notes will remain.`)}</p>}
      </section>
      <section><h3>{word(locale, "ข้อมูลเทียบฮีโร่", "Hero benchmarks")}</h3>
        <label><input type="checkbox" checked={heroBenchmarks.enabled} disabled={heroBenchmarksBusy || !heroBenchmarksLoaded} onChange={event => void changeHeroBenchmarks(event.currentTarget.checked)} /> {word(locale, "ใช้ benchmarks จาก OpenDota", "Use OpenDota benchmarks")}</label>
        <p>{word(locale, "เมื่อเปิดใช้ จะส่งเฉพาะรหัสฮีโร่ที่เลือก และอาจขอรายการฮีโร่สาธารณะแบบไม่มีพารามิเตอร์ (/api/heroes) เพื่อแปลงชื่อเป็นรหัส ไม่มีข้อมูลผู้เล่นหรือแมตช์", "When enabled, only the selected hero ID is sent; a parameterless public hero list (/api/heroes) may be requested to map its name to an ID. No player or match data is sent.")}</p>
        <p>{word(locale, `ใช้ไป ${heroBenchmarks.budget_used}/4 ใน 24 ชม.`, `${heroBenchmarks.budget_used}/4 requests used in 24 hours`)}</p>
      </section>
      <ProviderControls locale={locale} settings={settings} onSettingsChange={setSettings} />
    </div> : <p>{word(locale, "กำลังอ่านการตั้งค่า…", "Loading settings…")}</p>}
    <section><h3>{word(locale, "ประวัติคำขอในเครื่อง", "Local request ledger")}</h3><div className="analytics-ledger-wrap"><table><thead><tr>
      {[word(locale,"เวลา","Time"),word(locale,"ผู้ให้บริการ","Provider"),word(locale,"ปลายทาง","Endpoint"),word(locale,"โมเดล","Model"),word(locale,"ส่ง","Sent"),word(locale,"รับ","Received"),"Input","Cache read","Cache created","Output",word(locale,"สถานะ","Status")].map(label => <th key={label}>{label}</th>)}
    </tr></thead><tbody>{ledger.map((row,index) => <tr key={`${row.time_unix_ms}-${index}`}><td>{new Date(row.time_unix_ms).toLocaleString(locale === "th" ? "th-TH" : "en-GB", { timeZone: "Asia/Bangkok" })}</td><td>{row.provider}</td><td>{row.endpoint_class}</td><td>{row.model}</td><td>{row.bytes_sent}</td><td>{row.bytes_received}</td><td>{row.input_tokens ?? "—"}</td><td>{row.cache_read_tokens ?? "—"}</td><td>{row.cache_creation_tokens ?? "—"}</td><td>{row.output_tokens ?? "—"}</td><td>{row.status}</td></tr>)}</tbody></table></div></section>
    {error && <p role="alert">{error}</p>}
    {replayFetchConsentOpen && <div className="replay-fetch-consent-backdrop">
      <section className="replay-fetch-consent" role="dialog" aria-modal="true" aria-labelledby="replay-fetch-consent-title">
        <h3 id="replay-fetch-consent-title">{text.replayAutoFetchConsentTitle}</h3>
        <p>{text.replayAutoFetchConsentMatch}</p>
        <p>{text.replayAutoFetchConsentValve}</p>
        <p>{text.replayAutoFetchConsentOperators}</p>
        <p>{text.replayAutoFetchConsentOff}</p>
        <div>
          <button type="button" disabled={replayAutoFetchBusy} onClick={() => setReplayFetchConsentOpen(false)}>{text.replayAutoFetchConsentCancel}</button>
          <button type="button" disabled={replayAutoFetchBusy} onClick={() => void saveReplayAutoFetch(true, true)}>{text.replayAutoFetchConsentAccept}</button>
        </div>
      </section>
    </div>}
  </section>;
}
