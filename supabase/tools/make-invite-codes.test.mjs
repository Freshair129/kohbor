import assert from 'node:assert/strict';
import { createHash, webcrypto } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  ALPHABET,
  CODE_SHAPE,
  buildInsertSql,
  codeHash,
  insideRepo,
  makeCode,
  makeCodes,
  normalize,
  parseArgs,
  run,
} from './make-invite-codes.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const functionSource = readFileSync(join(here, '..', 'functions', 'alpha-download', 'index.ts'), 'utf8');

// Load the deployed function's own CODE_SHAPE and normalize() from its source, so a drift
// between the generator and the gate fails here instead of at a tester's download.
function functionNormalize() {
  const shape = functionSource.match(/const CODE_SHAPE = (\/.*\/);/)[1];
  const body = functionSource.match(/function normalize\(raw: unknown\): string \| null \{\n([\s\S]*?)\n\}/)[1];
  return new Function('CODE_SHAPE', `return function normalize(raw) {\n${body}\n};`)(
    new Function(`return ${shape};`)(),
  );
}

const outside = () => mkdtempSync(join(tmpdir(), 'kb-invite-'));

test('alphabet is Crockford base32 and matches the gate character class', () => {
  assert.equal(ALPHABET.length, 32);
  assert.equal(new Set(ALPHABET).size, 32);
  for (const banned of 'ILOU') assert.ok(!ALPHABET.includes(banned));
  assert.equal(String(CODE_SHAPE), functionSource.match(/const CODE_SHAPE = (\/.*\/);/)[1]);
});

test('generated codes have the gate shape and are accepted unchanged by the gate', () => {
  const gate = functionNormalize();
  for (const code of makeCodes(300)) {
    assert.match(code, CODE_SHAPE);
    assert.equal(gate(code), code);
    assert.equal(normalize(code), code);
  }
});

test('generator normalize matches the gate on typed variants and rejects', () => {
  const gate = functionNormalize();
  const code = makeCode(() => Buffer.from([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 31]));
  assert.equal(code, 'KB-0123-4567-89AZ');
  const inputs = [
    code, code.toLowerCase(), code.replace(/-/g, ''), ` ${code} `, code.replace(/-/g, '_'),
    'kb-o123-4567-89az', 'KB-0I23-4567-89AZ', 'KB-0L23-4567-89AZ', '0123456789AZ',
    'KB-0123-4567-89AU', 'KB-0123-4567-89A', '', 'x'.repeat(41), 42, null,
  ];
  for (const input of inputs) assert.equal(normalize(input), gate(input), String(input));
});

test('code hash equals the gate hash (WebCrypto SHA-256 of the canonical code)', async () => {
  for (const code of makeCodes(20)) {
    const digest = await webcrypto.subtle.digest('SHA-256', new TextEncoder().encode(code));
    assert.equal(codeHash(code), Buffer.from(digest).toString('hex'));
    assert.match(codeHash(code), /^[0-9a-f]{64}$/);
  }
});

test('byte mapping uses every symbol and no other', () => {
  const all = Buffer.from([...Array(256).keys()]);
  let offset = 0;
  const seen = new Set();
  for (let i = 0; i < 22; i += 1) {
    const code = makeCode(n => { const b = Buffer.alloc(n); for (let j = 0; j < n; j += 1) b[j] = all[(offset + j) % 256]; offset += n; return b; });
    for (const ch of code.replace(/^KB-|-/g, '')) seen.add(ch);
  }
  assert.deepEqual([...seen].sort().join(''), [...ALPHABET].sort().join(''));
});

test('SQL holds hashes only, escapes the label and keeps the table limits', () => {
  const codes = makeCodes(5);
  const sql = buildInsertSql(codes, { label: "tester's batch", maxDownloads: 2, expiresAt: '2030-01-01T00:00:00Z' });
  for (const code of codes) {
    assert.ok(!sql.includes(code));
    assert.ok(!sql.includes(code.replace(/-/g, '')));
    assert.ok(sql.includes(`'${createHash('sha256').update(code).digest('hex')}'`));
  }
  assert.ok(sql.includes("'tester''s batch'"));
  assert.ok(sql.includes("'2030-01-01T00:00:00Z'::timestamptz"));
  assert.match(sql, /^INSERT INTO public\.alpha_invites \(code_hash, label, max_downloads, expires_at\) VALUES\n/);
  assert.equal(buildInsertSql(['KB-0000-0000-0000']).includes('NULL, 3, NULL'), true);
});

test('options are validated and outputs must stay outside the repository', () => {
  const dir = outside();
  try {
    const out = join(dir, 'codes.txt');
    assert.throws(() => parseArgs(['--count', '0', '--out', out]), /--count/);
    assert.throws(() => parseArgs(['--count', '501', '--out', out]), /--count/);
    assert.throws(() => parseArgs(['--count', '2']), /--out is required/);
    assert.throws(() => parseArgs(['--count', '2', '--out', join(here, 'codes.txt')]), /inside the repository/);
    assert.throws(() => parseArgs(['--count', '2', '--out', out, '--sql', join(here, '..', 'x.sql')]), /inside the repository/);
    assert.throws(() => parseArgs(['--count', '2', '--out', out, '--label', 'x'.repeat(65)]), /--label/);
    assert.throws(() => parseArgs(['--count', '2', '--out', out, '--max-downloads', '101']), /--max-downloads/);
    assert.throws(() => parseArgs(['--count', '2', '--out', out, '--expires', '2030-01-01']), /--expires/);
    assert.throws(() => parseArgs(['--count', '2', '--out', out, '--expires', '2000-01-01T00:00:00Z']), /future/);
    assert.throws(() => parseArgs(['--count', '2', '--out', out, '--bogus', 'x']), /unknown option/);
    assert.throws(() => parseArgs(['--count', '--out', out]), /needs a value/);
    assert.equal(insideRepo(here), true);
    assert.equal(insideRepo(dir), false);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('run writes the codes file, never overwrites it, and prints hash-only SQL', () => {
  const dir = outside();
  try {
    const out = join(dir, 'codes.txt');
    let printed = '';
    const { codes } = run(['--count', '4', '--out', out, '--label', 'wave 2'], { stdout: { write: s => { printed += s; } } });
    assert.deepEqual(readFileSync(out, 'utf8').trim().split('\n'), codes);
    for (const code of codes) assert.ok(!printed.includes(code));
    assert.equal((printed.match(/'[0-9a-f]{64}'/g) ?? []).length, 4);
    assert.throws(() => run(['--count', '1', '--out', out], { stdout: { write() {} } }), /already exists/);
    assert.equal(readFileSync(out, 'utf8').trim().split('\n').length, 4);

    const sqlPath = join(dir, 'batch.sql');
    writeFileSync(sqlPath, 'existing');
    const second = join(dir, 'codes-2.txt');
    assert.throws(() => run(['--count', '1', '--out', second, '--sql', sqlPath], { stdout: { write() {} } }), /already exists/);
    assert.equal(readFileSync(sqlPath, 'utf8'), 'existing');
    assert.equal(existsSync(second), false, 'no codes file is left behind without its SQL');
    assert.throws(() => run(['--count', '1', '--out', second, '--sql', second], { stdout: { write() {} } }), /different files/);

    const third = join(dir, 'codes-3.txt');
    const thirdSql = join(dir, 'batch-3.sql');
    const r = run(['--count', '3', '--out', third, '--sql', thirdSql], { stdout: { write() { throw new Error('stdout used'); } } });
    assert.equal((readFileSync(thirdSql, 'utf8').match(/'[0-9a-f]{64}'/g) ?? []).length, 3);
    assert.deepEqual(readFileSync(third, 'utf8').trim().split('\n'), r.codes);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
