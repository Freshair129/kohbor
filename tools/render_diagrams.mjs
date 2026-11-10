#!/usr/bin/env node
/** Render and validate documentation SVGs. This does not execute the application. */
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.dirname(here);
const pinned = { mermaid: '12.3.0', jsdom: '31.0.0', svgdom: '0.1.29' };
const options = {
  '--catalog': path.join(root, 'docs/diagrams/catalog.json'),
  '--config': path.join(here, 'diagram-renderer.json'),
  '--font-dir': process.env.DIAGRAM_FONT_DIR || path.join(here, 'fonts'),
  '--module-root': process.env.DIAGRAM_MODULE_ROOT || path.join(here, 'node_modules')
};
const args = process.argv.slice(2);
if (args.includes('--help')) {
  console.log(`Usage: node tools/render_diagrams.mjs [options]

  --catalog FILE       Catalog with package-relative source/rendered paths
  --config FILE        Mermaid configuration (default: tools/diagram-renderer.json)
  --font-dir DIR       DejaVu Sans font directory (default: tools/fonts)
  --module-root DIR    Installed pinned modules (default: tools/node_modules)

DIAGRAM_MODULE_ROOT and DIAGRAM_FONT_DIR are optional defaults for the last two
options. Explicit options take precedence. Install dependencies with npm ci in
tools/. No browser or application runtime is used. The core renderer must be
present as tools/diagram_dom_renderer.mjs. Results are written to the catalog's
rendered paths and acceptance/diagram-validation.json.`);
  process.exit(0);
}

const digest = value => createHash('sha256').update(value).digest('hex');
const reportPath = path.join(root, 'acceptance/diagram-validation.json');
const report = {
  schema_version: '1.0.0', spec_version: null, status: 'FAIL',
  scope: 'Mermaid parse/render and structural SVG validation only. No application execution, visual acceptance or standards conformance claim.',
  renderer: { engine: 'Mermaid with jsdom DOM and svgdom SVG metrics; no browser', versions: {}, node: process.version },
  diagrams: [],
  visual_inspection: 'NOT_RUN',
  application_native_live_acoustic_resource_tests: 'NOT_RUN'
};
let temporaryDirectory;
let validationWindow;

function packagePath(value, label) {
  if (typeof value !== 'string' || !value || path.isAbsolute(value)) throw new Error(`${label} must be package-relative`);
  const resolved = path.resolve(root, value);
  const relative = path.relative(root, resolved);
  if (!relative || relative === '..' || relative.startsWith('..' + path.sep) || path.isAbsolute(relative)) {
    throw new Error(`${label} escapes the package root`);
  }
  return resolved;
}

async function atomicWrite(filename, content) {
  await fs.mkdir(path.dirname(filename), { recursive: true });
  const temporary = `${filename}.tmp-${process.pid}`;
  try {
    await fs.writeFile(temporary, content);
    await fs.rename(temporary, filename);
  } finally {
    await fs.rm(temporary, { force: true });
  }
}

async function moduleVersion(name, entry) {
  let directory = path.dirname(entry);
  while (directory !== path.dirname(directory)) {
    try {
      const manifest = JSON.parse(await fs.readFile(path.join(directory, 'package.json'), 'utf8'));
      if (manifest.name === name) return manifest.version;
    } catch { /* Continue to the owning package. */ }
    directory = path.dirname(directory);
  }
  throw new Error(`Cannot identify installed ${name} version`);
}

function runCore(core, argv) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [core, ...argv], { stdio: ['ignore', 'ignore', 'pipe'] });
    let stderr = '';
    child.stderr.on('data', chunk => { stderr = (stderr + chunk.toString()).slice(-16384); });
    child.on('error', reject);
    child.on('close', (code, signal) => resolve({ code, signal, stderr: stderr.trim() }));
  });
}

function validateSvg(svg) {
  const document = new validationWindow.DOMParser().parseFromString(svg, 'image/svg+xml');
  if (document.querySelector('parsererror')) throw new Error('Rendered SVG is not well-formed XML');
  const element = document.documentElement;
  if (element.localName !== 'svg' || element.namespaceURI !== 'http://www.w3.org/2000/svg') throw new Error('Rendered document has no SVG root');
  if (document.getElementsByTagNameNS('*', 'foreignObject').length) throw new Error('Rendered SVG contains foreignObject');
  const geometry = new Set(['x', 'y', 'dx', 'dy', 'x1', 'y1', 'x2', 'y2', 'cx', 'cy', 'r', 'rx', 'ry', 'width', 'height', 'viewBox', 'd', 'points', 'transform', 'rotate', 'textLength', 'startOffset', 'font-size', 'stroke-width', 'stroke-dasharray', 'stroke-dashoffset', 'opacity', 'style']);
  for (const node of [element, ...element.querySelectorAll('*')]) {
    for (const attribute of node.attributes) {
      if (geometry.has(attribute.name) && /NaN|[+-]?Infinity/.test(attribute.value)) throw new Error(`Nonfinite SVG geometry in ${attribute.name}`);
    }
  }
  const viewBox = (element.getAttribute('viewBox') || '').trim().split(/[\s,]+/).map(Number);
  if (viewBox.length !== 4 || !viewBox.every(Number.isFinite) || viewBox[2] <= 0 || viewBox[3] <= 0) throw new Error('Missing or invalid finite SVG viewBox');
  const textNodes = [...element.querySelectorAll('text')];
  if (!textNodes.length || !textNodes.some(node => node.textContent.trim())) throw new Error('Rendered SVG has no readable text nodes');
  return { width: viewBox[2], height: viewBox[3], text_nodes: textNodes.length };
}

try {
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index];
    if (!Object.hasOwn(options, name)) throw new Error(`Unknown option: ${name}`);
    const value = args[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`Missing value for ${name}`);
    options[name] = value;
  }
  for (const name of Object.keys(options)) options[name] = path.resolve(options[name]);
  const catalog = JSON.parse(await fs.readFile(options['--catalog'], 'utf8'));
  if (!Array.isArray(catalog.diagrams) || !catalog.diagrams.length) throw new Error('Catalog has no diagrams');
  report.spec_version = catalog.spec_version;
  const ids = new Set(), sources = new Set(), outputs = new Set(), basenames = new Set();
  const entries = [];
  for (const diagram of catalog.diagrams) {
    if (typeof diagram.id !== 'string' || !diagram.id || ids.has(diagram.id)) throw new Error('Catalog diagram IDs must be nonempty and unique');
    const source = packagePath(diagram.source, `${diagram.id} source`);
    const output = packagePath(diagram.rendered, `${diagram.id} rendered`);
    const basename = path.basename(source, '.mmd');
    if (!source.endsWith('.mmd') || !output.endsWith('.svg')) throw new Error(`${diagram.id} requires .mmd input and .svg output`);
    if (sources.has(source) || outputs.has(output) || basenames.has(basename)) throw new Error(`${diagram.id} has a duplicate source, output or renderer basename`);
    ids.add(diagram.id); sources.add(source); outputs.add(output); basenames.add(basename);
    const result = { id: diagram.id, status: 'FAIL', source_sha256: null, svg_sha256: null, width: null, height: null, text_nodes: 0 };
    report.diagrams.push(result);
    const bytes = await fs.readFile(source);
    if (!Buffer.from(bytes.toString('utf8'), 'utf8').equals(bytes)) throw new Error(`${diagram.id} source is not valid UTF-8`);
    result.source_sha256 = digest(bytes);
    entries.push({ source, output, basename, result });
  }
  const [major, minor, patch] = process.versions.node.split('.').map(Number);
  if (!(major >= 26 || major === 24 && minor >= 15 || major === 22 && (minor > 22 || minor === 22 && patch >= 2))) {
    throw new Error('Pinned dependencies require Node ^22.22.2 || ^24.15.0 || >=26.0.0');
  }
  const modules = options['--module-root'];
  const requireModule = createRequire(path.join(modules, '__diagram_resolver__.cjs'));
  const moduleEntries = {};
  for (const [name, expected] of Object.entries(pinned)) {
    moduleEntries[name] = requireModule.resolve(name);
    const actual = await moduleVersion(name, moduleEntries[name]);
    report.renderer.versions[name] = actual;
    if (actual !== expected) throw new Error(`${name} ${expected} required; found ${actual}. Run npm ci in tools/.`);
  }
  const { JSDOM } = await import(pathToFileURL(moduleEntries.jsdom).href);
  validationWindow = new JSDOM('').window;
  const configBytes = await fs.readFile(options['--config']);
  const config = JSON.parse(configBytes.toString('utf8'));
  if (config.htmlLabels !== false || config.flowchart?.htmlLabels !== false) throw new Error('Renderer configuration must disable HTML labels');
  report.renderer.config_sha256 = digest(configBytes);
  report.renderer.font_files_sha256 = {};
  for (const font of ['DejaVuSans.ttf', 'DejaVuSans-Bold.ttf', 'DejaVuSans-Oblique.ttf', 'DejaVuSans-BoldOblique.ttf']) {
    report.renderer.font_files_sha256[font] = digest(await fs.readFile(path.join(options['--font-dir'], font)));
  }
  const core = path.join(here, 'diagram_dom_renderer.mjs');
  report.renderer.core_sha256 = digest(await fs.readFile(core));
  temporaryDirectory = await fs.mkdtemp(path.join(os.tmpdir(), 'spec-diagram-render-'));
  const execution = await runCore(core, [
    '--config', options['--config'], '--font-dir', options['--font-dir'],
    '--module-root', modules, '--out-dir', temporaryDirectory, ...entries.map(entry => entry.source)
  ]);
  report.renderer.exit_code = execution.code;
  if (execution.signal) report.renderer.signal = execution.signal;
  if (execution.code !== 0 && execution.stderr) report.renderer.error = execution.stderr;
  const coreReport = JSON.parse(await fs.readFile(path.join(temporaryDirectory, 'render-report.json'), 'utf8'));
  if (!Array.isArray(coreReport.results)) throw new Error('Core renderer did not produce a result array');
  const renderedBySource = new Map(coreReport.results.map(result => [path.resolve(result.source), result]));
  for (const entry of entries) {
    try {
      const rendered = renderedBySource.get(entry.source);
      if (!rendered || rendered.status !== 'RENDERED') throw new Error(rendered?.error || 'Core did not render this source');
      if (rendered.parsed !== true) throw new Error('Core did not confirm successful Mermaid parsing');
      if (rendered.finite === false) throw new Error('Core reported nonfinite SVG data');
      if (rendered.foreignObject === true) throw new Error('Core reported an SVG foreignObject');
      if (rendered.source_sha256 !== entry.result.source_sha256) throw new Error('Source changed during rendering');
      const producedPath = path.join(temporaryDirectory, entry.basename + '.svg');
      if (path.resolve(rendered.out) !== producedPath) throw new Error('Core returned an unexpected output path');
      const svg = await fs.readFile(producedPath);
      const svgDigest = digest(svg);
      if (rendered.svg_sha256 !== svgDigest) throw new Error('Core SVG hash does not match generated bytes');
      const metrics = validateSvg(svg.toString('utf8'));
      await atomicWrite(entry.output, svg);
      Object.assign(entry.result, metrics, { status: 'PASS', source_sha256: rendered.source_sha256, svg_sha256: svgDigest });
      console.log(`${entry.result.id} PASS ${metrics.width}x${metrics.height}; ${metrics.text_nodes} text nodes`);
    } catch (error) {
      entry.result.error = error.message || String(error);
      console.error(`${entry.result.id} FAIL ${entry.result.error}`);
    }
  }
  report.status = execution.code === 0 && report.diagrams.every(result => result.status === 'PASS') ? 'PASS' : 'FAIL';
  report.checks = ['explicit Mermaid parse', 'SVG generation', 'source and output hashes', 'well-formed SVG XML', 'finite geometry and positive viewBox', 'no foreignObject', 'nonempty text nodes'];
} catch (error) {
  report.error = error.message || String(error);
  console.error(`Diagram validation FAIL: ${report.error}`);
} finally {
  validationWindow?.close();
  if (temporaryDirectory) await fs.rm(temporaryDirectory, { recursive: true, force: true });
  await atomicWrite(reportPath, JSON.stringify(report, null, 2) + '\n');
}
if (report.status !== 'PASS') process.exitCode = 1;
