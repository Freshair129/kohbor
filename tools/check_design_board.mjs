#!/usr/bin/env node
/** DOM logic checks for the design document. No rendering, browser or native acceptance. */
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const file = resolve(root, 'design/board/kohbor-design-board.html');
const html = readFileSync(file, 'utf8');
const checks = [];
let networkCalls = 0;
let microphoneCalls = 0;
let downloads = 0;
const errors = [];
const dom = new JSDOM(html, {
  runScripts: 'dangerously', url: 'https://example.invalid/design-board/',
  beforeParse(window) {
    // DOM does not perform layout. Width is a logic-harness input, never a measured viewport.
    Object.defineProperty(window.HTMLElement.prototype, 'clientWidth', { get() { return this.id === 'stage-fit' ? 1420 : 0; } });
    window.ResizeObserver = class { observe() {} disconnect() {} };
    window.matchMedia = () => ({ matches: false });
    window.HTMLElement.prototype.scrollIntoView = () => {};
    window.fetch = async () => { networkCalls++; throw new Error('No network in fixture harness'); };
    window.navigator.mediaDevices = { getUserMedia: async () => { microphoneCalls++; throw new Error('No microphone in fixture harness'); } };
    window.URL.createObjectURL = () => 'blob:fixture-download';
    window.URL.revokeObjectURL = () => {};
    window.HTMLAnchorElement.prototype.click = function() { if (this.download) downloads++; };
    window.addEventListener('error', event => errors.push(event.message));
  }
});
const { window: w } = dom;
const d = w.document;
const $ = id => d.getElementById(id);
const click = id => { assert.ok($(id), `Missing #${id}`); $(id).click(); };
const scenario = name => { $('scenario').value = name; $('scenario').dispatchEvent(new w.Event('change', { bubbles: true })); };
const escape = () => d.activeElement.dispatchEvent(new w.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
const run = (name, fn) => { fn(); checks.push({ name, status: 'PASS' }); };

run('Neutral startup preserves 5 enemy / 4 ally unknown slots without a selected hero', () => {
  assert.equal($('enemy-row').children.length, 5); assert.equal($('ally-row').children.length, 4);
  assert.ok([...d.querySelectorAll('.slot')].every(x => x.disabled));
  assert.equal(w.kohborBoardSnapshot().selection, null);
  assert.ok(!$('own-values').textContent.includes('720'));
  assert.ok([...d.querySelectorAll('.slot small')].every(x => x.textContent === '—'));
});
run('Fixture data is labelled outside the app and live mode requires an explicit scenario change', () => {
  assert.equal($('app-stage').contains($('scenario')), false);
  assert.equal($('app-stage').contains($('pro-mode')), false);
  scenario('live'); assert.equal($('enemy-0').disabled, false);
  assert.ok($('session-label').textContent.includes('สังเคราะห์'));
});
run('Explicit base card selection exposes a narrow summary without unseen vitals or skills', () => {
  click('enemy-0'); assert.equal(w.kohborBoardSnapshot().selection.kind, 'enemy');
  assert.ok($('right-content').textContent.includes('BASE SUMMARY'));
  assert.equal($('right-content').querySelectorAll('.meter').length, 0);
  assert.equal($('inline-toggle'), null);
});
run('Pro fixture begins neutral and does not create data permissions', () => {
  click('pro-mode'); assert.equal(w.kohborBoardSnapshot().selection, null);
  click('enemy-1'); assert.ok($('right-content').textContent.includes('PRO FIXTURE'));
  assert.equal($('right-content').querySelectorAll('.meter').length, 0);
});
run('Escape closes inline detail before inspector and restores card focus', () => {
  click('inline-toggle'); assert.equal(w.kohborBoardSnapshot().inlineOpen, true);
  escape(); assert.equal(w.kohborBoardSnapshot().inlineOpen, false); assert.ok(w.kohborBoardSnapshot().selection);
  escape(); assert.equal(w.kohborBoardSnapshot().selection, null); assert.equal(d.activeElement.id, 'enemy-1');
});
run('Capability loss clears detail and selection while preserving event-tab choice', () => {
  click('enemy-0'); click('inline-toggle'); click('events-tab'); click('base-mode');
  assert.equal(w.kohborBoardSnapshot().selection, null); assert.equal(w.kohborBoardSnapshot().inlineOpen, false);
  assert.equal(w.kohborBoardSnapshot().tab, 'events'); assert.equal($('right-content').querySelectorAll('.event-item').length, 2);
});
run('Right-side tabs support selected state and arrow-key focus navigation', () => {
  click('summary-tab'); $('summary-tab').focus();
  $('summary-tab').dispatchEvent(new w.KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
  assert.equal(d.activeElement.id, 'events-tab'); assert.equal($('events-tab').getAttribute('aria-selected'), 'true');
  assert.equal($('summary-tab').tabIndex, -1); assert.equal($('right-content').getAttribute('aria-labelledby'), 'events-tab');
});
run('Stale fixture clearly labels old own/CV observations and clears previous selection', () => {
  click('summary-tab'); click('enemy-0'); scenario('stale');
  assert.equal(w.kohborBoardSnapshot().selection, null);
  assert.ok($('map-state').textContent.includes('เก่า'));
  assert.ok($('own-values').textContent.includes('ล่าสุด'));
});
run('GSI-only unavailable CV never invents enemy identity or age and retains own fixture values', () => {
  scenario('unavailable');
  assert.ok([...$('enemy-row').children].every(x => x.disabled));
  assert.ok([...$('enemy-row').querySelectorAll('small')].every(x => x.textContent === '—'));
  assert.ok($('own-values').textContent.includes('720 / 1,200'));
  assert.equal($('source-age').textContent, '—');
});
run('Decorative map contains no selectable live marker', () => {
  assert.equal($('map-region').querySelectorAll('button,[role=button],[data-entity-key]').length, 0);
  assert.ok($('map-region').textContent.includes('ไม่มีตำแหน่งผู้เล่น'));
});
run('Contrast and motion controls invoke the generated token adapter attributes', () => {
  click('contrast-toggle'); assert.equal(d.documentElement.dataset.kbContrast, 'more');
  click('motion-toggle'); assert.equal(d.documentElement.dataset.kbMotion, 'reduced');
  assert.ok(d.querySelector('style[data-embed="tokens"]').textContent.includes(':root[data-kb-motion="reduced"]'));
});
run('Voice and opt-in specimens have no microphone or transport side effect', () => {
  for (const b of d.querySelectorAll('[data-voice]')) b.click();
  click('consent-sample'); assert.ok($('consent-note').textContent.includes('ตัวอย่าง'));
  assert.equal(microphoneCalls, 0); assert.equal(networkCalls, 0);
});
run('Embedded token download matches the canonical merged token bytes', () => {
  assert.deepEqual(JSON.parse($('kb-token-data').textContent), JSON.parse(readFileSync(resolve(root, 'design/tokens/kohbor.tokens.json'), 'utf8')));
  click('download-tokens'); click('download-css'); assert.equal(downloads, 2); assert.equal(networkCalls, 0);
});
run('Unknown reset removes fixture own values and disables all hero selections', () => {
  scenario('waiting'); assert.ok([...d.querySelectorAll('.slot')].every(x => x.disabled));
  assert.ok(!$('own-values').textContent.includes('720')); assert.equal(w.kohborBoardSnapshot().selection, null);
});
run('All referenced CSS custom properties are defined and HTML has no duplicate ids', () => {
  const styles = [...d.querySelectorAll('style')].map(x => x.textContent).join('\n');
  const allSource = styles + readFileSync(resolve(root, 'design/board/board.js'), 'utf8');
  const defined = new Set([...styles.matchAll(/(--kb-[a-z0-9-]+)\s*:/g)].map(x => x[1]));
  const used = new Set([...allSource.matchAll(/var\((--kb-[a-z0-9-]+)/g)].map(x => x[1]));
  assert.deepEqual([...used].filter(x => !defined.has(x)), []);
  const ids = [...d.querySelectorAll('[id]')].map(x => x.id); assert.equal(ids.length, new Set(ids).size);
});
run('Bundle causes no script exception or external resource reference', () => {
  assert.deepEqual(errors, []);
  assert.equal(d.querySelectorAll('script[src],link[rel=stylesheet],iframe,video,audio').length, 0);
  assert.ok([...d.querySelectorAll('img')].every(x => x.getAttribute('src').startsWith('data:')));
});

const sha = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const report = {
  status: 'PASS', evidence_class: 'synthetic_dom', design_version: '1.0.0', spec_baseline: '0.3.1',
  scope: `${checks.length} design-board DOM logic checks. jsdom does not render layout or execute native application behavior.`,
  environment: { node: process.version, jsdom: JSON.parse(readFileSync(resolve(root, 'tools/node_modules/jsdom/package.json'), 'utf8')).version, layout: 'NOT_AVAILABLE; clientWidth is a harness input' },
  files: ['design/board/kohbor-design-board.html', 'design/board/board.js', 'design/board/board.css', 'tools/check_design_board.mjs'].map(p => ({ path: p, sha256: sha(resolve(root, p)) })),
  checks, network_calls: networkCalls, microphone_calls: microphoneCalls,
  browser_rendered_viewport_and_visual_checks: 'NOT_RUN',
  browser_limitation: 'Cloud browser could not reach the workspace HTTP server; observing its error page was blocked by the browser URL policy. No alternate surface or policy workaround was used.',
  application_native_live_acoustic_resource_tests: 'NOT_RUN'
};
const reportPath = resolve(root, 'design/board/dom-validation.json');
const content = JSON.stringify(report, null, 2) + '\n';
if (process.argv.includes('--check')) assert.equal(readFileSync(reportPath, 'utf8'), content, 'Stale DOM receipt; rerun without --check');
else writeFileSync(reportPath, content);
console.log(JSON.stringify({ status: 'PASS', evidence_class: report.evidence_class, checks: checks.length, receipt: relative(root, reportPath), browser_layout: 'NOT_RUN', native: 'NOT_RUN' }));
dom.window.close();
