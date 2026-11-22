import { useEffect, useRef, useState } from "react";
import type { Locale } from "../copy";
import { CoachPanel } from "./CoachPanel";
import { MAX_REPORT_BYTES, readPostMatchReplay, type PostMatchReplay } from "./report";
import { mountReplay } from "./viewer.mjs";
import markup from "./viewer.html?raw";
import style from "./viewer.css?raw";

export function ReplayPanel({ locale, available }: { locale: Locale; available: boolean }) {
  const th = locale === "th";
  return available ? <LocalReplay locale={locale} /> : (
    <div className="shell-screen" style={{ overflow: "auto" }}>
      <section className="card">
        <h1>{th ? "รีเพลย์หลังเกม" : "Post-match replay"}</h1>
        <p role="status">{th ? "เปิดได้เมื่อ Dota 2 ปิดอยู่ หรือ Kohbor ยืนยันว่าจบแมตช์แล้ว"
          : "Available once Dota 2 is closed or Kohbor confirms the match has ended."}</p>
      </section>
    </div>
  );
}

function LocalReplay({ locale }: { locale: Locale }) {
  const th = locale === "th";
  const [report, setReport] = useState<PostMatchReplay | null>(null);
  const [invalid, setInvalid] = useState(false);
  const [reading, setReading] = useState(false);
  const target = useRef<HTMLDivElement>(null);
  const request = useRef(0);

  useEffect(() => () => { request.current++; }, []);
  useEffect(() => {
    const frame = target.current;
    if (!report || !frame) return;
    const root = frame.shadowRoot ?? frame.attachShadow({ mode: "open" });
    return mountReplay(root, report, { locale, markup, style });
  }, [report, locale]);

  async function openFile(file: File) {
    const ticket = ++request.current;
    setReading(true);
    setInvalid(false);
    let next: PostMatchReplay | null = null;
    if (file.size <= MAX_REPORT_BYTES) {
      try {
        next = readPostMatchReplay(JSON.parse(await file.text()));
      } catch {
        // An unreadable or malformed file follows the same input-error path.
      }
    }
    if (ticket !== request.current) return;
    setReading(false);
    if (next) setReport(next);
    else setInvalid(true);
  }

  return (
    <div className="shell-screen" style={{ overflow: "auto" }}>
      <section className="card">
        <h1>{th ? "รีเพลย์หลังเกม" : "Post-match replay"}</h1>
        <label>
          {th ? "เปิดรายงาน DEM (.json)" : "Open DEM report (.json)"}
          <input type="file" accept=".json,application/json" onChange={event => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            if (file) void openFile(file);
          }} />
        </label>
        {reading && <p role="status">{th ? "กำลังอ่านรายงาน…" : "Reading report…"}</p>}
        {invalid && <p role="alert">{th ? "เปิดไม่ได้ กรุณาเลือกไฟล์รายงาน DEM สำหรับ Kohbor ขนาดไม่เกิน 16 MB"
          : "Choose a Kohbor DEM report of up to 16 MB."}</p>}
      </section>
      {report && <CoachPanel report={report} locale={locale} />}
      <div ref={target} />
    </div>
  );
}
