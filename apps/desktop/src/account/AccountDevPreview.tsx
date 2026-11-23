import { useState } from "react";
import { FakeAccountPort } from "./FakeAccountPort";
import { AccountPage } from "./AccountPages";
import "./account.css";

/** Explicit development-only entry point. It is never used by the normal app route. */
export function AccountDevPreview({ enabled = false, locale = "en", back = () => {} }: {
  enabled?: boolean;
  locale?: "th" | "en";
  back?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [port] = useState(() => new FakeAccountPort({ state: "signed_out", serviceOpen: true }));
  if (!import.meta.env.DEV || !enabled) return null;
  if (!open) return <button type="button" className="account-dev-launch" onClick={() => setOpen(true)}>
    {locale === "th" ? "เปิดตัวอย่างบัญชีสำหรับพัฒนา" : "Open account developer preview"}
  </button>;

  const scenario = (state: "signed_out" | "needs_display_name" | "signed_in_online" | "signed_in_offline" | "service_unavailable" | "paused_new_device" | "deleting") => {
    const common = {
      displayName: state === "signed_in_online" || state === "signed_in_offline" || state === "paused_new_device" || state === "deleting" ? "Mali · ตัวอย่าง" : null,
      friendCode: state === "signed_in_online" || state === "signed_in_offline" || state === "paused_new_device" || state === "deleting"
        ? { value: "KB-DEMO-0001", freshness: state === "signed_in_offline" ? "cached" as const : "live" as const, isExample: true }
        : { value: null, freshness: "unknown" as const, isExample: false },
      sessions: state === "signed_in_online" ? [
        { thisDevice: true, signedInOn: "2026-11-22T00:00:00.000Z" },
        { thisDevice: false, signedInOn: "2026-11-21T00:00:00.000Z" },
      ] : null,
      liveMatch: false,
      serviceOpen: true,
    };
    port.setScenario({ ...common, state, firstSignIn: false, nameError: null, notice: null, deleteStep: null, deleteError: null });
  };

  return <div className="account-dev-preview">
    <div className="account-dev-banner" role="note">{locale === "th" ? "ตัวอย่างสำหรับพัฒนา · การทำงานทั้งหมดอยู่ในหน่วยความจำ" : "Developer preview · all actions stay in memory"}</div>
    <div className="account-dev-scenarios" aria-label={locale === "th" ? "สถานะตัวอย่าง" : "Preview states"}>
      <button type="button" onClick={() => scenario("signed_out")}>{locale === "th" ? "ผู้เยี่ยมชม" : "Guest"}</button>
      <button type="button" onClick={() => scenario("needs_display_name")}>{locale === "th" ? "ตั้งชื่อ" : "Name"}</button>
      <button type="button" onClick={() => { scenario("signed_in_online"); port.setScenario({ state: "signed_in_online", firstSignIn: true, displayName: "Mali · ตัวอย่าง", friendCode: { value: "KB-DEMO-0001", freshness: "live", isExample: true } }); }}>{locale === "th" ? "พร้อมใช้" : "Ready"}</button>
      <button type="button" onClick={() => scenario("signed_in_offline")}>{locale === "th" ? "ออฟไลน์" : "Offline"}</button>
      <button type="button" onClick={() => scenario("service_unavailable")}>{locale === "th" ? "บริการขัดข้อง" : "Service unavailable"}</button>
      <button type="button" onClick={() => scenario("paused_new_device")}>{locale === "th" ? "เครื่องใหม่" : "New device"}</button>
      <button type="button" onClick={() => scenario("deleting")}>{locale === "th" ? "ลบบัญชี" : "Delete"}</button>
      <button type="button" onClick={() => port.setScenario({ state: "browser_handoff", handoffPurpose: "sign_in", handoffDeadline: Date.now() + 30_000 })}>{locale === "th" ? "รอเบราว์เซอร์" : "Browser wait"}</button>
      <button type="button" onClick={() => port.setScenario({ state: "browser_handoff", handoffPurpose: "sign_in", handoffDeadline: Date.now() - 1 })}>{locale === "th" ? "หมดเวลา" : "Timed out"}</button>
      <button type="button" onClick={() => port.setScenario({ state: "signed_out", notice: "provider_declined" })}>{locale === "th" ? "ยกเลิกในเบราว์เซอร์" : "Declined"}</button>
      <button type="button" onClick={() => port.setScenario({ state: "signed_out", rateLimitDeadline: Date.now() + 30_000 })}>{locale === "th" ? "จำกัดการลอง" : "Rate limited"}</button>
      <button type="button" onClick={() => port.setScenario({ state: "deleting", deleteStep: "review", deleteError: "mismatch", displayName: "Mali · ตัวอย่าง" })}>{locale === "th" ? "บัญชีไม่ตรงกัน" : "Account mismatch"}</button>
      <button type="button" onClick={() => port.completeFreshSignIn(true)}>{locale === "th" ? "ยืนยันการเข้าสู่ระบบใหม่" : "Complete fresh sign-in"}</button>
      <button type="button" onClick={() => port.completeFreshSignIn(false)}>{locale === "th" ? "เลือกบัญชีอื่น" : "Choose another account"}</button>
    </div>
    <AccountPage account={port.get()} port={port} locale={locale} liveMatch={port.getState().liveMatch} back={back} />
  </div>;
}
