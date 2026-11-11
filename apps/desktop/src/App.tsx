import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactElement } from "react";
import type { Field, OwnSnapshot, RuntimeStatus } from "./types";

// Thai copy for closed error codes (contracts/error-catalog.json message keys).
const REASON_TH: Record<string, string> = {
  SOURCE_UNAVAILABLE: "ยังไม่ได้ตั้งค่าการเชื่อมต่อเกม",
  SOURCE_STALE: "ข้อมูลจากเกมเก่าเกิน 2 วินาที",
  OWNER_UNVERIFIED: "ยืนยันผู้เล่นไม่ได้ — ล็อกอิน Steam แล้วเปิด Dota 2 ใหม่",
  GSI_GAME_NOT_FOUND: "ไม่พบ Dota 2 — เลือกโฟลเดอร์เกม",
  GSI_CONFIG_CONFLICT: "ไฟล์ตั้งค่าถูกแก้จากภายนอก — ไม่เขียนทับ",
  GSI_CONFIG_WRITE_FAILED: "เขียนไฟล์ตั้งค่าไม่ได้",
  GSI_PORT_UNAVAILABLE: "พอร์ตถูกใช้อยู่ — ปิดโปรแกรมที่ใช้พอร์ต หรือเปิด Dota 2 ใหม่หลังตั้งค่า",
  GSI_PORT_EXHAUSTED: "พอร์ต 3000–3010 ถูกใช้หมด",
  GSI_SECRET_UNAVAILABLE: "เข้าถึงที่เก็บความลับของ Windows ไม่ได้",
  MEDIA_UNMAPPED: "แพ็กที่เลือกยังไม่มีเสียงเตือนสำคัญที่ผ่านการตรวจ",
  CAPABILITY_UNAVAILABLE: "ยังไม่มีในบิลด์นี้",
};

type Tone = "ok" | "warn" | "muted" | "stale";

function reason(code: string | null): string {
  return code ? (REASON_TH[code] ?? code) : "";
}

function Chip({ tone, label }: { tone: Tone; label: string }) {
  return <span className={`chip chip-${tone}`}>{label}</span>;
}

function Row({ name, chip, detail, action }: { name: string; chip: ReactElement; detail: string; action?: ReactElement }) {
  return (
    <div className="row" role="row">
      <span className="row-name" role="rowheader">{name}</span>
      <span role="cell">{chip}</span>
      <span className="row-detail" role="cell">{detail}</span>
      <span role="cell">{action}</span>
    </div>
  );
}

function num<T>(f: Field<T> | undefined): string {
  return f && f.value !== null && f.value !== undefined ? String(f.value) : "—";
}

export function App() {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [snap, setSnap] = useState<OwnSnapshot | null>(null);
  const [setupError, setSetupError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    const tick = async () => {
      try {
        const [s, o] = await Promise.all([
          invoke<RuntimeStatus>("runtime_get_status"),
          invoke<OwnSnapshot>("own_snapshot"),
        ]);
        if (alive) {
          setStatus(s);
          setSnap(o);
        }
      } catch {
        // Keep the last truthful state; a failed poll never fabricates readiness.
      }
    };
    void tick();
    const id = window.setInterval(tick, 1000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, []);

  const runSetup = async () => {
    setBusy(true);
    setSetupError(null);
    try {
      await invoke("gsi_setup");
    } catch (e) {
      setSetupError(reason(String(e)));
    } finally {
      setBusy(false);
    }
  };

  if (!status) {
    return <main className="app"><p className="muted">กำลังเริ่มระบบ…</p></main>;
  }

  const i = status.intake;
  const intakeChip: Record<RuntimeStatus["intake"]["state"], ReactElement> = {
    not_configured: <Chip tone="muted" label="ยังไม่ตั้งค่า" />,
    starting: <Chip tone="muted" label="กำลังเริ่ม" />,
    listening: <Chip tone="ok" label="พร้อมรับ" />,
    receiving: <Chip tone="ok" label="สด" />,
    stale: <Chip tone="stale" label="ข้อมูลเก่า" />,
    unavailable: <Chip tone="warn" label="ใช้ไม่ได้" />,
  };
  const intakeDetail =
    i.state === "receiving" || i.state === "stale"
      ? `พอร์ต ${i.port} · ${((i.last_admitted_age_ms ?? 0) / 1000).toFixed(1)} วิ`
      : i.state === "listening"
        ? `พอร์ต ${i.port} · ยังไม่มีข้อมูลจากเกม (เปิด Dota 2 ใหม่หลังตั้งค่า)`
        : reason(i.reason);

  const own = snap?.own?.fields;
  const phaseTh: Record<RuntimeStatus["session"]["phase"], string> = {
    none: "รอเข้าเกม",
    draft: "เลือกฮีโร่",
    pregame: "ก่อนเริ่ม",
    live: "กำลังเล่น",
    post_game: "จบเกม",
    unsupported: "ไม่รองรับ",
  };
  const cpu = status.resource.cpu;
  const mem = status.resource.memory;

  return (
    <main className="app">
      <header className="top">
        <h1>ภาพรวม</h1>
        <Chip tone={status.session.phase === "live" ? "ok" : "muted"} label={phaseTh[status.session.phase]} />
      </header>

      <section className="card" aria-labelledby="ready-h">
        <div className="card-head">
          <h2 id="ready-h">ความพร้อม</h2>
          <span className="muted small">แต่ละส่วนแยกกัน — ส่วนหนึ่งพร้อมไม่ได้แปลว่าทั้งหมดพร้อม</span>
        </div>
        <div role="table" aria-label="ความพร้อม">
          <Row
            name="รับข้อมูลเกม"
            chip={intakeChip[i.state]}
            detail={intakeDetail}
            action={
              i.state === "not_configured" || i.state === "unavailable" ? (
                <button type="button" onClick={runSetup} disabled={busy}>
                  ตั้งค่าการเชื่อมต่อเกม
                </button>
              ) : undefined
            }
          />
          <Row
            name="ยืนยันผู้เล่น"
            chip={
              status.ownership.state === "verified" ? <Chip tone="ok" label="ยืนยันแล้ว" />
              : status.ownership.state === "unknown" ? <Chip tone="muted" label="— ยังไม่ทราบ" />
              : <Chip tone="warn" label="ยืนยันไม่ได้" />
            }
            detail={status.ownership.state === "unknown" ? "ตรวจกับบัญชี Steam ที่ล็อกอินเมื่อเข้าเกม" : reason(status.ownership.reason)}
          />
          <Row name="แมตช์" chip={<Chip tone="muted" label={phaseTh[status.session.phase]} />} detail={status.session.session_epoch ? `เซสชัน #${status.session.session_epoch}` : "ยังไม่มีแมตช์"} />
          <Row name="โหมดเกม" chip={<Chip tone="stale" label="ยืนยันไม่ได้" />} detail="ข้อมูลจากเกมไม่ระบุโหมด" />
          <Row name="เสียงเตือนสำคัญ" chip={<Chip tone="warn" label="ยังไม่พร้อม" />} detail={reason(status.media.reason)} />
          <Row name="การมองเห็นแผนที่" chip={<Chip tone="muted" label="ใช้ไม่ได้" />} detail={reason(status.capture.reason)} />
          <Row
            name="ทรัพยากรเครื่อง"
            chip={cpu.state === "unavailable" ? <Chip tone="muted" label="วัดไม่ได้" /> : cpu.throttle === "throttled" ? <Chip tone="warn" label="ลดโหลด" /> : <Chip tone="ok" label="ปกติ" />}
            detail={`CPU ${cpu.last_sample_percent ?? "—"}% / 10% · RAM ${mem.working_set_bytes !== null ? Math.round(mem.working_set_bytes / 1e6) : "—"} MB / 900 MB`}
          />
          <Row name="บัญชี" chip={<Chip tone="muted" label="ไม่ได้ใช้" />} detail="ไม่บังคับ — ฟังก์ชันในเครื่องใช้ได้โดยไม่ต้องล็อกอิน" />
        </div>
        {setupError && <p className="error" role="alert">{setupError}</p>}
      </section>

      <section className="card" aria-labelledby="own-h">
        <div className="card-head">
          <h2 id="own-h">ฮีโร่ของคุณ</h2>
          <span className="muted small">
            {snap?.availability === "fresh" ? `สด · ${((snap.source.age_ms ?? 0) / 1000).toFixed(1)} วิ` : snap?.availability === "stale" ? "ข้อมูลเก่า" : "ยังไม่มีข้อมูล"}
          </span>
        </div>
        <dl className={`vitals ${snap?.availability === "stale" ? "is-stale" : ""}`}>
          <div><dt>ฮีโร่</dt><dd>{own?.hero_id.value?.replace("npc_dota_hero_", "") ?? "—"}</dd></div>
          <div><dt>เลเวล</dt><dd className="mono">{num(own?.level)}</dd></div>
          <div><dt>HP</dt><dd className="mono">{num(own?.hp)} / {num(own?.max_hp)}</dd></div>
          <div><dt>มานา</dt><dd className="mono">{own?.mana_percent.value !== null && own ? `${own.mana_percent.value}%` : "—"}</dd></div>
          <div><dt>สถานะ</dt><dd>{own?.alive.value === true ? "มีชีวิต" : own?.alive.value === false ? `ตาย · เกิดใหม่ใน ${num(own?.respawn_seconds)} วิ` : "—"}</dd></div>
        </dl>
      </section>
    </main>
  );
}
