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
