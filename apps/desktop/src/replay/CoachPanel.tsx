import type { CSSProperties, ReactNode } from "react";
import type { Locale } from "../copy";
import { clock, DEATH_TYPES, PHASES, RATE_MIN_MATCHES, ratio, summarize } from "./coach";
import type { DeathType, PostMatchReplay } from "./report";

type Text = { th: string; en: string };
const TYPE: Record<DeathType, Text> = {
  solo: { th: "โดนจับคนเดียว", en: "Caught alone" },
  gank: { th: "โดนแก๊ง", en: "Ganked" },
  teamfight: { th: "ในทีมไฟต์", en: "Teamfight" },
  other: { th: "อื่น ๆ (มีเพื่อนใกล้)", en: "Other (ally nearby)" },
};
const OUTCOME: Record<string, Text> = {
  ahead: { th: "นำ", en: "ahead" }, even: { th: "สูสี", en: "even" }, behind: { th: "ตามหลัง", en: "behind" },
};
const LANE: Record<string, Text> = { top: { th: "บน", en: "top" }, mid: { th: "กลาง", en: "mid" }, bot: { th: "ล่าง", en: "bot" } };

const tagStyle = (kind: "obs" | "der" | "unk"): CSSProperties => ({
  fontSize: 11, fontWeight: 600, padding: "1px 6px", borderRadius: 5, marginInlineStart: 6, whiteSpace: "nowrap",
  background: kind === "obs" ? "#123a33" : kind === "der" ? "#1f2c4d" : "#3a2323",
  color: kind === "obs" ? "#7fe3cf" : kind === "der" ? "#a9c3ff" : "#f1a79c",
});
function Tag({ kind, children }: { kind: "obs" | "der" | "unk"; children: ReactNode }) {
  return <span style={tagStyle(kind)}>{children}</span>;
}
const cell: CSSProperties = { padding: "4px 8px", borderBottom: "1px solid rgba(127,127,127,.25)", textAlign: "left" };
const num: CSSProperties = { ...cell, textAlign: "right", fontVariantNumeric: "tabular-nums" };

export function CoachPanel({ report, locale }: { report: PostMatchReplay; locale: Locale }) {
  const th = locale === "th";
  const t = (x: Text) => (th ? x.th : x.en);
  const summary = summarize(report);
  if (!summary) return null;
  const { matches: n, rules } = summary;
  const total = summary.deaths.length;
  const derived = <Tag kind="der">{th ? "คำนวณ" : "DERIVED"}</Tag>;
  const fullMap = <Tag kind="unk">{th ? "ใช้ตำแหน่งศัตรูทั้งแมป · ตอนเล่นมองไม่เห็น" : "Full-map enemy positions · not visible in play"}</Tag>;
  const phaseLabel = (i: number) => {
    const [lo, hi] = PHASES[i]!;
    return hi === Infinity ? `${lo / 60}:00+` : `${lo / 60}:00–${hi / 60 - 1}:59`;
  };

  return (
    <section className="card" aria-labelledby="coach-title" style={{ overflowX: "auto" }}>
      <h2 id="coach-title">{th ? "รีวิวการตาย (โค้ช)" : "Death review (coach)"}</h2>
      <p role="note">
        {th ? `${n} เกม · ฮีโร่ของคุณเท่านั้น · เพื่อนแสดงเป็นฮีโร่ + ช่อง` : `${n} matches · your hero only · teammates shown as hero + slot`}
        {n < RATE_MIN_MATCHES && <> · {th ? `ตัวอย่างน้อย (n = ${n}) แสดงเป็น k/n ไม่ใช่อัตรา` : `small sample (n = ${n}): counts as k/n, not rates`}</>}
        <br />
        {th ? `เกณฑ์ ${rules.version} เป็นค่าชั่วคราว รอยืนยันหลังดู 3–5 เกม` : `Rules ${rules.version} are provisional, to be confirmed after 3–5 matches`}
      </p>

      <h3>{th ? "ตายแบบไหน" : "How you died"}{derived}{fullMap}</h3>
      <table style={{ borderCollapse: "collapse", width: "100%" }}>
        <thead><tr>
          <th style={cell}>{th ? "ประเภท" : "Type"}</th>
          <th style={num}>{th ? "ครั้ง" : "Deaths"}</th>
          <th style={num}>{th ? "เกมที่เกิด" : "Matches"}</th>
          {PHASES.map((_, i) => <th key={i} style={num}>{phaseLabel(i)}</th>)}
        </tr></thead>
        <tbody>{DEATH_TYPES.map(type => (
          <tr key={type}>
            <td style={cell}>{t(TYPE[type])}</td>
            <td style={num}>{ratio(summary.byType[type], total, n)}</td>
            <td style={num}>{`${summary.matchesWithType[type]}/${n}`}</td>
            {summary.byPhase.map((row, i) => <td key={i} style={num}>{row[type]}</td>)}
          </tr>
        ))}</tbody>
      </table>
      <p style={{ fontSize: 12, opacity: .8 }}>
        {th
          ? `โดนแก๊ง = ศัตรูอย่างน้อย 2 คนในระยะ ${rules.deathType.radius} หน่วยหรือทำดาเมจใน ${rules.deathType.windowSeconds} วินาทีก่อนตาย และไม่มีเพื่อนในระยะ ${rules.deathType.allyRadius} · ทีมไฟต์ = มีฮีโร่ตาย ≥ ${rules.deathType.teamfightDeaths} ตัวภายใน ±${rules.deathType.teamfightSeconds} วินาทีและ ${rules.deathType.teamfightRadius} หน่วย`
          : `Ganked = at least 2 enemies within ${rules.deathType.radius} units or dealing damage in the ${rules.deathType.windowSeconds} s before death, with no ally within ${rules.deathType.allyRadius}. Teamfight = ≥ ${rules.deathType.teamfightDeaths} hero deaths within ±${rules.deathType.teamfightSeconds} s and ${rules.deathType.teamfightRadius} units.`}
      </p>

      <h3>{th ? "การมองเห็นตอนตาย" : "Vision at death"}{derived}</h3>
      <table style={{ borderCollapse: "collapse", width: "100%" }}><tbody>
        <tr><td style={cell}>{th ? `วอร์ดฝั่งเราครอบจุดตาย (≤ ${rules.vision.observerRadius} หน่วย)` : `Our observer ward covered the spot (≤ ${rules.vision.observerRadius} units)`}</td><td style={num}>{ratio(summary.vision.ownWard, total, n)}</td></tr>
        <tr><td style={cell}>{th ? "วอร์ดศัตรูครอบจุดตาย" : "Enemy observer ward covered the spot"}</td><td style={num}>{ratio(summary.vision.enemyWard, total, n)}</td></tr>
        <tr><td style={cell}>{th ? `ศัตรูมองเห็นคุณ ≥ ${rules.notes.seenSeconds} จาก ${rules.vision.visibleWindowSeconds} วินาทีก่อนตาย` : `Enemy team saw you ≥ ${rules.notes.seenSeconds} of the ${rules.vision.visibleWindowSeconds} s before death`}<Tag kind="obs">{th ? "สถานะการมองเห็นจาก DEM" : "DEM visibility state"}</Tag></td><td style={num}>{ratio(summary.vision.seen, total - summary.vision.seenUnknown, n)}</td></tr>
        <tr><td style={cell}>{th ? `ศัตรูใช้ Smoke ภายใน ${rules.smoke.windowSeconds} วินาทีก่อนตาย (เชื่อมจากเวลาเท่านั้น)` : `Enemy Smoke within ${rules.smoke.windowSeconds} s before death (timing link only)`}</td><td style={num}>{ratio(summary.vision.smoke, total, n)}</td></tr>
      </tbody></table>

      <h3>{th ? "ผลเลนที่นาที 10" : "Lane outcome at 10:00"}<Tag kind="obs">{th ? "net worth รายนาที" : "per-minute net worth"}</Tag><Tag kind="der">{th ? "การแบ่งเลนคำนวณ" : "lane assignment derived"}</Tag></h3>
      <table style={{ borderCollapse: "collapse", width: "100%" }}>
        <thead><tr>
          <th style={cell}>{th ? "เกม" : "Match"}</th>
          {(["top", "mid", "bot"] as const).map(lane => <th key={lane} style={num}>{t(LANE[lane]!)}</th>)}
        </tr></thead>
        <tbody>{report.matches.filter(m => m.coach).map(m => (
          <tr key={m.id}>
            <td style={cell}>{m.hero} · {m.patch ?? "?"}</td>
            {(["top", "mid", "bot"] as const).map(lane => {
              const row = m.coach!.lane10?.lanes.find(r => r.lane === lane);
              const mine = row?.heroes.includes("self");
              return <td key={lane} style={{ ...num, fontWeight: mine ? 700 : 400 }}>
                {row ? `${row.nwDiff === null ? "?" : (row.nwDiff > 0 ? "+" : "") + row.nwDiff}${row.outcome ? ` ${t(OUTCOME[row.outcome]!)}` : ""}${mine ? (th ? " (คุณ)" : " (you)") : ""}` : "—"}
              </td>;
            })}
          </tr>
        ))}</tbody>
      </table>
      <p style={{ fontSize: 12, opacity: .8 }}>
        {th ? `ส่วนต่าง net worth รวมของฮีโร่ฝั่งเรากับคู่เลน · ตามหลังหรือนำเมื่อห่างอย่างน้อย ${rules.lane.threshold} (ค่าชั่วคราว) · ไม่นับฮีโร่ที่โรม`
          : `Summed net-worth difference against lane opponents · behind or ahead from ${rules.lane.threshold} (provisional) · roaming heroes excluded`}
      </p>

      {summary.notes.length > 0 && <>
        <h3>{th ? "ข้อสังเกตจากหลักฐาน" : "Evidence notes"}{derived}</h3>
        <ul>{summary.notes.map(({ matchId, hero, note }, i) => <li key={`${matchId}-${i}`}>{n > 1 && <b>{hero} · </b>}{t(note)}</li>)}</ul>
      </>}

      <details>
        <summary>{th ? `รายการการตาย (${total})` : `Each death (${total})`}</summary>
        <table style={{ borderCollapse: "collapse", width: "100%" }}>
          <thead><tr>
            <th style={cell}>{th ? "เวลา" : "Time"}</th><th style={cell}>{th ? "ประเภท" : "Type"}</th>
            <th style={num}>{th ? "ศัตรู" : "Enemies"}</th><th style={num}>{th ? "เพื่อนใกล้" : "Allies near"}</th>
            <th style={num}>{th ? "ถูกเห็น (วิ)" : "Seen (s)"}</th><th style={cell}>{th ? "วอร์ดเรา/ศัตรู" : "Our/enemy ward"}</th>
            <th style={num}>{th ? "Smoke (วิก่อน)" : "Smoke (s before)"}</th><th style={num}>{th ? "อายุตัวอย่าง" : "Sample age"}</th>
          </tr></thead>
          <tbody>{summary.deaths.map(d => (
            <tr key={`${d.matchId}-${d.tick}`}>
              <td style={cell}>{n > 1 ? `${d.hero} ` : ""}{clock(d.t)}</td>
              <td style={cell}>{t(TYPE[d.type])}</td>
              <td style={num}>{d.enemiesInvolved}</td><td style={num}>{d.alliesNear}</td>
              <td style={num}>{d.enemyVisibleSeconds ?? "?"}</td>
              <td style={cell}>{`${d.ownWard ? "✓" : "–"} / ${d.enemyWard ? "✓" : "–"}`}</td>
              <td style={num}>{d.enemySmoke ?? "–"}</td><td style={num}>{d.sampleAge.toFixed(2)} s</td>
            </tr>
          ))}</tbody>
        </table>
      </details>
    </section>
  );
}
