#!/usr/bin/env node
// Offline generator for alpha download invite codes (see ../README.md).
// It never connects to a database and needs no secret. It writes:
//   - the plain codes to --out (a new file outside the repository; the codes are credentials);
//   - an INSERT for public.alpha_invites holding only SHA-256 hashes, to --sql or stdout.
// The code format, normalization and hash must stay identical to
// ../functions/alpha-download/index.ts; the tests check this against that file.
import { createHash, randomBytes } from 'node:crypto';
import { existsSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';

// Crockford base32: digits and capitals without I, L, O, U (32 symbols).
export const ALPHABET = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
export const CODE_SHAPE = /^KB-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}-[0-9A-HJKMNP-TV-Z]{4}$/;
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const MAX_COUNT = 500;

export function makeCode(random = randomBytes) {
  // 256 is a multiple of 32, so `byte & 31` picks every symbol with equal probability.
  const bytes = random(12);
  let s = '';
  for (let i = 0; i < 12; i += 1) s += ALPHABET[bytes[i] & 31];
  return `KB-${s.slice(0, 4)}-${s.slice(4, 8)}-${s.slice(8, 12)}`;
}

// Same steps as normalize() in the alpha-download function.
export function normalize(raw) {
  if (typeof raw !== 'string' || raw.length > 40) return null;
  let s = raw.toUpperCase().replace(/[\s_]/g, '').replace(/O/g, '0').replace(/[IL]/g, '1');
  s = s.replace(/-/g, '');
  if (s.startsWith('KB')) s = s.slice(2);
  if (!/^[0-9A-HJKMNP-TV-Z]{12}$/.test(s)) return null;
  const code = `KB-${s.slice(0, 4)}-${s.slice(4, 8)}-${s.slice(8, 12)}`;
  return CODE_SHAPE.test(code) ? code : null;
}

export function codeHash(code) {
  return createHash('sha256').update(code, 'utf8').digest('hex');
}

export function makeCodes(count, random = randomBytes) {
  const codes = new Set();
  while (codes.size < count) codes.add(makeCode(random));
  return [...codes];
}

function sqlString(value) {
  return `'${String(value).replace(/'/g, "''")}'`;
}

export function buildInsertSql(codes, { label = null, maxDownloads = 3, expiresAt = null } = {}) {
  const rows = codes.map(code => {
    const values = [
      sqlString(codeHash(code)),
      label === null ? 'NULL' : sqlString(label),
      String(maxDownloads),
      expiresAt === null ? 'NULL' : `${sqlString(expiresAt)}::timestamptz`,
    ];
    return `  (${values.join(', ')})`;
  });
  return 'INSERT INTO public.alpha_invites (code_hash, label, max_downloads, expires_at) VALUES\n' +
    `${rows.join(',\n')};\n`;
}

export function insideRepo(path, root = REPO_ROOT) {
  const rel = relative(root, resolve(path));
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
}

export function parseArgs(argv) {
  const opts = { count: null, out: null, sql: null, label: null, maxDownloads: 3, expiresAt: null };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (value === undefined || value.startsWith('--')) throw new Error(`${flag} needs a value`);
    i += 1;
    if (flag === '--count') opts.count = Number(value);
    else if (flag === '--out') opts.out = value;
    else if (flag === '--sql') opts.sql = value;
    else if (flag === '--label') opts.label = value;
    else if (flag === '--max-downloads') opts.maxDownloads = Number(value);
    else if (flag === '--expires') opts.expiresAt = value;
    else throw new Error(`unknown option ${flag}`);
  }
  if (!Number.isInteger(opts.count) || opts.count < 1 || opts.count > MAX_COUNT) {
    throw new Error(`--count must be an integer from 1 to ${MAX_COUNT}`);
  }
  if (!opts.out) throw new Error('--out is required (a new file outside the repository for the plain codes)');
  for (const path of [opts.out, opts.sql].filter(Boolean)) {
    if (insideRepo(path)) throw new Error(`${path} is inside the repository; write codes and SQL outside it`);
  }
  if (opts.label !== null && (opts.label.length < 1 || opts.label.length > 64)) {
    throw new Error('--label must be 1 to 64 characters');
  }
  if (!Number.isInteger(opts.maxDownloads) || opts.maxDownloads < 1 || opts.maxDownloads > 100) {
    throw new Error('--max-downloads must be an integer from 1 to 100');
  }
  if (opts.expiresAt !== null) {
    const when = Date.parse(opts.expiresAt);
    if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?(\.\d+)?(Z|[+-]\d{2}:\d{2})$/.test(opts.expiresAt) || Number.isNaN(when)) {
      throw new Error('--expires must be an ISO 8601 time with a zone, e.g. 2026-12-31T23:59:59+07:00');
    }
    if (when <= Date.now()) throw new Error('--expires must be in the future');
  }
  return opts;
}

export function run(argv, { random = randomBytes, stdout = process.stdout } = {}) {
  const opts = parseArgs(argv);
  // Refuse before writing anything, so a batch never ends up with codes but no matching SQL.
  for (const path of [opts.out, opts.sql].filter(Boolean)) {
    if (existsSync(path)) throw new Error(`${path} already exists; choose a new file`);
  }
  if (opts.sql && resolve(opts.sql) === resolve(opts.out)) throw new Error('--sql and --out must be different files');
  const codes = makeCodes(opts.count, random);
  const sql = buildInsertSql(codes, opts);
  // 'wx' refuses to overwrite an existing file, so an earlier batch of codes is never lost.
  writeFileSync(opts.out, `${codes.join('\n')}\n`, { encoding: 'utf8', flag: 'wx' });
  if (opts.sql) writeFileSync(opts.sql, sql, { encoding: 'utf8', flag: 'wx' });
  else stdout.write(sql);
  return { codes, sql };
}

if (import.meta.main) {
  try {
    const { codes } = run(process.argv.slice(2));
    process.stderr.write(`wrote ${codes.length} codes; give each to one person and keep the file private\n`);
  } catch (error) {
    process.stderr.write(`make-invite-codes: ${error.message}\n`);
    process.exitCode = 1;
  }
}
