import assert from "node:assert/strict";
import { test } from "node:test";
// @ts-ignore Node's built-in TypeScript loader requires the explicit source extension.
import { ALLY_SLOT_IDS, DECK_PLACEHOLDERS, ENEMY_SLOT_IDS, deckCopy } from "../src/deck/copy.ts";

test("ally placeholders say the game supplies only the own hero", () => {
  assert.deepEqual(ALLY_SLOT_IDS, ["A1", "A2", "A3", "A4"]);
  assert.equal(DECK_PLACEHOLDERS.allies.th, "— ยังไม่มีข้อมูล");
  assert.equal(DECK_PLACEHOLDERS.allySource.th, "เกมให้ข้อมูลเฉพาะตัวคุณ");
});

test("enemy placeholders do not invent identities or unseen details", () => {
  assert.deepEqual(ENEMY_SLOT_IDS, ["E1", "E2", "E3", "E4", "E5"]);
  assert.equal(DECK_PLACEHOLDERS.enemies.th, "— ยังไม่เห็น · ต้องใช้ภาพ");
  assert.equal(deckCopy("th").enemyCaption, "ศัตรู: ต้องใช้ภาพจากแผนที่ — ยังไม่เปิดใช้ในรุ่นนี้");
});

test("later-slice data and event feed stay explicit placeholders", () => {
  assert.equal(DECK_PLACEHOLDERS.emptyEvents.th, "ยังไม่มีเหตุการณ์");
  assert.equal(DECK_PLACEHOLDERS.unavailableInVersion.th, "ยังไม่มีข้อมูลในรุ่นนี้");
  assert.equal(DECK_PLACEHOLDERS.noPack.th, "ไม่มีแพ็ก");
  assert.equal(deckCopy("th").oldData, "ข้อมูลเก่า");
  assert.equal(deckCopy("en").oldData, "Old data");
});
