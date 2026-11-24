# Minimap hero identity evaluator

The original evaluator, tests, baseline inventory and results were copied from `spike/minimap-hero-id` at source revision `7cf478cf12b6b35cc193ae2a8dc3a0fb0c2701d5`. The recording adapter and its test were added here; the copied `results.json` remains the original empty-data/synthetic result record.

This is a dependency-free offline evaluator for a **five-known-enemy** minimap experiment. It reads 8-bit RGB/RGBA PNG frames, applies supplied human marker boxes, and compares each visible crop against exactly five bundled hero templates.

Methods:

- `template_match`: zero-mean normalized grayscale correlation.
- `color_histogram_plus_template`: 75% normalized correlation plus 25% 12-bin saturation-weighted hue histogram intersection.

The evaluator does not find marker boxes. It cannot measure detector misses, fog-of-war absence, overlap/ping detection, capture overhead, or whole-frame performance. For future measurements, each frame manifest must contain the known five-hero lineup, current template filenames, screen dimensions, HUD scale, hero-icon setting, capture rate, match reference, and a complete annotation of visible marker boxes and conditions. Set confidence thresholds before evaluation if abstention rate is needed; changing thresholds after examining holdout results invalidates the holdout.

## Kohbor Lab recordings

The Lab recorder writes one directory per install-keyed match hash with `manifest.json`, `frames.jsonl` and lossless `frame-XXXXXX.png` minimap crops. Point the evaluator at that directory:

```powershell
node tools/spikes/minimap-hero-id/evaluate.mjs --recording "<app-data>/lab/minimap-recordings/<match-hash>" --out .\recording-results.json
```

Without a separate annotation file, the evaluator only inventories the stored frames and reports classification metrics as unavailable. It never guesses marker boxes or turns an enemy-lineup field into labels. A later labeling step may provide an annotation JSON with `schema_version: 1`, the same `match_hash`, and a `frames` object keyed by the exact `frame_file` values in `frames.jsonl`. Each labeled frame supplies `enemy_lineup` (five template IDs matching PNG filenames under `apps/desktop/src/assets/dota/6989/heroes`), `markers` (human-labeled `{ "hero": "axe", "box": [x, y, width, height], "conditions": ["clear"] }` rows, with `box` coordinates inside that minimap PNG), and optional `annotations_complete: true` only after the annotator has completed the frame.

```powershell
node tools/spikes/minimap-hero-id/evaluate.mjs --recording "<app-data>/lab/minimap-recordings/<match-hash>" --annotations .\annotations.json --out .\recording-results.json
```

Recorded roster IDs and any later template IDs are kept as distinct fields. Do not copy player names, account IDs, chat, audio or full-screen captures into annotation files. The recorder manifest may legitimately contain null game clock, build or roster values when the host has no approved read-only source; the evaluator preserves those unknowns.

Run the pure-logic tests directly with Node:

```powershell
node --test --test-isolation=none tools/spikes/minimap-hero-id/evaluate.test.mjs
```

`--test-isolation=none` keeps Node's test runner in-process in this restricted Windows environment, where its default worker spawn is denied.

Recreate the current empty-data result and the separate synthetic matcher-cost diagnostic:

```powershell
node tools/spikes/minimap-hero-id/evaluate.mjs --manifest tools/spikes/minimap-hero-id/dataset-manifest.json --out tools/spikes/minimap-hero-id/results.json --benchmark --benchmark-frames 100
```

The benchmark feeds exact template images back to the matcher. It is a compute-only timing probe, not a labelled frame, an accuracy score, or native capture evidence.
