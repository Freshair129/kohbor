import assert from "node:assert/strict";
import { test } from "node:test";
import { matchViewerCopy } from "../src/replay/matchViewerCopy.mjs";

test("viewer runtime copy is complete in both locales and English values contain no Thai text", () => {
  const thai = matchViewerCopy.th.viewerStrings;
  const english = matchViewerCopy.en.viewerStrings;
  assert.deepEqual(Object.keys(thai).sort(), Object.keys(english).sort());
  assert.ok(Object.entries(english).every(([, value]) => !/[\u0e00-\u0e7f]/u.test(value)));
  assert.equal(thai["HP ลดเร็ว"], "HP ลดเร็ว");
  assert.equal(english["HP ลดเร็ว"], "Fast HP drop");
  assert.equal(english["เลือกฮีโร่จากรายชื่อด้านบนเพื่อแสดงไทม์ไลน์"], "Select heroes above to show the timeline");
  assert.match(matchViewerCopy.en.ultimateUnavailable, /ultimate detection is unavailable/i);
  assert.match(matchViewerCopy.en.bkbOnlyLabel, /BKB use only/i);
});

test("replay parsing labels use one term and localise failure reasons", () => {
  assert.equal(matchViewerCopy.th.parse, "แยกรีเพลย์");
  assert.equal(matchViewerCopy.th.reparse, "แยกรีเพลย์อีกครั้ง");
  assert.equal(matchViewerCopy.th.states.available, "ยังไม่แยก");
  assert.equal(matchViewerCopy.th.states.queued, "รอคิว");
  assert.equal(matchViewerCopy.th.states.parsing, "กำลังแยก");
  assert.equal(matchViewerCopy.th.states.ready, "พร้อมดู");
  assert.equal(matchViewerCopy.th.states.failed("engine_timeout"), "แยกไม่สำเร็จ · ใช้เวลานานเกินกำหนด");
  assert.equal(matchViewerCopy.th.states.failed("match_active"), "แยกไม่สำเร็จ · หยุดไว้เพราะเริ่มแมตช์ใหม่");
  assert.equal(matchViewerCopy.th.states.failed("unrecognised_code"), "แยกไม่สำเร็จ · ไม่ทราบสาเหตุ");
  assert.equal(matchViewerCopy.en.states.failed("engine_timeout"), "Could not parse · timed out");
});
