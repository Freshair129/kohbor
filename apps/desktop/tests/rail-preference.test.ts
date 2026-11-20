import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  DEFAULT_RAIL_STATE,
  RAIL_STORAGE_KEY,
  isRailState,
  nextRailState,
  persistRailState,
  readRailState,
  shouldToggleRailFromShortcut,
} from "../src/shell/railPreference.ts";

function keyEvent(overrides: Partial<KeyboardEvent> = {}): Pick<KeyboardEvent, "key" | "ctrlKey" | "shiftKey" | "altKey" | "metaKey"> {
  return { key: "b", ctrlKey: true, shiftKey: false, altKey: false, metaKey: false, ...overrides };
}

test("rail preference validates saved values and defaults to collapsed", () => {
  assert.equal(DEFAULT_RAIL_STATE, "collapsed");
  assert.equal(isRailState("collapsed"), true);
  assert.equal(isRailState("expanded"), true);
  assert.equal(isRailState("peek"), false);
  assert.equal(nextRailState("collapsed"), "expanded");
  assert.equal(nextRailState("expanded"), "collapsed");
  assert.equal(readRailState({ getItem: key => key === RAIL_STORAGE_KEY ? "expanded" : null }), "expanded");
  assert.equal(readRailState({ getItem: () => "invalid" }), "collapsed");
  assert.equal(readRailState({ getItem: () => null }), "collapsed");
  assert.equal(readRailState({ getItem: () => { throw new Error("storage blocked"); } }), "collapsed");
});

test("rail preference keeps the in-memory change when localStorage writes fail", () => {
  let savedKey = "";
  let savedValue = "";
  const storage = {
    setItem(key: string, value: string) { savedKey = key; savedValue = value; },
  };
  assert.equal(persistRailState(storage, "expanded"), "expanded");
  assert.equal(savedKey, RAIL_STORAGE_KEY);
  assert.equal(savedValue, "expanded");
  assert.equal(persistRailState({ setItem: () => { throw new Error("quota"); } }, "collapsed"), "collapsed");
});

test("Ctrl+B toggles the rail only without extra modifiers or an open modal/drawer", () => {
  assert.equal(shouldToggleRailFromShortcut(keyEvent(), false), true);
  assert.equal(shouldToggleRailFromShortcut(keyEvent({ key: "B" }), false), true);
  assert.equal(shouldToggleRailFromShortcut(keyEvent(), true), false);
  assert.equal(shouldToggleRailFromShortcut(keyEvent({ ctrlKey: false }), false), false);
  assert.equal(shouldToggleRailFromShortcut(keyEvent({ shiftKey: true }), false), false);
  assert.equal(shouldToggleRailFromShortcut(keyEvent({ altKey: true }), false), false);
  assert.equal(shouldToggleRailFromShortcut(keyEvent({ metaKey: true }), false), false);
  assert.equal(shouldToggleRailFromShortcut(keyEvent({ key: "c" }), false), false);

  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /document\.addEventListener\("keydown", onKeyDown\)/);
  assert.match(app, /document\.querySelector\('dialog\[open\], \[aria-modal="true"\]'/);
  assert.match(app, /shouldToggleRailFromShortcut\(event, modalOrDrawerOpen\)/);
  assert.match(app, /event\.preventDefault\(\);\s*rail\.toggle\(\)/);
});

test("rail markup exposes both states, sections, unsupported store state and accessible toggle names", () => {
  const rail = readFileSync(new URL("../src/shell/RailLeft.tsx", import.meta.url), "utf8");
  const game = readFileSync(new URL("../src/shell/GameSelector.tsx", import.meta.url), "utf8");
  assert.match(rail, /id="rail"/);
  assert.match(rail, /aria-expanded=\{expanded\}\s+aria-controls="rail"/);
  assert.match(rail, /"ขยายแถบเมนู \(Ctrl\+B\)"/);
  assert.match(rail, /"ย่อแถบเมนู \(Ctrl\+B\)"/);
  assert.match(rail, /"»"/);
  assert.match(rail, /"«"/);
  assert.match(rail, /"เมนูหลัก"/);
  assert.match(rail, /"ระบบ"/);
  assert.match(rail, /"ยังไม่เปิด"/);
  assert.match(rail, /data-destination=\{id\}/);
  assert.match(game, /เกมที่ใช้อยู่ · /);
  assert.match(game, /gameStatusLabel\(items\[0\]!\.status, locale\)/);
  assert.match(game, /aria-controls="shell-game-menu"/);
  assert.match(game, /shell-game-expanded-name/);
});
