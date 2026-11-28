import assert from "node:assert/strict";
import test from "node:test";
import { drawCenteredPortrait } from "../src/replay/matchViewer.mjs";

test("hero portrait crop uses the centered square from a native 128x72 image", () => {
  const calls = [];
  const context = { drawImage: (...args) => calls.push(args) };
  const image = { naturalWidth: 128, naturalHeight: 72 };

  drawCenteredPortrait(context, image, 10, 20, 28, 28);

  assert.deepEqual(calls, [[image, 28, 0, 72, 72, 10, 20, 28, 28]]);
});

test("hero portrait crop retains the centered 144px square for 256x144 placeholders", () => {
  const calls = [];
  const context = { drawImage: (...args) => calls.push(args) };
  const image = { naturalWidth: 256, naturalHeight: 144 };

  drawCenteredPortrait(context, image, 10, 20, 28, 28);

  assert.deepEqual(calls, [[image, 56, 0, 144, 144, 10, 20, 28, 28]]);
});
