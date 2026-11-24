import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { deflateSync } from 'node:zlib';
import { classifyCrop, cropImage, decodePng, evaluateRecording, sampleStats } from './evaluate.mjs';

function pngChunk(type, data) {
  const name = Buffer.from(type, 'ascii');
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  // The decoder intentionally ignores CRC; a fixed value keeps this test encoder small.
  return Buffer.concat([length, name, data, Buffer.alloc(4)]);
}

function encodeRgbaPng(width, height, rgba) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  const raw = Buffer.alloc(height * (width * 4 + 1));
  for (let y = 0; y < height; y += 1) {
    const row = y * (width * 4 + 1);
    raw[row] = 0;
    Buffer.from(rgba).copy(raw, row + 1, y * width * 4, (y + 1) * width * 4);
  }
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk('IHDR', header),
    pngChunk('IDAT', deflateSync(raw)),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function patternImage(index, width = 4, height = 4) {
  const data = new Uint8Array(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const offset = pixel * 4;
    data[offset] = (pixel * 17 + index * 31) & 0xff;
    data[offset + 1] = (pixel * 37 + index * 53) & 0xff;
    data[offset + 2] = (pixel * 59 + index * 79) & 0xff;
    data[offset + 3] = 255;
  }
  return { width, height, data };
}

function fiveTemplates() {
  return Array.from({ length: 5 }, (_, index) => ({ hero: `hero-${index}`, image: patternImage(index) }));
}

test('decodes an 8-bit RGBA PNG into row-major RGBA pixels', () => {
  const pixels = [
    255, 0, 0, 255, 0, 255, 0, 128,
    0, 0, 255, 255, 4, 5, 6, 255,
  ];
  const image = decodePng(encodeRgbaPng(2, 2, pixels));
  assert.equal(image.width, 2);
  assert.equal(image.height, 2);
  assert.deepEqual([...image.data], pixels);
});

test('crops exact bounds and rejects an out-of-image marker box', () => {
  const image = patternImage(1, 4, 4);
  const crop = cropImage(image, [1, 1, 2, 2]);
  assert.equal(crop.width, 2);
  assert.equal(crop.height, 2);
  assert.throws(() => cropImage(image, [3, 3, 2, 2]), /outside the image/);
});

test('both methods rank the matching one-of-five template first', () => {
  const templates = fiveTemplates();
  for (const method of ['template_match', 'color_histogram_plus_template']) {
    const result = classifyCrop(templates[3].image, templates, method, { minScore: 0.7, minMargin: 0.05 });
    assert.equal(result.hero, 'hero-3');
    assert.equal(result.top1, 'hero-3');
    assert.equal(result.accepted, true);
  }
});

test('ambiguous equal templates abstain instead of inventing an identity', () => {
  const image = patternImage(2);
  const templates = Array.from({ length: 5 }, (_, index) => ({ hero: `hero-${index}`, image }));
  const result = classifyCrop(image, templates, 'template_match', { minScore: 0.7, minMargin: 0.05 });
  assert.equal(result.top1, 'hero-0');
  assert.equal(result.hero, 'UNKNOWN');
  assert.equal(result.accepted, false);
});

test('empty timing samples stay explicitly unavailable', () => {
  assert.deepEqual(sampleStats([]), { n: 0, mean_ms: null, p50_ms: null, p95_ms: null, max_ms: null });
});

test('reads Kohbor recorder manifests and frames without inventing marker labels', () => {
  const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'minimap-recorder-evaluator-'));
  const matchHash = 'a'.repeat(64);
  const matchDirectory = path.join(temporaryRoot, matchHash);
  fs.mkdirSync(matchDirectory);
  try {
    fs.writeFileSync(path.join(matchDirectory, 'manifest.json'), JSON.stringify({
      schema_version: 1,
      match_hash: matchHash,
      match_hash_source: 'session_epoch_fallback',
      game_build: null,
      resolution: { width: 1920, height: 1080 },
      dpi: 96,
      capture_profile: 'p1080-default-v3',
      crop_confirmed: true,
      dota_minimap_settings: { show_hero_icons: true, hero_size: 600 },
      team_side: 'radiant',
      ally_hero_ids: null,
      enemy_hero_ids: null,
      sampling_rate_fps: 2,
      frame_count: 1,
      frame_bytes: 85,
      dropped_frames: 0,
      stop_reason: null,
    }));
    fs.writeFileSync(path.join(matchDirectory, 'frame-000001.png'), encodeRgbaPng(2, 2, [
      255, 0, 0, 255, 0, 255, 0, 255,
      0, 0, 255, 255, 255, 255, 255, 255,
    ]));
    fs.writeFileSync(path.join(matchDirectory, 'frames.jsonl'), `${JSON.stringify({
      frame_file: 'frame-000001.png',
      game_clock_seconds: null,
      session_epoch: 7,
      phase: 'live',
      capture_age_ms: 12,
      crop_rect: { x: 0, y: 800, width: 2, height: 2 },
      capture_size: { width: 1920, height: 1080 },
      stale: false,
    })}\n`);

    const result = evaluateRecording(matchDirectory);

    assert.equal(result.dataset.frame_count, 1);
    assert.equal(result.dataset.labels_complete, false);
    assert.equal(result.dataset.frames[0].visible_marker_crops, 0);
    assert.equal(result.dataset.frames[0].recorder_metadata.session_epoch, 7);
    assert.equal(result.methods.template_match.status, 'NOT_RUN_NO_LABELED_FRAMES');
    assert.equal(result.methods.template_match.metrics.micro_top1_accuracy, null);
    assert.equal(result.dataset.source_inventory.enemy_hero_ids, null);
    assert.equal(result.dataset.source_inventory.match_hash_source, 'session_epoch_fallback');
  } finally {
    fs.rmSync(temporaryRoot, { recursive: true, force: true });
  }
});
