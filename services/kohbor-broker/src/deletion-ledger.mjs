import { createCipheriv, createDecipheriv, createHmac, randomBytes, randomUUID } from 'node:crypto';
import { open, readFile, rename, unlink } from 'node:fs/promises';
import { isAbsolute, resolve } from 'node:path';

export const DELETION_LEDGER_RETENTION_MS = 30 * 24 * 60 * 60 * 1000;
const DELETION_LEDGER_KEY_DOMAIN = 'kohbor-deletion-ledger-encryption-v1';
const LEDGER_VERSION = 1;

function entryValue(entry) {
  if (!entry || typeof entry !== 'object' ||
      !/^[0-9a-f]{64}$/.test(entry.subjectHash ?? '')) {
    throw new TypeError('invalid deletion ledger entry');
  }
  const deletedAt = new Date(entry.deletedAt);
  if (!Number.isFinite(deletedAt.getTime())) throw new TypeError('invalid deletion ledger entry');
  return { subjectHash: entry.subjectHash, deletedAt: deletedAt.toISOString() };
}

function encryptionKey(pepper) {
  return createHmac('sha256', pepper).update(DELETION_LEDGER_KEY_DOMAIN).digest();
}

function encryptEntry(entry, key) {
  const iv = randomBytes(12);
  const cipher = createCipheriv('aes-256-gcm', key, iv);
  const ciphertext = Buffer.concat([
    cipher.update(JSON.stringify(entryValue(entry)), 'utf8'),
    cipher.final(),
  ]);
  return JSON.stringify({
    version: LEDGER_VERSION,
    iv: iv.toString('base64url'),
    tag: cipher.getAuthTag().toString('base64url'),
    ciphertext: ciphertext.toString('base64url'),
  });
}

function decodeBase64Url(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9_-]+$/.test(value)) return null;
  const decoded = Buffer.from(value, 'base64url');
  return decoded.toString('base64url') === value ? decoded : null;
}

function decryptEntry(line, key) {
  try {
    const envelope = JSON.parse(line);
    if (envelope?.version !== LEDGER_VERSION) throw new Error();
    const iv = decodeBase64Url(envelope.iv);
    const tag = decodeBase64Url(envelope.tag);
    const ciphertext = decodeBase64Url(envelope.ciphertext);
    if (!iv || iv.length !== 12 || !tag || tag.length !== 16 || !ciphertext) throw new Error();
    const decipher = createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(tag);
    const plaintext = Buffer.concat([decipher.update(ciphertext), decipher.final()]).toString('utf8');
    const parsed = JSON.parse(plaintext);
    if (Object.keys(parsed).sort().join(',') !== 'deletedAt,subjectHash') throw new Error();
    return entryValue(parsed);
  } catch {
    throw new Error('deletion ledger could not be read');
  }
}

async function readEntries(path, key) {
  const contents = await readFile(path, 'utf8');
  if (contents === '') return [];
  return contents.split(/\r?\n/).filter(Boolean).map(line => decryptEntry(line, key));
}

async function writeEntries(path, entries, key) {
  const temporaryPath = `${path}.${process.pid}.${randomUUID()}.tmp`;
  let handle;
  try {
    handle = await open(temporaryPath, 'wx', 0o600);
    const contents = entries.map(entry => encryptEntry(entry, key)).join('\n');
    if (contents) await handle.writeFile(`${contents}\n`, 'utf8');
    await handle.sync();
    await handle.close();
    handle = undefined;
    await rename(temporaryPath, path);
  } catch (error) {
    await handle?.close().catch(() => {});
    await unlink(temporaryPath).catch(() => {});
    throw error;
  }
}

export function createEncryptedDeletionLedger({ filePath, pepper }) {
  if (typeof filePath !== 'string' || !isAbsolute(filePath) ||
      typeof pepper !== 'string' || Buffer.byteLength(pepper) < 16) {
    throw new TypeError('deletion ledger requires an absolute path and the configured subject pepper');
  }
  const path = resolve(filePath);
  const key = encryptionKey(pepper);

  async function createEmptyFile() {
    let handle;
    try {
      handle = await open(path, 'wx', 0o600);
      await handle.chmod(0o600);
      await handle.sync();
      await handle.close();
      handle = undefined;
    } catch (error) {
      if (error?.code !== 'EEXIST') throw new Error('deletion ledger could not be provisioned');
    } finally {
      await handle?.close();
    }
  }

  async function entriesForReplay(pool) {
    try {
      return await readEntries(path, key);
    } catch (error) {
      if (error?.code !== 'ENOENT') throw error;
      const tables = await pool.query(`
        SELECT to_regclass('public.accounts') AS accounts,
               to_regclass('public.login_identities') AS login_identities,
               to_regclass('public.schema_migrations') AS schema_migrations
      `);
      const state = tables.rows[0];
      if (state.accounts == null && state.login_identities == null && state.schema_migrations == null) {
        await createEmptyFile();
        try {
          return await readEntries(path, key);
        } catch (readError) {
          if (readError?.code !== 'ENOENT') throw readError;
          throw new Error('deletion ledger could not be provisioned');
        }
      }
      throw new Error('deletion ledger is missing for an initialized account database');
    }
  }

  return Object.freeze({
    async append(entry) {
      const line = encryptEntry(entry, key);
      const handle = await open(path, 'a', 0o600);
      try {
        await handle.chmod(0o600);
        await handle.writeFile(`${line}\n`, 'utf8');
        await handle.sync();
      } finally {
        await handle.close();
      }
    },

    async remove(entry) {
      let currentEntries;
      try {
        currentEntries = await readEntries(path, key);
      } catch (error) {
        if (error?.code === 'ENOENT') return false;
        throw error;
      }
      const wanted = entryValue(entry);
      const index = currentEntries.findIndex(candidate =>
        candidate.subjectHash === wanted.subjectHash && candidate.deletedAt === wanted.deletedAt);
      if (index === -1) return false;
      currentEntries.splice(index, 1);
      await writeEntries(path, currentEntries, key);
      return true;
    },

    async entries() {
      return readEntries(path, key);
    },

    async replay(pool) {
      const entries = await entriesForReplay(pool);
      const tables = await pool.query(`
        SELECT to_regclass('public.accounts') AS accounts,
               to_regclass('public.login_identities') AS login_identities
      `);
      const { accounts, login_identities: loginIdentities } = tables.rows[0];
      if (!accounts && !loginIdentities) return 0;
      if (!accounts || !loginIdentities) throw new Error('deletion ledger replay found an incomplete account schema');

      let removed = 0;
      for (const entry of entries) {
        const result = await pool.query(
          `DELETE FROM accounts a
            USING login_identities i
           WHERE a.id = i.account_id
             AND i.subject_hash = $1
             AND a.created_at < $2`,
          [entry.subjectHash, new Date(entry.deletedAt)],
        );
        removed += result.rowCount;
      }
      return removed;
    },

    async prune({ now = Date.now(), retainedBackupTimes } = {}) {
      const currentEntries = await readEntries(path, key);
      if (!Array.isArray(retainedBackupTimes)) {
        return { removed: 0, retained: currentEntries.length, backupInventoryProvided: false };
      }
      const backupTimes = retainedBackupTimes.map(value => new Date(value).getTime());
      if (backupTimes.some(value => !Number.isFinite(value))) {
        throw new TypeError('invalid retained backup timestamp');
      }
      const retained = currentEntries.filter(entry => {
        const deletedAt = new Date(entry.deletedAt).getTime();
        const minimumAgeReached = now - deletedAt >= DELETION_LEDGER_RETENTION_MS;
        const predatingBackupRemains = backupTimes.some(snapshotAt => snapshotAt <= deletedAt);
        return !minimumAgeReached || predatingBackupRemains;
      });
      const removed = currentEntries.length - retained.length;
      if (removed > 0) await writeEntries(path, retained, key);
      return { removed, retained: retained.length, backupInventoryProvided: true };
    },
  });
}
