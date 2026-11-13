import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const preview = await import("../src/live/preview.ts");
const { createFrameBuffer, parseFrame } = await import("../src/live/frame.ts");
const { parseCaptureStatus } = await import("../src/live/state.ts");

function frame(sequence = 1, width = 1, height = 1, generation = 7) {
  return createFrameBuffer({ flags: 0, width, height, stride: width * 4, session_epoch: 1,
    calibration_generation: generation, frame_seq: sequence, age_ms: 12 },
  new Uint8Array(width * height * 4).fill(255));
}

test("preview samples monotonic counters over real elapsed seconds with a fake clock", () => {
  let now = 0;
  const pipeline = preview.createPreviewPipeline({ generation: 7, now: () => now,
    paint: () => { now += 2; return true; } });
  assert.equal(pipeline.sample().received_fps, null);
  pipeline.receive(frame());
  pipeline.receive(frame(2));
  now = 1_000;
  assert.deepEqual(pipeline.sample(), {
    received_fps: 2, painted_fps: 2, dropped_count: 0, parse_paint_ms_mean: 2,
    received_total: 2, painted_total: 2,
  });
  pipeline.receive(frame(3));
  now = 3_000;
  assert.equal(pipeline.sample().received_fps, 0.5);
  now = 4_000;
  const idle = pipeline.sample();
  assert.equal(idle.painted_fps, 0);
  assert.equal(idle.parse_paint_ms_mean, null);
});

test("preview paints immediately without a scheduler and counts only parsed messages", () => {
  let now = 0;
  const painted: number[] = [];
  const pipeline = preview.createPreviewPipeline({ generation: 7, now: () => now,
    paint: (value) => { painted.push(value.header.frame_seq); return true; } });
  pipeline.receive(frame(1));
  assert.deepEqual(painted, [1]);
  pipeline.receive(new ArrayBuffer(2));
  pipeline.receive(frame(2, 1, 1, 8));
  now = 1_000;
  const metrics = pipeline.sample();
  assert.equal(metrics.received_fps, 2);
  assert.equal(metrics.painted_fps, 1);
  assert.equal(metrics.dropped_count, 1);
});

test("an unpainted pending frame is replaced by the newest frame", () => {
  let ready = false;
  const painted: number[] = [];
  const pipeline = preview.createPreviewPipeline({ generation: 7, now: () => 0,
    paint: (value) => {
      if (!ready) return false;
      painted.push(value.header.frame_seq);
      return true;
    } });
  pipeline.receive(frame(1));
  ready = true;
  pipeline.receive(frame(2));
  assert.deepEqual(painted, [2]);
  assert.equal(pipeline.totals().dropped_count, 1);
});

test("messages arriving while painting keep only the latest pending frame", () => {
  const painted: number[] = [];
  const pipeline = preview.createPreviewPipeline({ generation: 7, now: () => 0,
    paint: (value) => {
      painted.push(value.header.frame_seq);
      if (value.header.frame_seq === 1) {
        pipeline.receive(frame(2));
        pipeline.receive(frame(3));
      }
      return true;
    } });
  pipeline.receive(frame(1));
  assert.deepEqual(painted, [1, 3]);
  assert.equal(pipeline.totals().dropped_count, 1);
});

test("canvas drawing reuses the same ImageData and backing buffer until dimensions change", () => {
  let allocations = 0;
  const images: object[] = [];
  const context = {
    createImageData: (width: number, height: number) => {
      allocations += 1;
      return { width, height, data: new Uint8ClampedArray(width * height * 4) };
    },
    putImageData: (image: object) => images.push(image),
  };
  const canvas = { width: 1, height: 1, getContext: () => context };
  const draw = preview.createCanvasPainter(() => canvas);
  const first = frame();
  new Uint8Array(first, 32).set([3, 2, 1, 255]);
  assert.equal(draw(parseFrame(first)!), true);
  assert.deepEqual([...images[0]!.data], [1, 2, 3, 255]);
  const backing = images[0]!.data.buffer;
  assert.equal(draw(parseFrame(frame(2))!), true);
  assert.equal(allocations, 1);
  assert.equal(images[0], images[1]);
  assert.equal(images[1]!.data.buffer, backing);
  assert.deepEqual([...images[0]!.data], [255, 255, 255, 255]);
  draw(parseFrame(frame(3, 2, 1))!);
  assert.equal(allocations, 2);
  assert.notEqual(images[1], images[2]);
});

test("host sent and grab measurements are parsed independently and remain unknown when absent", () => {
  const payload = { phase: "live", fps_measured: 30, frame_age_ms: 12, rect_profile: "p1080-default-v0",
    calibration_generation: 7, auto_confirmed: true };
  const legacy = parseCaptureStatus(payload);
  assert.ok(legacy);
  assert.equal(legacy.grab_fps, null);
  assert.equal(legacy.sent_total, null);
  assert.equal(legacy.grab_total, null);
  const current = parseCaptureStatus({ ...payload, grab_fps: 58, sent_total: 60, grab_total: 120 });
  assert.ok(current);
  assert.equal(current.fps_measured, 30);
  assert.equal(current.grab_fps, 58);
  assert.equal(current.sent_total, 60);
  assert.equal(current.grab_total, 120);
  assert.equal(parseCaptureStatus({ ...payload, sent_total: 1.5 }), null);
  assert.equal(parseCaptureStatus({ ...payload, grab_fps: "60" }), null);
});

test("page visibility wording reports hidden state without asserting a cause", () => {
  assert.equal(preview.visibilityText("hidden", "th"), "หน้าต่างถูกซ่อนหรือถูกบัง — ภาพอาจช้า");
  assert.equal(preview.visibilityText("hidden", "en"), "window hidden or covered — preview may be slow");
  assert.equal(preview.visibilityText("visible", "th"), null);
  assert.equal(preview.visibilityText(null, "en"), null);
  assert.equal(preview.previewRateText(null, null, "th"), "ส่ง — · รับ — · วาด — fps");
  assert.equal(preview.previewRateText(30, { received_fps: 29, painted_fps: 28 }, "en"),
    "sent 30 · received 29 · painted 28 fps");
});

test("dashboard fills its panel and keeps vertical scrolling off the dashboard root", () => {
  const css = readFileSync(new URL("../src/live/live.css", import.meta.url), "utf8");
  const roots = [...css.matchAll(/\.live-tab\s*\{([^}]+)\}/g)].map((match) => match[1]!).join("\n");
  assert.match(roots, /height:\s*100%/);
  assert.match(roots, /min-height:\s*0/);
  assert.match(roots, /overflow:\s*hidden/);
  assert.doesNotMatch(roots, /overflow(?:-y)?:\s*(?:auto|scroll)/);
  assert.match(css, /\.live-map-canvas\s*\{[^}]*object-fit:\s*contain/);
  assert.match(css, /:focus-visible/);
  const component = readFileSync(new URL("../src/live/LiveTab.tsx", import.meta.url), "utf8");
  assert.match(component, /<details className="live-controls-disclosure">/);
  assert.match(component, /<details className="live-resource-disclosure">/);
  assert.doesNotMatch(component, /<details[^>]*\bopen(?:=|\s|>)/);
  const hook = readFileSync(new URL("../src/live/useMinimap.ts", import.meta.url), "utf8");
  assert.doesNotMatch(hook, /requestAnimationFrame/);
});
