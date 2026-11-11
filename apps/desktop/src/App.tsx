import { PHASE_TH } from "./copy";
import { Guide } from "./Guide";
import { OwnHero } from "./OwnHero";
import { Chip, Readiness } from "./Readiness";
import { useRuntime } from "./runtime";

const HOST_LOST = "หน้าจอนี้ติดต่อระบบหลักของ Kohbor ไม่ได้";

export function App() {
  const { status, snapshot, hostLost, setup } = useRuntime();

  if (!status) {
    return (
      <main className="app">
        {hostLost ? (
          <p className="banner banner-warn" role="alert">
            {HOST_LOST} — ปิดแล้วเปิด Kohbor ใหม่
          </p>
        ) : (
          <p className="muted" role="status">
            กำลังเริ่มระบบ…
          </p>
        )}
      </main>
    );
  }

  const phase = status.session.phase;
  const sourceLive = status.intake.state === "receiving";
  const phaseTone = phase === "none" ? "unknown" : phase === "unsupported" ? "off" : sourceLive ? "fresh" : "stale";

  return (
    <main className="app">
      <header className="top">
        <h1>ภาพรวม</h1>
        <Chip tone={phaseTone} label={phaseTone === "stale" ? `${PHASE_TH[phase]} (ข้อมูลเก่า)` : PHASE_TH[phase]} />
      </header>

      {hostLost && (
        <p className="banner banner-warn" role="alert">
          {HOST_LOST} — ค่าด้านล่างเป็นค่าล่าสุดที่ได้รับ ไม่ใช่ค่าปัจจุบัน
        </p>
      )}

      <div className={hostLost ? "content is-outdated" : "content"}>
        {/* Advice derived from values that are no longer current would mislead. */}
        {!hostLost && <Guide status={status} setup={setup} />}
        <Readiness status={status} />
        <OwnHero snap={snapshot} phase={phase} sourceLive={sourceLive} />
      </div>
    </main>
  );
}
