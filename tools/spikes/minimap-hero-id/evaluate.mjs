import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { deflateSync, inflateSync } from 'node:zlib';
import { performance } from 'node:perf_hooks';
import { fileURLToPath, pathToFileURL } from 'node:url';

const METHODS = ['template_match', 'color_histogram_plus_template'];
const UNKNOWN = 'UNKNOWN';

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

/** Decode non-interlaced, 8-bit RGB/RGBA PNGs using only Node built-ins. */
export function decodePng(buffer) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  if (!Buffer.isBuffer(buffer) || buffer.length < 8 || !buffer.subarray(0, 8).equals(signature)) {
    throw new Error('not a PNG file');
  }

  let width;
  let height;
  let bitDepth;
  let colorType;
  let interlace;
  const compressed = [];
  let offset = 8;
  while (offset + 12 <= buffer.length) {
    const size = buffer.readUInt32BE(offset);
    const type = buffer.toString('ascii', offset + 4, offset + 8);
    const dataStart = offset + 8;
    const dataEnd = dataStart + size;
    if (dataEnd + 4 > buffer.length) throw new Error('truncated PNG chunk');
    if (type === 'IHDR') {
      width = buffer.readUInt32BE(dataStart);
      height = buffer.readUInt32BE(dataStart + 4);
      bitDepth = buffer[dataStart + 8];
      colorType = buffer[dataStart + 9];
      interlace = buffer[dataStart + 12];
    } else if (type === 'IDAT') {
      compressed.push(buffer.subarray(dataStart, dataEnd));
    } else if (type === 'IEND') {
      break;
    }
    offset = dataEnd + 4;
  }

  if (!width || !height || bitDepth !== 8 || ![2, 6].includes(colorType) || interlace !== 0) {
    throw new Error('PNG must be non-interlaced, 8-bit RGB or RGBA');
  }
  const channels = colorType === 6 ? 4 : 3;
  const rowBytes = width * channels;
  const raw = inflateSync(Buffer.concat(compressed));
  if (raw.length !== height * (rowBytes + 1)) throw new Error('invalid PNG scanline size');
  const decoded = Buffer.alloc(height * rowBytes);

  let sourceOffset = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = raw[sourceOffset++];
    const rowOffset = y * rowBytes;
    const previousOffset = rowOffset - rowBytes;
    for (let x = 0; x < rowBytes; x += 1) {
      const rawValue = raw[sourceOffset++];
      const left = x >= channels ? decoded[rowOffset + x - channels] : 0;
      const up = y > 0 ? decoded[previousOffset + x] : 0;
      const upperLeft = y > 0 && x >= channels ? decoded[previousOffset + x - channels] : 0;
      let predictor = 0;
      if (filter === 1) predictor = left;
      else if (filter === 2) predictor = up;
      else if (filter === 3) predictor = Math.floor((left + up) / 2);
      else if (filter === 4) predictor = paeth(left, up, upperLeft);
      else if (filter !== 0) throw new Error(`unsupported PNG filter ${filter}`);
      decoded[rowOffset + x] = (rawValue + predictor) & 0xff;
    }
  }

  const rgba = new Uint8Array(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const from = pixel * channels;
    const to = pixel * 4;
    rgba[to] = decoded[from];
    rgba[to + 1] = decoded[from + 1];
    rgba[to + 2] = decoded[from + 2];
    rgba[to + 3] = channels === 4 ? decoded[from + 3] : 255;
  }
  return { width, height, data: rgba };
}

export function cropImage(image, box) {
  if (!Array.isArray(box) || box.length !== 4 || !box.every(Number.isInteger)) {
    throw new Error('crop box must be [x, y, width, height] integers');
  }
  const [x, y, width, height] = box;
  if (x < 0 || y < 0 || width < 1 || height < 1 || x + width > image.width || y + height > image.height) {
    throw new Error('crop box is outside the image');
  }
  const data = new Uint8Array(width * height * 4);
  for (let row = 0; row < height; row += 1) {
    const source = ((y + row) * image.width + x) * 4;
    const target = row * width * 4;
    data.set(image.data.subarray(source, source + width * 4), target);
  }
  return { width, height, data };
}

function resizeNearest(image, width, height) {
  if (image.width === width && image.height === height) return image;
  const data = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    const sourceY = Math.min(image.height - 1, Math.floor(y * image.height / height));
    for (let x = 0; x < width; x += 1) {
      const sourceX = Math.min(image.width - 1, Math.floor(x * image.width / width));
      const source = (sourceY * image.width + sourceX) * 4;
      const target = (y * width + x) * 4;
      data[target] = image.data[source];
      data[target + 1] = image.data[source + 1];
      data[target + 2] = image.data[source + 2];
      data[target + 3] = image.data[source + 3];
    }
  }
  return { width, height, data };
}

function luma(image, pixel) {
  const offset = pixel * 4;
  return 0.299 * image.data[offset] + 0.587 * image.data[offset + 1] + 0.114 * image.data[offset + 2];
}

function normalizedCrossCorrelation(crop, template) {
  const resized = resizeNearest(template, crop.width, crop.height);
  let weightSum = 0;
  let cropSum = 0;
  let templateSum = 0;
  for (let i = 0; i < crop.width * crop.height; i += 1) {
    const weight = resized.data[i * 4 + 3] / 255;
    if (weight === 0) continue;
    weightSum += weight;
    cropSum += luma(crop, i) * weight;
    templateSum += luma(resized, i) * weight;
  }
  if (weightSum === 0) return -1;
  const cropMean = cropSum / weightSum;
  const templateMean = templateSum / weightSum;
  let numerator = 0;
  let cropVariance = 0;
  let templateVariance = 0;
  for (let i = 0; i < crop.width * crop.height; i += 1) {
    const weight = resized.data[i * 4 + 3] / 255;
    if (weight === 0) continue;
    const a = luma(crop, i) - cropMean;
    const b = luma(resized, i) - templateMean;
    numerator += weight * a * b;
    cropVariance += weight * a * a;
    templateVariance += weight * b * b;
  }
  if (cropVariance === 0 || templateVariance === 0) return -1;
  return Math.max(-1, Math.min(1, numerator / Math.sqrt(cropVariance * templateVariance)));
}

function hueHistogram(image, alphaMask = null) {
  const bins = new Float64Array(12);
  let total = 0;
  for (let pixel = 0; pixel < image.width * image.height; pixel += 1) {
    const offset = pixel * 4;
    if (alphaMask && alphaMask.data[offset + 3] === 0) continue;
    const r = image.data[offset] / 255;
    const g = image.data[offset + 1] / 255;
    const b = image.data[offset + 2] / 255;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const delta = max - min;
    if (delta === 0) continue;
    let hue;
    if (max === r) hue = ((g - b) / delta) % 6;
    else if (max === g) hue = (b - r) / delta + 2;
    else hue = (r - g) / delta + 4;
    hue = ((hue / 6) + 1) % 1;
    const saturation = max === 0 ? 0 : delta / max;
    const weight = saturation;
    bins[Math.min(11, Math.floor(hue * 12))] += weight;
    total += weight;
  }
  if (total > 0) for (let i = 0; i < bins.length; i += 1) bins[i] /= total;
  return bins;
}

function histogramIntersection(left, right) {
  let score = 0;
  for (let i = 0; i < left.length; i += 1) score += Math.min(left[i], right[i]);
  return score;
}

function candidateScore(crop, template, method) {
  const ncc = normalizedCrossCorrelation(crop, template);
  const templateScore = (ncc + 1) / 2;
  if (method === 'template_match') return templateScore;
  const resized = resizeNearest(template, crop.width, crop.height);
  const cropHistogram = hueHistogram(crop);
  const templateHistogram = hueHistogram(resized, resized);
  const colorScore = histogramIntersection(cropHistogram, templateHistogram);
  return 0.75 * templateScore + 0.25 * colorScore;
}

export function rankCrop(crop, templates, method) {
  if (!METHODS.includes(method)) throw new Error(`unsupported method: ${method}`);
  if (!Array.isArray(templates) || templates.length !== 5) throw new Error('exactly five enemy templates are required');
  return templates
    .map(({ hero, image }) => ({ hero, score: candidateScore(crop, image, method) }))
    .sort((a, b) => b.score - a.score || a.hero.localeCompare(b.hero));
}

export function classifyCrop(crop, templates, method, thresholds) {
  const ranking = rankCrop(crop, templates, method);
  const top = ranking[0];
  const margin = top.score - ranking[1].score;
  const hasThresholds = Number.isFinite(thresholds?.minScore) && Number.isFinite(thresholds?.minMargin);
  const accepted = !hasThresholds || (top.score >= thresholds.minScore && margin >= thresholds.minMargin);
  return {
    hero: accepted ? top.hero : UNKNOWN,
    top1: top.hero,
    score: top.score,
    margin,
    accepted,
    ranking,
  };
}

function percentile(sorted, fraction) {
  if (sorted.length === 0) return null;
  const position = (sorted.length - 1) * fraction;
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return sorted[lower];
  return sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower);
}

export function sampleStats(samples) {
  if (samples.length === 0) return { n: 0, mean_ms: null, p50_ms: null, p95_ms: null, max_ms: null };
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    n: sorted.length,
    mean_ms: sorted.reduce((sum, value) => sum + value, 0) / sorted.length,
    p50_ms: percentile(sorted, 0.5),
    p95_ms: percentile(sorted, 0.95),
    max_ms: sorted.at(-1),
  };
}

function wilsonInterval(successes, total) {
  if (total === 0) return null;
  const z = 1.959963984540054;
  const p = successes / total;
  const denominator = 1 + (z * z) / total;
  const center = (p + (z * z) / (2 * total)) / denominator;
  const margin = (z * Math.sqrt((p * (1 - p) / total) + (z * z) / (4 * total * total))) / denominator;
  return [Math.max(0, center - margin), Math.min(1, center + margin)];
}

function classificationMetrics(records, classes, thresholdsAvailable) {
  if (records.length === 0) {
    return {
      n_visible_crops: 0,
      frame_count: 0,
      micro_top1_accuracy: null,
      macro_frame_top1_accuracy: null,
      per_frame: [],
      per_hero: {},
      confusion_matrix: {},
      false_missing_rate: null,
      false_missing_definition: 'visible annotated crop not assigned its true hero (UNKNOWN or wrong hero); detector misses are outside this crop-classification evaluator',
    };
  }
  const frames = new Map();
  const counts = new Map();
  const confusion = new Map();
  let correct = 0;
  let unknown = 0;
  for (const record of records) {
    const frame = frames.get(record.frame_id) ?? { total: 0, correct: 0, unknown: 0 };
    frame.total += 1;
    if (record.predicted === record.hero) {
      correct += 1;
      frame.correct += 1;
    }
    if (record.predicted === UNKNOWN) {
      unknown += 1;
      frame.unknown += 1;
    }
    frames.set(record.frame_id, frame);

    const heroCount = counts.get(record.hero) ?? { total: 0, correct: 0, unknown: 0 };
    heroCount.total += 1;
    if (record.predicted === record.hero) heroCount.correct += 1;
    if (record.predicted === UNKNOWN) heroCount.unknown += 1;
    counts.set(record.hero, heroCount);

    const row = confusion.get(record.hero) ?? new Map();
    row.set(record.predicted, (row.get(record.predicted) ?? 0) + 1);
    confusion.set(record.hero, row);
  }
  const perFrame = [...frames.entries()].map(([frame_id, value]) => ({
    frame_id,
    n_visible_crops: value.total,
    top1_accuracy: value.correct / value.total,
    false_missing_rate: (value.total - value.correct) / value.total,
    abstention_rate: thresholdsAvailable ? value.unknown / value.total : null,
  }));
  const perHero = {};
  for (const hero of classes) {
    const value = counts.get(hero) ?? { total: 0, correct: 0, unknown: 0 };
    perHero[hero] = {
      n: value.total,
      top1_accuracy: value.total ? value.correct / value.total : null,
      confidence_interval_95_wilson: wilsonInterval(value.correct, value.total),
      false_missing_rate: value.total ? (value.total - value.correct) / value.total : null,
      abstention_rate: thresholdsAvailable && value.total ? value.unknown / value.total : null,
    };
  }
  const confusionMatrix = {};
  for (const hero of classes) {
    const row = confusion.get(hero) ?? new Map();
    confusionMatrix[hero] = {};
    for (const predicted of [...classes, UNKNOWN]) confusionMatrix[hero][predicted] = row.get(predicted) ?? 0;
  }
  return {
    n_visible_crops: records.length,
    frame_count: frames.size,
    micro_top1_accuracy: correct / records.length,
    macro_frame_top1_accuracy: perFrame.reduce((sum, item) => sum + item.top1_accuracy, 0) / perFrame.length,
    confidence_interval_95_wilson: wilsonInterval(correct, records.length),
    per_frame: perFrame,
    per_hero: perHero,
    confusion_matrix: confusionMatrix,
    false_missing_rate: (records.length - correct) / records.length,
    abstention_rate: thresholdsAvailable ? unknown / records.length : null,
    false_missing_definition: 'visible annotated crop not assigned its true hero (UNKNOWN or wrong hero); detector misses are outside this crop-classification evaluator',
  };
}

function conditionMetrics(records, classes, thresholdsAvailable) {
  const conditions = new Map();
  for (const record of records) {
    for (const condition of record.conditions) {
      const entry = conditions.get(condition) ?? { total: 0, correct: 0, unknown: 0 };
      entry.total += 1;
      if (record.predicted === record.hero) entry.correct += 1;
      if (record.predicted === UNKNOWN) entry.unknown += 1;
      conditions.set(condition, entry);
    }
  }
  return Object.fromEntries([...conditions.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([condition, value]) => [condition, {
    n_visible_crops: value.total,
    top1_accuracy: value.correct / value.total,
    false_missing_rate: (value.total - value.correct) / value.total,
    abstention_rate: thresholdsAvailable ? value.unknown / value.total : null,
  }]));
}

function sha256File(filePath) {
  return createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function loadTemplateSet(templateRoot, lineup, cache) {
  return lineup.map((hero) => {
    const filePath = path.join(templateRoot, `${hero}.png`);
    if (!cache.has(filePath)) cache.set(filePath, decodePng(fs.readFileSync(filePath)));
    return { hero, image: cache.get(filePath) };
  });
}

export function evaluateManifest(manifestPath) {
  const resolvedManifest = path.resolve(manifestPath);
  const manifest = JSON.parse(fs.readFileSync(resolvedManifest, 'utf8'));
  return evaluateManifestData(manifest, path.dirname(resolvedManifest));
}

const REPOSITORY_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const RECORDING_FRAME_NAME = /^frame-[0-9]{6,}\.png$/;

function validRecorderFrameRow(row) {
  const rect = row?.crop_rect;
  const size = row?.capture_size;
  return row !== null
    && typeof row === 'object'
    && Number.isSafeInteger(row.session_epoch) && row.session_epoch > 0
    && (row.game_clock_seconds === null || Number.isSafeInteger(row.game_clock_seconds))
    && row.phase === 'live'
    && Number.isSafeInteger(row.capture_age_ms) && row.capture_age_ms >= 0
    && rect !== null && typeof rect === 'object'
    && Number.isInteger(rect.x) && Number.isInteger(rect.y)
    && Number.isInteger(rect.width) && rect.width > 0
    && Number.isInteger(rect.height) && rect.height > 0
    && size !== null && typeof size === 'object'
    && Number.isInteger(size.width) && size.width > 0
    && Number.isInteger(size.height) && size.height > 0
    && typeof row.stale === 'boolean';
}

/**
 * Read Kohbor Lab's per-match manifest and frames.jsonl. Optional labels are a separate
 * hand-authored file; this adapter never derives marker boxes or hero labels from pixels.
 */
export function evaluateRecording(recordingDirectory, annotationsPath = null) {
  const directory = path.resolve(recordingDirectory);
  const manifest = JSON.parse(fs.readFileSync(path.join(directory, 'manifest.json'), 'utf8'));
  if (manifest.schema_version !== 1
    || !/^[a-f0-9]{64}$/.test(manifest.match_hash ?? '')
    || !['match_id_hmac', 'session_epoch_fallback'].includes(manifest.match_hash_source)) {
    throw new Error('recording manifest schema_version 1, install-keyed match_hash and match_hash_source are required');
  }
  if (path.basename(directory).toLowerCase() !== manifest.match_hash) {
    throw new Error('recording directory must be named by manifest.match_hash');
  }
  if (!Number.isInteger(manifest.sampling_rate_fps) || manifest.sampling_rate_fps < 1 || manifest.sampling_rate_fps > 5) {
    throw new Error('recording sampling_rate_fps must be between 1 and 5');
  }

  const indexPath = path.join(directory, 'frames.jsonl');
  const rows = fs.readFileSync(indexPath, 'utf8').split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line));
  if (rows.length !== manifest.frame_count) throw new Error('manifest frame_count does not match frames.jsonl');
  const seen = new Set();
  const annotations = annotationsPath === null
    ? null
    : JSON.parse(fs.readFileSync(path.resolve(annotationsPath), 'utf8'));
  if (annotations !== null && (annotations.schema_version !== 1 || annotations.match_hash !== manifest.match_hash || !annotations.frames || typeof annotations.frames !== 'object')) {
    throw new Error('annotation file must match schema_version 1 and the recording match_hash');
  }

  const frames = rows.map((row) => {
    if (!validRecorderFrameRow(row)
      || typeof row.frame_file !== 'string'
      || !RECORDING_FRAME_NAME.test(row.frame_file ?? '')
      || seen.has(row.frame_file)) {
      throw new Error('frames.jsonl contains an invalid or duplicate frame_file');
    }
    seen.add(row.frame_file);
    const label = annotations?.frames?.[row.frame_file] ?? null;
    if (label !== null && (!Array.isArray(label.markers) || !Array.isArray(label.enemy_lineup) || label.enemy_lineup.length !== 5 || new Set(label.enemy_lineup).size !== 5 || !label.enemy_lineup.every((hero) => typeof hero === 'string'))) {
      throw new Error(`annotation for ${row.frame_file} needs five template IDs and a markers[] array`);
    }
    return {
      id: `${manifest.match_hash}:${row.frame_file}`,
      image: row.frame_file,
      ...(label ? { enemy_lineup: label.enemy_lineup, markers: label.markers } : { markers: [] }),
      annotations_complete: label?.annotations_complete === true,
      match_ref: manifest.match_hash,
      capture_rate_fps: manifest.sampling_rate_fps,
      hero_icons: manifest.dota_minimap_settings?.show_hero_icons ?? null,
      hud_scale: null,
      recorder_metadata: {
        game_clock_seconds: row.game_clock_seconds ?? null,
        session_epoch: row.session_epoch,
        phase: row.phase,
        capture_age_ms: row.capture_age_ms,
        crop_rect: row.crop_rect,
        capture_size: row.capture_size,
        stale: row.stale,
        enemy_hero_ids: manifest.enemy_hero_ids ?? null,
        ally_hero_ids: manifest.ally_hero_ids ?? null,
      },
    };
  });
  if (annotations !== null && Object.keys(annotations.frames).some((name) => !seen.has(name))) {
    throw new Error('annotation file references a frame not present in frames.jsonl');
  }

  const dataset = {
    schema_version: 1,
    source_revision: null,
    capture_rate_fps: manifest.sampling_rate_fps,
    template_root: path.join(REPOSITORY_ROOT, 'apps/desktop/src/assets/dota/6989/heroes'),
    thresholds: null,
    source_inventory: {
      recording_format: 'Kohbor Lab per-match manifest.json plus frames.jsonl',
      match_hash: manifest.match_hash,
      match_hash_source: manifest.match_hash_source,
      frame_count: manifest.frame_count,
      frame_bytes: manifest.frame_bytes,
      dropped_frames: manifest.dropped_frames,
      stop_reason: manifest.stop_reason ?? null,
      game_build: manifest.game_build ?? null,
      resolution: manifest.resolution ?? null,
      dpi: manifest.dpi ?? null,
      capture_profile: manifest.capture_profile ?? null,
      crop_confirmed: manifest.crop_confirmed,
      dota_minimap_settings: manifest.dota_minimap_settings ?? null,
      team_side: manifest.team_side ?? null,
      enemy_hero_ids: manifest.enemy_hero_ids ?? null,
      ally_hero_ids: manifest.ally_hero_ids ?? null,
      labels_supplied_separately: annotations !== null,
    },
    frames,
  };
  return evaluateManifestData(dataset, directory);
}

function evaluateManifestData(manifest, baseDir, templateRootOverride = null) {
  if (manifest.schema_version !== 1 || !Array.isArray(manifest.frames)) throw new Error('manifest schema_version 1 and frames[] are required');
  const templateRoot = templateRootOverride ?? path.resolve(baseDir, manifest.template_root);
  const cache = new Map();
  const rowsByMethod = Object.fromEntries(METHODS.map((method) => [method, []]));
  const timingByMethod = Object.fromEntries(METHODS.map((method) => [method, []]));
  const frameSummary = [];
  const resolutions = new Map();
  const hudScales = new Set();
  const iconSettings = new Set();
  const lineups = new Set();
  let labelsComplete = true;

  for (const frame of manifest.frames) {
    const markers = Array.isArray(frame.markers) ? frame.markers : [];
    const hasCandidateLineup = Array.isArray(frame.enemy_lineup) && frame.enemy_lineup.length === 5;
    if (!frame.id || !frame.image || (!hasCandidateLineup && markers.length > 0)) {
      throw new Error('each frame needs an id and PNG image; labeled frames also need exactly five enemy_lineup template IDs');
    }
    const imagePath = path.resolve(baseDir, frame.image);
    const image = decodePng(fs.readFileSync(imagePath));
    if (Array.isArray(frame.resolution) && (frame.resolution[0] !== image.width || frame.resolution[1] !== image.height)) {
      throw new Error(`frame ${frame.id} resolution metadata does not match PNG dimensions`);
    }
    if (frame.annotations_complete !== true) labelsComplete = false;
    const classes = hasCandidateLineup ? frame.enemy_lineup : [];
    const templates = classes.length ? loadTemplateSet(templateRoot, classes, cache) : [];
    const frameCounts = { visible_markers: 0, condition_labels: new Set() };
    const frameTimes = Object.fromEntries(METHODS.map((method) => [method, 0]));

    resolutions.set(`${image.width}x${image.height}`, (resolutions.get(`${image.width}x${image.height}`) ?? 0) + 1);
    if (frame.hud_scale !== undefined && frame.hud_scale !== null) hudScales.add(String(frame.hud_scale));
    if (frame.hero_icons !== undefined && frame.hero_icons !== null) iconSettings.add(String(frame.hero_icons));
    if (classes.length) lineups.add([...classes].sort().join(','));

    for (const marker of markers) {
      if (marker.visible === false) continue;
      if (!classes.includes(marker.hero) || !Array.isArray(marker.box)) throw new Error(`frame ${frame.id} marker must name a visible enemy in the five-hero lineup`);
      const crop = cropImage(image, marker.box);
      const conditions = Array.isArray(marker.conditions) && marker.conditions.length ? marker.conditions : ['clear'];
      frameCounts.visible_markers += 1;
      conditions.forEach((condition) => frameCounts.condition_labels.add(condition));
      for (const method of METHODS) {
        const start = performance.now();
        const result = classifyCrop(crop, templates, method, manifest.thresholds ?? null);
        const elapsed = performance.now() - start;
        frameTimes[method] += elapsed;
        rowsByMethod[method].push({
          frame_id: frame.id,
          hero: marker.hero,
          predicted: result.hero,
          top1: result.top1,
          score: result.score,
          margin: result.margin,
          conditions,
        });
      }
    }
    for (const method of METHODS) timingByMethod[method].push(frameTimes[method]);
    frameSummary.push({
      id: frame.id,
      match_ref: frame.match_ref ?? null,
      resolution: [image.width, image.height],
      hud_scale: frame.hud_scale ?? null,
      hero_icons: frame.hero_icons ?? null,
      capture_rate_fps: frame.capture_rate_fps ?? manifest.capture_rate_fps ?? null,
      annotations_complete: frame.annotations_complete === true,
      visible_marker_crops: frameCounts.visible_markers,
      conditions: [...frameCounts.condition_labels].sort(),
      recorder_metadata: frame.recorder_metadata ?? null,
    });
  }

  const thresholdsAvailable = Number.isFinite(manifest.thresholds?.minScore) && Number.isFinite(manifest.thresholds?.minMargin);
  const methods = {};
  for (const method of METHODS) {
    const records = rowsByMethod[method];
    methods[method] = {
      status: records.length ? 'MEASURED_ON_ANNOTATED_CROPS' : 'NOT_RUN_NO_LABELED_FRAMES',
      scope: 'manual marker-crop classification only; no full-frame detector or capture path is measured',
      thresholds: thresholdsAvailable ? manifest.thresholds : null,
      metrics: classificationMetrics(records, [...new Set(manifest.frames.flatMap((frame) => frame.enemy_lineup ?? []))], thresholdsAvailable),
      by_condition: conditionMetrics(records, [], thresholdsAvailable),
      matcher_time_per_frame_ms: sampleStats(timingByMethod[method]),
      timing_clock: 'Node performance.now wall-clock elapsed time; not process CPU time',
      capture_rate_budget_ms: manifest.capture_rate_fps ? 1000 / manifest.capture_rate_fps : null,
      annotations_complete: manifest.frames.length > 0 && labelsComplete,
    };
  }
  return {
    schema_version: 1,
    source_revision: manifest.source_revision ?? null,
    evidence_class: manifest.frames.length ? 'source' : 'source',
    generated_at_utc: new Date().toISOString(),
    dataset: {
      frame_count: manifest.frames.length,
      visible_marker_crops: rowsByMethod.template_match.length,
      unique_lineups: lineups.size,
      resolutions: Object.fromEntries([...resolutions.entries()].sort(([a], [b]) => a.localeCompare(b))),
      hud_scales: [...hudScales].sort(),
      hero_icon_settings: [...iconSettings].sort(),
      labels_complete: manifest.frames.length > 0 && labelsComplete,
      source_inventory: manifest.source_inventory ?? {},
      frames: frameSummary,
    },
    methods,
    limitation: manifest.frames.length === 0
      ? 'No labeled minimap frames were available. All real accuracy, confusion, false-missing, condition and per-frame CPU metrics are null; no labels were inferred.'
      : 'Results apply only to manually annotated visible marker crops. Detection misses, unannotated icons, fog absence, capture cost and live CPU load are not measured.',
  };
}

function timingBenchmark(templates, iterations, captureRateFps) {
  const crops = templates.map(({ image }) => resizeNearest(image, image.width, image.height));
  const results = {};
  for (const method of METHODS) {
    for (let warm = 0; warm < 5; warm += 1) {
      for (const crop of crops) classifyCrop(crop, templates, method, null);
    }
    const elapsedFrames = [];
    for (let frame = 0; frame < iterations; frame += 1) {
      const start = performance.now();
      for (const crop of crops) classifyCrop(crop, templates, method, null);
      elapsedFrames.push(performance.now() - start);
    }
    const stats = sampleStats(elapsedFrames);
    results[method] = {
      ...stats,
      frame_equivalent_per_second: stats.mean_ms ? 1000 / stats.mean_ms : null,
      markers_per_frame: crops.length,
      candidate_count_per_marker: templates.length,
      capture_rate_budget_ms: captureRateFps ? 1000 / captureRateFps : null,
      classifier_only_within_capture_budget_p95: captureRateFps ? stats.p95_ms <= 1000 / captureRateFps : null,
      timing_clock: 'Node performance.now wall-clock elapsed time; not process CPU time',
    };
  }
  return results;
}

export function runSyntheticBenchmark(templateRoot, { iterations = 100, captureRateFps = 30 } = {}) {
  const candidateNames = fs.readdirSync(templateRoot)
    .filter((name) => name.toLowerCase().endsWith('.png'))
    .filter((name) => !/(?:_alt\d*|_persona\d*|_carnival|_creep|_tower|^default)\.png$/i.test(name))
    .sort()
    .slice(0, 5);
  if (candidateNames.length !== 5) throw new Error('benchmark requires five base hero PNG templates');
  const templates = candidateNames.map((name) => {
    const templatePath = path.join(templateRoot, name);
    return {
      hero: path.basename(name, '.png'),
      image: decodePng(fs.readFileSync(templatePath)),
      sha256: sha256File(templatePath),
    };
  });
  return {
    evidence_class: 'synthetic',
    status: 'SYNTHETIC_COMPUTE_ONLY',
    candidate_templates: candidateNames,
    template_dimensions: templates.map(({ hero, image, sha256 }) => ({ hero, width: image.width, height: image.height, sha256 })),
    iterations,
    markers_per_frame: 5,
    candidate_count: 5,
    crop_dimensions: [templates[0].image.width, templates[0].image.height],
    all_templates_same_dimensions: templates.every(({ image }) => image.width === templates[0].image.width && image.height === templates[0].image.height),
    capture_rate_fps: captureRateFps,
    timing: timingBenchmark(templates, iterations, captureRateFps),
    limitation: 'Synthetic exact-template crops only. Includes crop classification and 5-way matching; excludes minimap detection, crop generation from a captured frame, capture, PNG decode and concurrent native load. Not an accuracy or live-performance result.',
  };
}

function parseArgs(argv) {
  const options = { benchmark: false, benchmarkFrames: 100 };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--manifest') options.manifest = argv[++i];
    else if (arg === '--recording') options.recording = argv[++i];
    else if (arg === '--annotations') options.annotations = argv[++i];
    else if (arg === '--out') options.out = argv[++i];
    else if (arg === '--benchmark') options.benchmark = true;
    else if (arg === '--benchmark-frames') options.benchmarkFrames = Number(argv[++i]);
    else throw new Error(`unknown argument: ${arg}`);
  }
  if ((!options.manifest && !options.recording) || (options.manifest && options.recording) || !options.out) {
    throw new Error('usage: node evaluate.mjs (--manifest <json> | --recording <match-folder> [--annotations <json>]) --out <json> [--benchmark] [--benchmark-frames <n>]');
  }
  if (options.annotations && !options.recording) throw new Error('--annotations requires --recording');
  if (!Number.isInteger(options.benchmarkFrames) || options.benchmarkFrames < 1) throw new Error('--benchmark-frames must be a positive integer');
  return options;
}

function main(argv) {
  const options = parseArgs(argv);
  const inputPath = path.resolve(options.manifest ?? options.recording);
  const manifestPath = options.manifest ? inputPath : path.join(inputPath, 'manifest.json');
  const result = options.manifest
    ? evaluateManifest(inputPath)
    : evaluateRecording(inputPath, options.annotations ?? null);
  result.evaluator = {
    path: path.relative(process.cwd(), fileURLToPath(import.meta.url)).replaceAll(path.sep, '/'),
    sha256: sha256File(fileURLToPath(import.meta.url)),
    manifest_path: path.relative(process.cwd(), manifestPath).replaceAll(path.sep, '/'),
    manifest_sha256: sha256File(manifestPath),
    node_version: process.version,
  };
  if (options.annotations) {
    const annotationsPath = path.resolve(options.annotations);
    result.evaluator.annotations_path = path.relative(process.cwd(), annotationsPath).replaceAll(path.sep, '/');
    result.evaluator.annotations_sha256 = sha256File(annotationsPath);
  }
  if (options.benchmark) {
    const manifest = options.manifest ? JSON.parse(fs.readFileSync(manifestPath, 'utf8')) : null;
    const templateRoot = manifest
      ? path.resolve(path.dirname(manifestPath), manifest.template_root)
      : path.join(REPOSITORY_ROOT, 'apps/desktop/src/assets/dota/6989/heroes');
    const captureRateFps = manifest?.capture_rate_fps ?? result.dataset?.frames?.[0]?.capture_rate_fps ?? 30;
    result.synthetic_benchmark = runSyntheticBenchmark(templateRoot, {
      iterations: options.benchmarkFrames,
      captureRateFps,
    });
    result.evidence_class = 'synthetic';
  }
  fs.mkdirSync(path.dirname(path.resolve(options.out)), { recursive: true });
  fs.writeFileSync(path.resolve(options.out), `${JSON.stringify(result, null, 2)}\n`, 'utf8');
  process.stdout.write(`wrote ${path.resolve(options.out)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error.stack ?? error}\n`);
    process.exitCode = 1;
  }
}
