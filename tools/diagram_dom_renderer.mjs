import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
let configFile;
let selectedFontDirectory;
let moduleDirectory = process.env.DIAGRAM_MODULE_ROOT || path.join(path.dirname(new URL(import.meta.url).pathname), 'node_modules');
let outDirectory = path.dirname(new URL(import.meta.url).pathname);
const inputFiles = [];
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--config') configFile = args[++i];
  else if (args[i] === '--out-dir') outDirectory = args[++i];
  else if (args[i] === '--font-dir') selectedFontDirectory = args[++i];
  else if (args[i] === '--module-root') moduleDirectory = args[++i];
  else inputFiles.push(args[i]);
}
const moduleRequire = createRequire(path.resolve(moduleDirectory, '__diagram_resolver__.cjs'));
const moduleEntries = { jsdom: moduleRequire.resolve('jsdom'), svgdom: moduleRequire.resolve('svgdom') };
moduleEntries.mermaid = moduleRequire.resolve('mermaid');
const { JSDOM } = await import(pathToFileURL(moduleEntries.jsdom).href);
const { createSVGWindow, config: svgConfig } = await import(pathToFileURL(moduleEntries.svgdom).href);
const configuration = configFile ? JSON.parse(await fs.readFile(configFile, 'utf8')) : {
  startOnLoad: false, securityLevel: 'strict', theme: 'neutral', layout: 'dagre',
  fontFamily: 'DejaVu Sans', htmlLabels: false,
  flowchart: { htmlLabels: false, useMaxWidth: false }, sequence: { useMaxWidth: false }
};
// Explicitly approved renderer setting; the shared source config may omit it.
configuration.htmlLabels = false;
const fontDirectory = selectedFontDirectory || process.env.DIAGRAM_FONT_DIR || path.join(path.dirname(new URL(import.meta.url).pathname), 'fonts');
svgConfig.setFontDir(fontDirectory).setFontFamilyMappings({
  'DejaVu Sans': 'DejaVuSans.ttf',
  'sans-serif': 'DejaVuSans.ttf',
  'Arial': 'DejaVuSans.ttf',
  'Open Sans': 'DejaVuSans.ttf',
  'DejaVu Sans Bold': 'DejaVuSans-Bold.ttf',
  'DejaVu Sans Oblique': 'DejaVuSans-Oblique.ttf',
  'DejaVu Sans Bold Oblique': 'DejaVuSans-BoldOblique.ttf'
}).preloadFonts();
// DejaVu's older OS/2 table can omit these metadata fields. Derive them
// from the same font's unmodified glyph outlines instead of inventing metrics.
for (const font of Object.values(svgConfig.getFonts())) {
  for (const [property, character] of [['xHeight', 'x'], ['capHeight', 'H']]) {
    if (!Number.isFinite(font[property]) || font[property] <= 0) {
      const height = font.glyphForCodePoint(character.codePointAt(0)).bbox.maxY;
      if (!Number.isFinite(height) || height <= 0) throw new Error(`Missing usable ${property} font metric`);
      Object.defineProperty(font, property, { value: height, configurable: true });
    }
  }
}
await fs.mkdir(outDirectory, { recursive: true });

const dom = new JSDOM('<!doctype html><html><body></body></html>', { pretendToBeVisual: true });
const win = dom.window;
for (const key of ['window', 'document', 'HTMLElement', 'SVGElement', 'Element', 'Node', 'DOMParser', 'XMLSerializer']) {
  Object.defineProperty(globalThis, key, { value: key === 'window' ? win : win[key], configurable: true });
}
Object.defineProperty(globalThis, 'navigator', { value: win.navigator, configurable: true });
globalThis.getComputedStyle = win.getComputedStyle.bind(win);
globalThis.requestAnimationFrame = win.requestAnimationFrame.bind(win);
globalThis.cancelAnimationFrame = win.cancelAnimationFrame.bind(win);
globalThis.CSSStyleSheet = win.CSSStyleSheet;
win.CSSStyleSheet.prototype.replaceSync = function (css) {
  const style = win.document.createElement('style');
  style.textContent = css;
  win.document.head.appendChild(style);
  while (this.cssRules.length) this.deleteRule(0);
  for (const rule of style.sheet.cssRules) this.insertRule(rule.cssText, this.cssRules.length);
  style.remove();
};
const metrics = createSVGWindow();
const root = metrics.document.documentElement;
const presentationProperties = ['fill','fill-opacity','fill-rule','stroke','stroke-width','stroke-opacity','stroke-linecap','stroke-linejoin','stroke-dasharray','stroke-dashoffset','stroke-miterlimit','opacity','color','font-family','font-size','font-weight','font-style','letter-spacing','word-spacing','text-anchor','dominant-baseline','alignment-baseline','visibility','display','paint-order','vector-effect','marker-start','marker-mid','marker-end','stop-color','stop-opacity'];
const presentationSet = new Set(presentationProperties);
const presentationSheet = win.document.createElement('style');
win.document.head.prepend(presentationSheet);
const cssMirrors = new WeakMap();
let presentationIndex = 0;
function promoteStyles() {
  for (const original of win.document.querySelectorAll('svg style')) {
    let mirror = cssMirrors.get(original);
    if (!mirror) {
      mirror = win.document.createElement('style');
      win.document.head.appendChild(mirror);
      cssMirrors.set(original, mirror);
    }
    if (mirror.textContent !== original.textContent) mirror.textContent = original.textContent;
  }
  const rules = [];
  for (const element of win.document.querySelectorAll('svg, svg *')) {
    const attributes = element.getAttributeNames().filter(name => presentationSet.has(name));
    if (!attributes.length) continue;
    let id = element.getAttribute('data-dom-presentation');
    if (!id) { id = `p${++presentationIndex}`; element.setAttribute('data-dom-presentation', id); }
    rules.push(`:where([data-dom-presentation="${id}"]){${attributes.map(name => `${name}:${element.getAttribute(name)}`).join(';')}}`);
  }
  const css = rules.join('\n');
  if (presentationSheet.textContent !== css) presentationSheet.textContent = css;
}

function measuredClone(node) {
  promoteStyles();
  root.innerHTML = new win.XMLSerializer().serializeToString(node);
  const copy = root.firstChild;
  const originals = [node, ...node.querySelectorAll('*')];
  const copies = [copy, ...copy.querySelectorAll('*')];
  originals.forEach((original, i) => {
    const clone = copies[i];
    if (!clone || !clone.setAttribute) return;
    const css = win.getComputedStyle(original);
    for (const prop of ['font-size', 'font-family', 'font-weight', 'text-anchor', 'dominant-baseline']) {
      const value = css.getPropertyValue(prop);
      if (value) clone.setAttribute(prop, value);
    }
    let family = css.getPropertyValue('font-family');
    if (!family || family.includes('var(')) family = configuration.fontFamily || 'DejaVu Sans';
    let size = css.getPropertyValue('font-size');
    if (!size || size.includes('var(')) size = configuration.themeVariables?.fontSize || '16px';
    clone.setAttribute('font-family', family);
    clone.setAttribute('font-size', size);
    const bold = css.fontWeight === 'bold' || Number.parseInt(css.fontWeight, 10) >= 600;
    const italic = ['italic', 'oblique'].includes(css.fontStyle);
    const metricFamily = bold || italic ? `DejaVu Sans${bold ? ' Bold' : ''}${italic ? ' Oblique' : ''}` : family;
    clone.setAttribute('font-family', metricFamily);
    clone.style.fontFamily = metricFamily;
    clone.style.fontSize = size;
    normalizeTextLengths(clone, Number.parseFloat(size));
  });
  flattenPlainTextRows(copy);
  copy.setAttribute('xml:space', 'preserve');
  return copy;
}
function normalizeTextLengths(element, fontSize) {
  if (!['text', 'tspan', 'textPath'].includes(element.localName)) return;
  for (const name of ['x', 'y', 'dx', 'dy']) {
    const value = element.getAttribute(name);
    if (!value) continue;
    // svgdom parses these with parseFloat and otherwise treats em as pixels.
    // Resolve em against the element's actual computed font before measurement.
    element.setAttribute(name, value.replace(/(-?(?:\d*\.)?\d+)(em|px)\b/g, (_, number, unit) => String(Number(number) * (unit === 'em' ? fontSize : 1))));
  }
}
function flattenPlainTextRows(container) {
  const rows = [...container.querySelectorAll('tspan.text-outer-tspan.row')];
  if (container.matches?.('tspan.text-outer-tspan.row')) rows.unshift(container);
  for (const row of rows) {
    const children = [...row.children];
    if (!children.length || children.some(child => child.localName !== 'tspan')) continue;
    const weights = new Set(children.map(child => child.getAttribute('font-weight') || row.getAttribute('font-weight') || '400'));
    const styles = new Set(children.map(child => child.getAttribute('font-style') || row.getAttribute('font-style') || 'normal'));
    if (weights.size === 1 && styles.size === 1) row.textContent = row.textContent;
  }
}
function portableSvg(svg) {
  const holder = win.document.createElement('div');
  holder.innerHTML = svg;
  win.document.body.appendChild(holder);
  promoteStyles();
  const svgNode = holder.querySelector('svg');
  const elements = [svgNode, ...svgNode.querySelectorAll('*')];
  const snapshots = elements.map(element => {
    const css = win.getComputedStyle(element);
    return presentationProperties.map(name => [name, css.getPropertyValue(name)]).filter(([, value]) => value && !value.includes('var('));
  });
  elements.forEach((element, i) => {
    for (const [name, value] of snapshots[i]) element.setAttribute(name, value);
    normalizeTextLengths(element, Number.parseFloat(element.getAttribute('font-size')));
    element.removeAttribute('style');
    element.removeAttribute('data-dom-presentation');
  });
  flattenPlainTextRows(svgNode);
  // Size each edge-label backplate from the final complete rows, rather than
  // an earlier word-fragment box. Opaque backplates keep edge strokes off text.
  for (const text of svgNode.querySelectorAll('.edgeLabel text')) {
    const background = [...text.parentElement.children].find(element => element.localName === 'rect' && element.classList.contains('background'));
    if (!background || !background.hasAttribute('width')) continue;
    const box = measuredClone(text).getBBox();
    if (![box.x, box.y, box.width, box.height].every(Number.isFinite)) throw new Error('Nonfinite edge-label backplate');
    background.setAttribute('x', String(box.x - 2));
    background.setAttribute('y', String(box.y - 2));
    background.setAttribute('width', String(box.width + 4));
    background.setAttribute('height', String(box.height + 4));
    background.setAttribute('opacity', '1');
  }
  svgNode.querySelectorAll('[data-dom-presentation]').forEach(element => element.removeAttribute('data-dom-presentation'));
  svgNode.setAttributeNS('http://www.w3.org/XML/1998/namespace', 'xml:space', 'preserve');
  svgNode.querySelectorAll('style').forEach(element => element.remove());
  const output = new win.XMLSerializer().serializeToString(svgNode);
  holder.remove();
  return output;
}
win.SVGElement.prototype.getBBox = function () {
  const b = measuredClone(this).getBBox();
  if (![b.x,b.y,b.width,b.height].every(Number.isFinite)) throw new Error(`Nonfinite metric: ${this.outerHTML.slice(0,800)}`);
  return { x: b.x, y: b.y, width: b.width, height: b.height };
};
win.SVGElement.prototype.getComputedTextLength = function () {
  const n = measuredClone(this);
  return n.getComputedTextLength ? n.getComputedTextLength() : n.getBBox().width;
};
win.SVGElement.prototype.getBoundingClientRect = function () {
  const b = this.getBBox();
  return { ...b, top: b.y, left: b.x, right: b.x + b.width, bottom: b.y + b.height, toJSON() { return this; } };
};
const { default: mermaid } = await import(pathToFileURL(moduleEntries.mermaid).href);
mermaid.initialize(configuration);
let index = 0;
const results = [];
const digest = value => crypto.createHash('sha256').update(value).digest('hex');
for (const file of inputFiles) {
  const source = await fs.readFile(file, 'utf8');
  try {
    await mermaid.parse(source);
    const rendered = await mermaid.render(`localDiagram${++index}`, source);
    const svg = portableSvg(rendered.svg);
    const out = path.join(outDirectory, path.basename(file, '.mmd') + '.svg');
    await fs.writeFile(out, svg);
    const svgDoc = new win.DOMParser().parseFromString(svg, 'image/svg+xml');
    const result = { source: file, source_sha256: digest(source), out, svg_sha256: digest(svg), bytes: Buffer.byteLength(svg), parsed: true, foreignObject: svg.includes('<foreignObject'), finite: !/NaN|Infinity/.test(svg), viewBox: svgDoc.documentElement.getAttribute('viewBox'), status: 'RENDERED' };
    results.push(result);
    console.log(JSON.stringify(result));
  } catch (error) {
    const result = { source: file, source_sha256: digest(source), status: 'FAILED', error: error.stack || String(error) };
    results.push(result);
    console.log(JSON.stringify(result));
  }
}
const versions = {};
for (const [name, entry] of Object.entries(moduleEntries)) {
  let directory = path.dirname(entry);
  while (directory !== path.dirname(directory)) {
    try {
      const manifest = JSON.parse(await fs.readFile(path.join(directory, 'package.json'), 'utf8'));
      if (manifest.name === name) { versions[name] = manifest.version; break; }
    } catch {}
    directory = path.dirname(directory);
  }
}
const report = { engine: 'Mermaid with jsdom DOM and svgdom SVG metrics; no browser', versions, node: process.version, configuration, config_source: configFile || null, font_directory: fontDirectory, font_file_sha256: digest(await fs.readFile(path.join(fontDirectory, 'DejaVuSans.ttf'))), style_pipeline: 'Promote SVG styles and presentation attributes into jsdom CSS cascade, then bake computed paint/font presentation attributes and remove stylesheets.', limitations: ['SVG metrics are supplied by svgdom, not a browser layout engine.', 'Generic sans-serif and Arial measurement probes map to bundled DejaVu Sans.', 'Rendered layout still requires independent rasterization and visual inspection.', 'No runtime application behavior was tested by rendering diagrams.'], results };
await fs.writeFile(path.join(outDirectory, 'render-report.json'), JSON.stringify(report, null, 2) + '\n');
if (results.some(result => result.status === 'FAILED' || !result.finite || result.foreignObject)) process.exitCode = 1;
