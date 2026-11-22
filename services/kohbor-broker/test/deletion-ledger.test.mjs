import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile, mkdir, rm, rmdir, stat } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createEncryptedDeletionLedger, DELETION_LEDGER_RETENTION_MS } from '../src/deletion-ledger.mjs';

const testRoot = resolve(dirname(fileURLToPath(import.meta.url)), `.tmp-ledger-${process.pid}-${randomUUID()}`);
const ledgerPath = resolve(testRoot, 'deletions.enc');
const pepper = 'synthetic-ledger-pepper-only';

test('encrypted deletion ledger replays by keyed hash and deletion time and waits for backup rotation', async () => {
  await mkdir(testRoot);
  try {
    const ledger = createEncryptedDeletionLedger({ filePath: ledgerPath, pepper });
    const now = Date.now();
    const recentDeletion = new Date(now - 31 * 24 * 60 * 60 * 1000);
    const olderDeletion = new Date(now - 32 * 24 * 60 * 60 * 1000);
    const recentHash = 'a'.repeat(64);
    const olderHash = 'b'.repeat(64);
    await ledger.append({ subjectHash: recentHash, deletedAt: recentDeletion });
    await ledger.append({ subjectHash: olderHash, deletedAt: olderDeletion });

    const encrypted = await readFile(ledgerPath, 'utf8');
    assert.equal(encrypted.includes(recentHash), false);
    assert.equal(encrypted.includes(olderHash), false);
    assert.equal(encrypted.includes(recentDeletion.toISOString()), false);
    assert.deepEqual(await ledger.entries(), [
      { subjectHash: recentHash, deletedAt: recentDeletion.toISOString() },
      { subjectHash: olderHash, deletedAt: olderDeletion.toISOString() },
    ]);
    await assert.rejects(
      createEncryptedDeletionLedger({ filePath: ledgerPath, pepper: 'different-pepper-material' }).entries(),
      /deletion ledger could not be read/,
    );

    const replayed = [];
    const replayCount = await ledger.replay({
      async query(sql, values = []) {
        if (sql.includes('to_regclass')) {
          return { rows: [{ accounts: 'accounts', login_identities: 'login_identities' }] };
        }
        assert.match(sql, /i\.subject_hash = \$1/);
        assert.match(sql, /a\.created_at < \$2/);
        replayed.push(values);
        return { rowCount: values[0] === recentHash ? 1 : 0 };
      },
    });
    assert.equal(replayCount, 1);
    assert.deepEqual(replayed, [
      [recentHash, recentDeletion],
      [olderHash, olderDeletion],
    ]);

    assert.deepEqual(await ledger.prune({ now }), {
      removed: 0, retained: 2, backupInventoryProvided: false,
    });
    const snapshotAt = new Date(now - 31.5 * 24 * 60 * 60 * 1000);
    assert.deepEqual(await ledger.prune({ now, retainedBackupTimes: [snapshotAt] }), {
      removed: 1, retained: 1, backupInventoryProvided: true,
    });
    assert.deepEqual(await ledger.entries(), [
      { subjectHash: recentHash, deletedAt: recentDeletion.toISOString() },
    ]);
    assert.equal(now - recentDeletion.getTime() >= DELETION_LEDGER_RETENTION_MS, true);
  } finally {
    await rm(ledgerPath, { force: true });
    await rmdir(testRoot);
  }
});

test('a missing deletion ledger is provisioned only for a fresh database', async () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), `.tmp-ledger-fresh-${process.pid}-${randomUUID()}`);
  await mkdir(root);
  const filePath = resolve(root, 'deletions.enc');
  try {
    const ledger = createEncryptedDeletionLedger({ filePath, pepper });
    const removed = await ledger.replay({
      async query(sql) {
        assert.match(sql, /to_regclass\('public\.accounts'\)/);
        return { rows: [{ accounts: null, login_identities: null, schema_migrations: null }] };
      },
    });
    assert.equal(removed, 0);
    const details = await stat(filePath);
    assert.equal(details.size, 0);
    if (process.platform !== 'win32') {
      assert.equal(details.mode & 0o777, 0o600);
    }
  } finally {
    await rm(filePath, { force: true });
    await rmdir(root);
  }
});

test('a missing deletion ledger refuses an initialized database without exposing row data', async () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), `.tmp-ledger-existing-${process.pid}-${randomUUID()}`);
  await mkdir(root);
  const filePath = resolve(root, 'deletions.enc');
  try {
    const ledger = createEncryptedDeletionLedger({ filePath, pepper });
    await assert.rejects(ledger.replay({
      async query(sql) {
        assert.match(sql, /to_regclass\('public\.accounts'\)/);
        return { rows: [{ accounts: 'accounts', login_identities: 'login_identities', schema_migrations: 'schema_migrations' }] };
      },
    }), /deletion ledger is missing for an initialized account database/);
    await assert.rejects(stat(filePath), error => error?.code === 'ENOENT');
  } finally {
    await rm(filePath, { force: true });
    await rmdir(root);
  }
});
