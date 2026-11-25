// Hero benchmarks live in their own module: index.tsx must stay free of effects so no AI report is
// ever generated on view (analytics-knowledge test), while this panel loads benchmark state on view.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { buildHeroBenchmarksViewModel, observedHeroBenchmarkMetrics } from "../heroBenchmarksModel.mjs";
import type { KnowledgeHero } from "./index";

type HeroBenchmarksHostView = {
  enabled: boolean;
  state: "disabled" | "unknown" | "loading" | "ready" | "offline" | "budget_exhausted";
  budget_used: number;
  hero_id: number | null;
  fetched_at_ms: number | null;
  benchmarks: {
    hero_id: number;
    gold_per_min?: Array<{ percentile: number; value: number }> | null;
    xp_per_min?: Array<{ percentile: number; value: number }> | null;
    kills_per_min?: Array<{ percentile: number; value: number }> | null;
    last_hits_per_min?: Array<{ percentile: number; value: number }> | null;
    hero_damage_per_min?: Array<{ percentile: number; value: number }> | null;
    hero_healing_per_min?: Array<{ percentile: number; value: number }> | null;
    tower_damage?: Array<{ percentile: number; value: number }> | null;
  } | null;
};

export function HeroBenchmarksPanel({ hero, durationSeconds }: { hero: KnowledgeHero; durationSeconds: number | null }) {
  const [enabled, setEnabled] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [view, setView] = useState<HeroBenchmarksHostView | null>(null);

  useEffect(() => {
    let current = true;
    void invoke<HeroBenchmarksHostView>("hero_benchmarks_state_get").then((next) => {
      if (!current) return;
      setView(next);
      setEnabled(next.enabled);
      setLoaded(true);
    }).catch(() => {
      if (!current) return;
      setError("อ่านสถานะ benchmark ไม่ได้");
      setLoaded(true);
    });
    return () => { current = false; };
  }, []);

  useEffect(() => {
    if (!enabled || !hero.id) return;
    let current = true;
    setView((previous) => ({
      enabled: true,
      state: "loading",
      budget_used: previous?.budget_used ?? 0,
      hero_id: null,
      fetched_at_ms: null,
      benchmarks: null,
    }));
    setError(null);
    void invoke<HeroBenchmarksHostView>("hero_benchmarks_get", { heroKey: hero.id }).then((next) => {
      if (current) setView(next);
    }).catch(() => {
      if (!current) return;
      setView((previous) => previous ? { ...previous, enabled: true, state: "offline" } : {
        enabled: true, state: "offline", budget_used: 0, hero_id: null, fetched_at_ms: null, benchmarks: null,
      });
      setError("เชื่อมต่อ OpenDota ไม่ได้");
    });
    return () => { current = false; };
  }, [enabled, hero.id]);

  async function setConsent(nextEnabled: boolean) {
    setBusy(true);
    setError(null);
    try {
      const next = await invoke<HeroBenchmarksHostView>("hero_benchmarks_set", { enabled: nextEnabled });
      setView(next);
      setEnabled(next.enabled);
    } catch {
      setError(nextEnabled ? "บันทึกการยินยอมไม่ได้" : "ถอนความยินยอมแล้ว แต่ลบ cache ไม่สำเร็จ");
      if (!nextEnabled) {
        setEnabled(false);
        setView((previous) => previous ? { ...previous, enabled: false, state: "disabled", benchmarks: null, fetched_at_ms: null, hero_id: null } : null);
      }
    } finally {
      setBusy(false);
    }
  }

  const model = buildHeroBenchmarksViewModel({
    enabled,
    requestState: view?.state ?? "unknown",
    budgetUsed: view?.budget_used ?? 0,
    benchmarks: view?.benchmarks,
    observed: observedHeroBenchmarkMetrics(hero, durationSeconds),
    fetchedAt: view?.fetched_at_ms,
  });
  const fetchedDate = model.fetchedAt === null ? null : new Date(model.fetchedAt).toLocaleDateString("th-TH");
  return <section className="a2k-benchmarks" aria-labelledby="hero-benchmarks-title">
    <h3 id="hero-benchmarks-title">เทียบผลงานฮีโร่กับ benchmarks</h3>
    <label className="a2k-benchmarks__switch">
      <input type="checkbox" checked={enabled} disabled={busy || !loaded} onChange={(event) => void setConsent(event.currentTarget.checked)} />
      {enabled ? "ใช้ benchmarks จาก OpenDota" : "เปิดใช้ benchmarks จาก OpenDota"}
    </label>
    <p className="a2k-benchmarks__consent">เมื่อเปิดใช้ จะส่งเฉพาะรหัสฮีโร่ที่เลือก และอาจขอรายการฮีโร่สาธารณะแบบไม่มีพารามิเตอร์ (/api/heroes) เพื่อแปลงชื่อฮีโร่เป็นรหัส ไม่มีข้อมูลผู้เล่นหรือแมตช์</p>
    <p>{model.budgetText}</p>
    {error && <p role="alert">{error}</p>}
    {!enabled && <p>ปิดอยู่ · เปิดใช้เพื่อดูตำแหน่ง percentile ของฮีโร่นี้</p>}
    {enabled && model.state === "loading" && <p role="status">กำลังอ่านข้อมูลเทียบ…</p>}
    {enabled && model.state === "offline" && <p role="alert">เชื่อมต่อ OpenDota ไม่ได้ · ค่าจากรีเพลย์ยังอยู่ในเครื่อง</p>}
    {enabled && model.state === "budget_exhausted" && <p>ครบโควตาคำขอในช่วง 24 ชั่วโมง</p>}
    {enabled && model.state === "unknown" && <p>ไม่ทราบข้อมูล benchmark หรือยังไม่มีรหัสฮีโร่จากแหล่งที่อนุญาต</p>}
    {enabled && model.state === "ready" && <>
      <p>{model.attribution}{fetchedDate ? ` · ดึงข้อมูล ${fetchedDate}` : ""}</p>
      <p>{model.population} · ไม่แบ่งตาม bracket</p>
      <table><thead><tr><th>ตัวชี้วัด</th><th>รีเพลย์นี้</th><th>ตำแหน่งเทียบฮีโร่</th></tr></thead><tbody>
        {model.metrics.map((metric) => <tr key={metric.key}>
          <th>{metric.label}</th>
          <td>{metric.value === null ? "— ไม่ทราบ" : metric.value.toFixed(1)}</td>
          <td>{metric.percentile === null ? "— ไม่ทราบ" : `ประมาณ P${metric.percentile.toFixed(0)}`}</td>
        </tr>)}
      </tbody></table>
    </>}
  </section>;
}
