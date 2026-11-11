import { useState } from "react";
import { OWNED_CONFIG_FILE, reason, seconds } from "./copy";
import type { RuntimeStatus } from "./types";

type Action = "setup" | "replace";

interface Step {
  tone: "action" | "info" | "warn";
  title: string;
  body: string;
  action?: { kind: Action; label: string };
}

const SETUP_DISCLOSURE =
  `เมื่อกดปุ่ม Kohbor จะสร้างไฟล์ ${OWNED_CONFIG_FILE} หนึ่งไฟล์ในโฟลเดอร์ตั้งค่าของ Dota 2 ` +
  "เพื่อให้เกมส่งข้อมูลฮีโร่ของคุณมาที่เครื่องนี้ (127.0.0.1) ไม่แก้หรือลบไฟล์อื่น และข้อมูลไม่ออกนอกเครื่อง";

/** The single most useful next step for the current state, or null when nothing is pending. */
export function nextStep(status: RuntimeStatus): Step | null {
  const i = status.intake;
  switch (i.state) {
    case "not_configured":
      return {
        tone: "action",
        title: "เริ่มต้น: เชื่อมต่อ Kohbor กับ Dota 2",
        body: SETUP_DISCLOSURE,
        action: { kind: "setup", label: "ตั้งค่าการเชื่อมต่อเกม" },
      };
    case "unavailable":
      switch (i.reason) {
        case "GSI_CONFIG_CONFLICT":
          return {
            tone: "warn",
            title: "ไฟล์ตั้งค่าของ Kohbor ถูกแก้จากภายนอก",
            body: `ไฟล์ ${OWNED_CONFIG_FILE} ในโฟลเดอร์ Dota 2 ไม่ตรงกับที่ Kohbor เขียนไว้ จึงยังไม่เขียนทับ ถ้าต้องการให้ Kohbor ใช้งานได้ ให้แทนที่ไฟล์นี้`,
            action: { kind: "replace", label: "แทนที่ไฟล์ตั้งค่า…" },
          };
        case "GSI_GAME_NOT_FOUND":
          return {
            tone: "warn",
            title: "ไม่พบ Dota 2",
            body: "Kohbor มองหาเกมจากคลัง Steam ของบัญชี Windows นี้ และต้องพบเพียงที่เดียว ติดตั้งหรือเปิด Steam ให้เรียบร้อยแล้วลองอีกครั้ง — การเลือกโฟลเดอร์เกมเองยังไม่มีในบิลด์นี้",
            action: { kind: "setup", label: "ลองอีกครั้ง" },
          };
        case "GSI_PORT_UNAVAILABLE":
          return {
            tone: "warn",
            title: "พอร์ตรับข้อมูลถูกโปรแกรมอื่นใช้อยู่",
            body: "ปิดโปรแกรมที่ใช้พอร์ตนั้นแล้วกดลองอีกครั้ง ถ้าพอร์ตยังไม่ว่าง Kohbor จะย้ายไปพอร์ตอื่นและเขียนไฟล์ตั้งค่าใหม่ ซึ่งต้องปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง",
            action: { kind: "setup", label: "ลองอีกครั้ง" },
          };
        case "GSI_PORT_EXHAUSTED":
          return {
            tone: "warn",
            title: "พอร์ต 3000–3010 ถูกใช้หมด",
            body: "ปิดโปรแกรมที่ใช้พอร์ตในช่วงนี้อย่างน้อยหนึ่งโปรแกรมแล้วลองอีกครั้ง ยังไม่มีการเขียนไฟล์ตั้งค่า",
            action: { kind: "setup", label: "ลองอีกครั้ง" },
          };
        case "GSI_CONFIG_WRITE_FAILED":
          return {
            tone: "warn",
            title: "เขียนไฟล์ตั้งค่าไม่ได้",
            body: "ตรวจสิทธิ์การเขียนของโฟลเดอร์ Dota 2 และพื้นที่ว่างในดิสก์ แล้วลองอีกครั้ง ไฟล์เดิมยังอยู่ครบ",
            action: { kind: "setup", label: "ลองอีกครั้ง" },
          };
        case "GSI_SECRET_UNAVAILABLE":
          return {
            tone: "warn",
            title: "เข้าถึงที่เก็บความลับของ Windows ไม่ได้",
            body: "Kohbor เก็บโทเค็นของการเชื่อมต่อไว้กับบัญชี Windows ของคุณ ออกจากระบบ Windows แล้วเข้าใหม่ จากนั้นเปิด Kohbor อีกครั้ง ส่วนอื่นของแอปยังทำงานตามปกติ",
          };
        default:
          return { tone: "warn", title: "รับข้อมูลเกมไม่ได้", body: reason(i.reason) };
      }
    case "listening":
      return {
        tone: "info",
        title: "ปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง",
        body: `Kohbor พร้อมรับข้อมูลที่พอร์ต ${i.port ?? "—"} แล้ว แต่ยังไม่มีข้อมูลเข้ามา เกมอ่านไฟล์ตั้งค่าตอนเปิดเท่านั้น ถ้าเกมเปิดค้างอยู่ก่อนตั้งค่าให้ปิดแล้วเปิดใหม่`,
      };
    case "stale":
      return {
        tone: "info",
        title: "ไม่ได้รับข้อมูลจากเกมแล้ว",
        body: `ข้อมูลล่าสุดมาถึงเมื่อ ${seconds(i.last_admitted_age_ms)} ที่แล้ว ค่าที่แสดงอยู่เป็นค่าเก่า ถ้าปิดเกมไปแล้วไม่ต้องทำอะไร`,
      };
    case "receiving":
      if (status.ownership.state === "unverified" || status.ownership.state === "mismatch") {
        return {
          tone: "warn",
          title: "ยืนยันไม่ได้ว่าข้อมูลนี้เป็นของคุณ",
          body: "Kohbor แสดงค่าฮีโร่เฉพาะเมื่อบัญชี Steam ที่ล็อกอินตรงกับผู้เล่นในเกม ล็อกอิน Steam ให้เรียบร้อยแล้วปิดและเปิด Dota 2 ใหม่",
        };
      }
      return null;
    default:
      return null;
  }
}

interface Props {
  status: RuntimeStatus;
  setup: (replace: boolean) => Promise<string | null>;
}

export function Guide({ status, setup }: Props) {
  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);
  const step = nextStep(status);
  if (!step) return null;

  const run = async (replace: boolean) => {
    setBusy(true);
    setFailed(null);
    setConfirming(false);
    setFailed(await setup(replace));
    setBusy(false);
  };
  const action = step.action;
  // The row already carries the reason once the host reports it as its state.
  const showFailure = failed !== null && failed !== status.intake.reason;

  const replacing = action?.kind === "replace" && confirming;

  return (
    <section className={`guide guide-${step.tone}`} aria-labelledby="guide-h">
      <div className="guide-text">
        <h2 id="guide-h">{step.title}</h2>
        {/* The confirmation takes the place of the explanation, so the page keeps its height. */}
        <p>
          {replacing
            ? `ยืนยันการแทนที่: จะเขียนทับ ${OWNED_CONFIG_FILE} และออกโทเค็นใหม่ สิ่งที่แก้ไว้ในไฟล์นี้จะหายไป และต้องปิดแล้วเปิด Dota 2 ใหม่หนึ่งครั้ง`
            : step.body}
        </p>
        {showFailure && (
          <p className="error" role="alert">
            {reason(failed)}
          </p>
        )}
      </div>
      {action && !replacing && (
        <button
          type="button"
          className="primary"
          disabled={busy}
          aria-busy={busy}
          onClick={() => (action.kind === "replace" ? setConfirming(true) : void run(false))}
        >
          {busy ? "กำลังดำเนินการ…" : action.label}
        </button>
      )}
      {replacing && (
        <div className="guide-actions" role="group" aria-label="ยืนยันการแทนที่ไฟล์ตั้งค่า">
          <button type="button" className="primary" disabled={busy} onClick={() => void run(true)}>
            ยืนยันแทนที่
          </button>
          <button type="button" disabled={busy} onClick={() => setConfirming(false)}>
            ยกเลิก
          </button>
        </div>
      )}
    </section>
  );
}
