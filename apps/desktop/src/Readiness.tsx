import type { ReactElement } from "react";
import { PHASE_TH, reason, seconds } from "./copy";
import type { RuntimeStatus } from "./types";

/**
 * Honest-state encodings (contracts/design-tokens.json): each tone differs by
 * glyph, border style and words, never by colour alone.
 */
export type Tone = "fresh" | "stale" | "unknown" | "off" | "warn";

export function Chip({ tone, label }: { tone: Tone; label: string }) {
  return (
    <span className={`chip chip-${tone}`}>
      <span className="chip-glyph" aria-hidden="true" />
      {label}
    </span>
  );
}

function Row({ name, chip, detail }: { name: string; chip: ReactElement; detail: string }) {
  return (
    <div className="row" role="row">
      <span className="row-name" role="rowheader">
        {name}
      </span>
      <span role="cell">{chip}</span>
      <span className="row-detail" role="cell">
        {detail}
      </span>
    </div>
  );
}

function megabytes(bytes: number | null): string {
  return bytes === null ? "—" : String(Math.round(bytes / 1e6));
}

export function Readiness({ status }: { status: RuntimeStatus }) {
  const i = status.intake;
  const receiving = i.state === "receiving";

  const intakeChip: Record<RuntimeStatus["intake"]["state"], ReactElement> = {
    not_configured: <Chip tone="unknown" label="ยังไม่ตั้งค่า" />,
    starting: <Chip tone="unknown" label="กำลังเริ่ม" />,
    listening: <Chip tone="unknown" label="รอข้อมูลจากเกม" />,
    receiving: <Chip tone="fresh" label="สด" />,
    stale: <Chip tone="stale" label="ข้อมูลเก่า" />,
    unavailable: <Chip tone="warn" label="ใช้ไม่ได้" />,
  };
  const intakeDetail =
    i.state === "receiving" || i.state === "stale"
      ? `พอร์ต ${i.port ?? "—"} · ข้อมูลล่าสุด ${seconds(i.last_admitted_age_ms)} ที่แล้ว`
      : i.state === "listening"
        ? `พอร์ต ${i.port ?? "—"} · ยังไม่มีข้อมูลเข้ามา`
        : reason(i.reason);

  // Ownership and match describe the last admitted payload. Once the source is
  // not live they are shown as old, never as a current affirmative state.
  const own = status.ownership;
  const ownChip =
    own.state === "unknown" ? (
      <Chip tone="unknown" label="ยังไม่ทราบ" />
    ) : own.state !== "verified" ? (
      <Chip tone="warn" label="ยืนยันไม่ได้" />
    ) : receiving ? (
      <Chip tone="fresh" label="ยืนยันแล้ว" />
    ) : (
      <Chip tone="stale" label="ยืนยันแล้ว (ข้อมูลเก่า)" />
    );
  const ownDetail =
    own.state === "unknown"
      ? "ตรวจกับบัญชี Steam ที่ล็อกอินเมื่อเข้าเกม"
      : own.state === "verified"
        ? "บัญชี Steam ที่ล็อกอินตรงกับผู้เล่นในเกม"
        : reason(own.reason ?? "OWNER_UNVERIFIED");

  const phase = status.session.phase;
  const matchChip =
    phase === "none" ? (
      <Chip tone="unknown" label={PHASE_TH.none} />
    ) : phase === "unsupported" ? (
      <Chip tone="off" label={PHASE_TH.unsupported} />
    ) : receiving ? (
      <Chip tone="fresh" label={PHASE_TH[phase]} />
    ) : (
      <Chip tone="stale" label={`${PHASE_TH[phase]} (ข้อมูลเก่า)`} />
    );
  const matchDetail =
    status.session.session_epoch !== null ? `เซสชัน #${status.session.session_epoch}` : "ยังไม่มีแมตช์";

  const mode = status.mode;
  const modeChip =
    mode.state === "identified" && mode.value ? (
      <Chip tone="fresh" label={mode.value} />
    ) : mode.state === "unsupported" ? (
      <Chip tone="off" label="ไม่รองรับ" />
    ) : (
      <Chip tone="stale" label="ยืนยันไม่ได้" />
    );
  const modeDetail = mode.state === "unverified" ? "ข้อมูลจากเกมไม่ระบุโหมด" : "";

  const media = status.media;
  const mediaChip =
    media.state === "ready" ? (
      <Chip tone="fresh" label="พร้อม" />
    ) : media.state === "unavailable" ? (
      <Chip tone="off" label="ใช้ไม่ได้" />
    ) : (
      <Chip tone="warn" label="ยังไม่พร้อม" />
    );

  const capture = status.capture;
  const captureChip =
    capture.state === "available" ? (
      <Chip tone="fresh" label="พร้อม" />
    ) : capture.state === "unsupported" ? (
      <Chip tone="off" label="ไม่รองรับ" />
    ) : (
      <Chip tone="off" label="ใช้ไม่ได้" />
    );

  const cpu = status.resource.cpu;
  const mem = status.resource.memory;
  const resourceChip =
    cpu.state === "unavailable" ? (
      <Chip tone="unknown" label="วัดไม่ได้" />
    ) : mem.state === "over_budget" ? (
      <Chip tone="warn" label="RAM เกินงบ" />
    ) : cpu.throttle === "throttled" ? (
      <Chip tone="warn" label="ลดโหลด" />
    ) : (
      <Chip tone="fresh" label="ปกติ" />
    );
  const resourceDetail = `CPU ${cpu.last_sample_percent ?? "—"}% จากเพดาน 10% · RAM ${megabytes(mem.working_set_bytes)} MB จากงบ 900 MB`;

  const account = status.account.state;
  const accountChip =
    account === "signed_in" ? (
      <Chip tone="fresh" label="ล็อกอินแล้ว" />
    ) : account === "signed_out" ? (
      <Chip tone="unknown" label="ยังไม่ล็อกอิน" />
    ) : (
      <Chip tone="off" label="ไม่ได้ใช้" />
    );

  return (
    <section className="card" aria-labelledby="ready-h">
      <div className="card-head">
        <h2 id="ready-h">ความพร้อม</h2>
        <span className="muted small">แต่ละส่วนแยกกัน — ส่วนหนึ่งพร้อมไม่ได้แปลว่าทั้งหมดพร้อม</span>
      </div>
      <div role="table" aria-labelledby="ready-h">
        <Row name="รับข้อมูลเกม" chip={intakeChip[i.state]} detail={intakeDetail} />
        <Row name="ยืนยันผู้เล่น" chip={ownChip} detail={ownDetail} />
        <Row name="แมตช์" chip={matchChip} detail={matchDetail} />
        <Row name="โหมดเกม" chip={modeChip} detail={modeDetail} />
        <Row name="เสียงเตือนสำคัญ" chip={mediaChip} detail={reason(media.reason)} />
        <Row name="การมองเห็นแผนที่" chip={captureChip} detail={reason(capture.reason)} />
        <Row name="ทรัพยากรเครื่อง" chip={resourceChip} detail={resourceDetail} />
        <Row name="บัญชี" chip={accountChip} detail="ไม่บังคับ — ฟังก์ชันในเครื่องใช้ได้โดยไม่ต้องล็อกอิน" />
      </div>
    </section>
  );
}
