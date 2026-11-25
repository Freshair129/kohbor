import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { Locale } from "../copy";
import "./knowledge/knowledge.css";

type ReviewState = Record<string, any>;

const verdictLabel = (value: string) => ({ above: "เกินคาด", as: "ตามคาด", below: "ต่ำกว่าคาด" }[value] ?? "— ไม่ทราบ");

export function LaneReviewPage({ locale }: { locale: Locale }) {
  const [report, setReport] = useState<ReviewState | null>(null);
  const [review, setReview] = useState<ReviewState | null>(null);
  const [tier, setTier] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const th = locale === "th";

  useEffect(() => {
    let active = true;
    void invoke<ReviewState>("analytics_lane_calibration_get").then(value => { if (active) setReport(value); })
      .catch(() => { if (active) setReport({ available: false, reason: "calibration_unavailable" }); });
    return () => { active = false; };
  }, []);

  async function action<T>(name: string, args?: Record<string, unknown>) {
    setBusy(true); setError(null);
    try {
      const value = await invoke<T>(name, args);
      return value;
    } catch (reason) {
      setError(typeof reason === "string" ? reason : (th ? "ทำรายการไม่สำเร็จ" : "The action failed"));
      return null;
    } finally { setBusy(false); }
  }

  async function runCalibration() {
    const value = await action<ReviewState>("analytics_lane_calibration_run");
    if (value) setReport(value);
  }
  async function startReview() {
    const value = await action<ReviewState>("analytics_lane_review_start");
    if (value) setReview(value);
  }
  async function submitBlind() {
    if (tier === null) return;
    const value = await action<ReviewState>("analytics_lane_review_submit_blind", { tier });
    if (value) setReview(value);
  }
  async function submitHalfBlind(verdict: string) {
    const value = await action<ReviewState>("analytics_lane_review_submit_half_blind", { verdict });
    if (value) setReview(value);
  }
  async function next() {
    const value = await action<ReviewState>("analytics_lane_review_next");
    if (value) setReview(value);
  }
  async function exportReview() {
    const value = await action<ReviewState>("analytics_lane_review_export");
    if (!value) return;
    const blob = new Blob([JSON.stringify(value, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "lane-coach-review.json";
    anchor.click();
    URL.revokeObjectURL(url);
  }

  return <section className="a2k-comparison a2k-lane-review">
    <h2>{th ? "ทบทวนคะแนนเลน" : "Lane score review"}</h2>
    <section className="a2k-lane-review__calibration">
      <h3>{th ? "L4 · ปรับเทียบจากรีเพลย์ในเครื่อง" : "L4 · Local replay calibration"}</h3>
      {!report || report.available === false
        ? <p>{th ? "ยังไม่มีรายงานปรับเทียบ ต้องมีเลนมาตรฐานอย่างน้อย 30 เลน คำสั่งนี้ไม่เขียนตารางค่าคงที่" : "No calibration report. At least 30 standard lanes are required; this tool never writes the constants table."}</p>
        : <><p>{th ? "ชุดปรับเทียบปิดแล้ว · ค่าที่เสนอไม่ทำให้ป้ายทดลองผ่านเกณฑ์" : "Calibration set is closed. The proposal does not pass the trial gate."}</p>
          <p>n={report.lane_count ?? "— ไม่ทราบ"} · hash {report.proposed_constants_hash ?? "— ไม่ทราบ"}</p></>}
      {report?.available === false && <button type="button" disabled={busy} onClick={() => void runCalibration()}>{th ? "คำนวณรายงานปรับเทียบ" : "Run calibration report"}</button>}
    </section>
    <section className="a2k-lane-review__coach">
      <h3>{th ? "L5 · ทบทวนแบบปิดบังข้อมูล" : "L5 · Blind coach review"}</h3>
      <p>{th ? "โค้ชจะเห็นฮีโร่ บทบาท และแพตช์ก่อน จากนั้นเห็นผลดิบ แล้วจึงเปิดค่าคาดหมายและคำตัดสินของแบบจำลอง" : "The coach sees heroes, roles and patch first, then raw results, then the expectation and model verdict."}</p>
      {!review && <button type="button" disabled={busy || !report || report.available === false} onClick={() => void startReview()}>{th ? "เริ่มรอบทบทวน" : "Start review"}</button>}
      {review && review.complete === true && <><p>{th ? "ทบทวนครบทุกเลน" : "All review lanes are complete."}</p><button type="button" disabled={busy} onClick={() => void exportReview()}>{th ? "ส่งออกใบทบทวน" : "Export review sheets"}</button></>}
      {review && review.complete !== true && <div className="a2k-lane-review__card">
        <header><strong>{review.lane} · {review.shape}</strong><span>{review.index + 1}/{review.total}</span></header>
        <p>{review.patch ?? "— ไม่ทราบ"}</p>
        {(["radiant", "dire"] as const).map(side => <p key={side}><b>{side === "radiant" ? "Radiant" : "Dire"}</b> · {(review.heroes?.[side] ?? []).map((hero: any) => `${hero.hero}${hero.role ? ` (${hero.role})` : ""}`).join(" · ") || "— ไม่ทราบ"}</p>)}
        {review.stage === "blind" && <>
          <p>{review.prompt}</p>
          <label>{th ? "ระดับความได้เปรียบของ Radiant" : "Radiant advantage tier"} <select value={tier ?? ""} onChange={event => setTier(event.target.value === "" ? null : Number(event.target.value))}>
            <option value="">{th ? "เลือกค่าก่อน" : "Choose a value"}</option>
            {Array.from({ length: review.tier_max - review.tier_min + 1 }, (_, index) => review.tier_min + index).map(value => <option key={value} value={value}>{value}</option>)}
          </select></label>
          <button type="button" disabled={busy || tier === null} onClick={() => void submitBlind()}>{th ? "บันทึก แล้วดูตัวเลขดิบ" : "Save and show raw numbers"}</button>
        </>}
        {review.stage === "half_blind" && <>
          <p className="a2k-lane-reading__verdict">{review.raw?.trial_label}</p><pre>{JSON.stringify(review.raw, null, 2)}</pre><p>{review.prompt}</p>
          {(["above", "as", "below"] as const).map(value => <button key={value} type="button" disabled={busy} onClick={() => void submitHalfBlind(value)}>{verdictLabel(value)}</button>)}
        </>}
        {review.stage === "revealed" && <>
          <p>{review.prompt}</p><p>โค้ช: {review.coach_verdict && verdictLabel(review.coach_verdict)} · แบบจำลอง: {review.model_verdict && verdictLabel(review.model_verdict)}</p>
          <p>ค่าคาดหมาย: {review.expectation?.value ?? "— ไม่ทราบ"} · {review.expectation?.source_label ?? "— ไม่ทราบ"} · {review.expectation?.trial_label ?? "ทดลอง · เกณฑ์ชั่วคราว"}</p>
          <button type="button" disabled={busy} onClick={() => void next()}>{th ? "เปิดเลนถัดไป" : "Reveal next lane"}</button>
        </>}
      </div>}
    </section>
    {error && <p className="analytics-error" role="alert">{error}</p>}
  </section>;
}
