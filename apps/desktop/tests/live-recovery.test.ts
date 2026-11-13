import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const recovery = await import("../src/live/captureRecovery.ts");
const { liveCopy } = await import("../src/live/copy.ts");

const reasons = [
  ["unsupported", "user_invalidated", 'คุณทำเครื่องหมายว่าแผนที่ไม่ถูกต้อง — กด "ลองใหม่" เพื่อจับภาพต่อ', 'You marked the map as wrong — press "Retry" to resume capture'],
  ["unsupported", "no_game_window", "ไม่พบหน้าต่าง Dota 2 ที่มองเห็นได้ — เปิดเกมในโหมด Borderless", "No visible Dota 2 window found — open the game in Borderless mode"],
  ["unsupported", "crop_out_of_bounds", 'กรอบที่ตั้งไว้ออกนอกจอ — กด "รีเซ็ต" หรือปรับกรอบ', 'The configured crop is outside the screen — press "Reset" or adjust the crop'],
  ["unsupported", "unsupported_resolution", "จอต้องเป็น 1920x1080 ที่สเกล 100% (รุ่นนี้)", "Display must be 1920x1080 at 100% scaling (this build)"],
  ["unsupported", "capture_disabled", "ปิดการจับภาพด้วย KOHBOR_CAPTURE=0", "Capture disabled by KOHBOR_CAPTURE=0"],
  ["calibrating", "paused", "หยุดจับภาพ", "Capture paused"],
  ["lost", "reopening", "กำลังเปิดการจับภาพใหม่", "Reopening capture"],
  ["lost", "access_lost", "การเชื่อมต่อการจับภาพขาดหาย — กำลังลองใหม่", "Capture access lost — retrying"],
  ["calibrating", "calibrating", "กำลังตรวจตำแหน่งแผนที่ — ยังไม่แสดงภาพสด", "Checking map position — live picture is hidden"],
] as const;

for (const [phase, reason, th, en] of reasons) {
  test(`capture reason ${reason} has accurate Thai and English text`, () => {
    assert.equal(recovery.phaseReasonText(phase, reason, "th"), th);
    assert.equal(recovery.phaseReasonText(phase, reason, "en"), en);
  });
}

test("unknown capture reasons retain their code instead of claiming a resolution problem", () => {
  assert.equal(recovery.phaseReasonText("unsupported", "future_<layout>", "th"), "จับภาพไม่ได้ (เหตุผล: future_<layout>)");
  assert.equal(recovery.phaseReasonText("unsupported", "future_<layout>", "en"), "Capture unavailable (reason: future_<layout>)");
});

test("unsupported without a reason never invents a resolution diagnosis", () => {
  for (const locale of ["th", "en"] as const) {
    const text = recovery.phaseReasonText("unsupported", null, locale);
    assert.notEqual(text, recovery.phaseReasonText("unsupported", "unsupported_resolution", locale));
    assert.ok(text !== null && !text.includes("1920"));
    assert.ok(recovery.phaseReasonText("unsupported", "capture_unavailable", locale, "DXGI x")?.includes("DXGI x"));
    assert.equal(recovery.phaseReasonText("live", null, locale), null);
    assert.equal(recovery.phaseReasonText(null, null, locale), null);
  }
});

test("marking wrong arms on the first click and invokes only on a second click within four seconds", () => {
  let now = 1_000;
  const first = recovery.advanceInvalidation(null, now);
  assert.deepEqual(first, { armedUntil: 5_000, invoke: false });
  now = 4_999;
  assert.equal(recovery.isInvalidationArmed(first.armedUntil, now), true);
  const second = recovery.advanceInvalidation(first.armedUntil, now);
  assert.deepEqual(second, { armedUntil: null, invoke: true });
  now += 1;
  assert.equal(recovery.advanceInvalidation(second.armedUntil, now).invoke, false);
});

test("a click at or after the four-second deadline rearms instead of invalidating", () => {
  let now = 0;
  const first = recovery.advanceInvalidation(null, now);
  now = 4_000;
  assert.equal(recovery.isInvalidationArmed(first.armedUntil, now), false);
  assert.deepEqual(recovery.advanceInvalidation(first.armedUntil, now), { armedUntil: 8_000, invoke: false });
  now = 9_000;
  assert.equal(recovery.advanceInvalidation(first.armedUntil, now).invoke, false);
});

test("recovery controls use explicit labels and a separate bottom section", () => {
  assert.equal(liveCopy("th").mapWrong, "ทำเครื่องหมายว่าแผนที่ผิด");
  assert.equal(liveCopy("en").mapWrong, "Mark map as wrong");
  assert.equal(liveCopy("th").confirmMarkWrong, "กดอีกครั้งเพื่อยืนยัน");
  assert.equal(liveCopy("th").retryCapture, "ลองใหม่");
  const source = readFileSync(new URL("../src/live/LiveTab.tsx", import.meta.url), "utf8");
  assert.ok(source.indexOf('className="live-mark-wrong-section"') > source.indexOf("text.displayRate"));
  assert.match(source, /config\?\.effective_rect/);
  const hook = readFileSync(new URL("../src/live/useCaptureConfig.ts", import.meta.url), "utf8");
  assert.match(hook, /runAction\("capture_retry"\)/);
});
