import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { classifyDatabaseTarget } from '../src/database-target.mjs';
import { brokerDatabaseUrl } from '../src/server.mjs';

const projectRef = 'abcdefghijklmnopqrst';
const hostedRefusalMessage = 'the broker does not serve against a hosted database yet';
const hostedProjectUrl =
  `postgresql://postgres:synthetic-password@db.${projectRef}.supabase.co:5432/postgres`;
const hostedPoolerUrl =
  'postgres://postgres.synthetic:synthetic-password@aws-0-us-west-1.pooler.supabase.com:6543/postgres';

test('database target classification accepts and rejects only the specified shapes', () => {
  const cases = [
    {
      name: 'local localhost with password',
      value: 'postgres://kohbor_s2:local-secret@localhost:5432/kohbor_s2',
      target: 'local-synthetic',
    },
    {
      name: 'local IPv4 with implicit port',
      value: 'postgresql://kohbor_s2@127.0.0.1/kohbor_s2',
      target: 'local-synthetic',
    },
    {
      name: 'local IPv6',
      value: 'postgresql://kohbor_s2@[::1]:5432/kohbor_s2',
      target: 'local-synthetic',
    },
    { name: 'Supabase project host', value: hostedProjectUrl, target: 'supabase' },
    { name: 'Supabase pooler host', value: hostedPoolerUrl, target: 'supabase' },
    {
      name: 'uppercase Supabase hostname',
      value: `postgres://user:pw@DB.${projectRef.toUpperCase()}.SUPABASE.CO:5432/postgres`,
      target: 'supabase',
    },
    {
      name: 'pooler look-alike with attacker suffix',
      value: 'postgres://user:pw@evil.pooler.supabase.com.attacker.example:5432/postgres',
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'pooler look-alike with trailing domain',
      value: 'postgres://user:pw@aws-0.pooler.supabase.com.attacker.example:5432/postgres',
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'userinfo host spoof',
      value: `postgres://db.${projectRef}.supabase.co@attacker.example:5432/postgres`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'project host in password only',
      value: `postgres://user:db.${projectRef}.supabase.co@attacker.example:5432/postgres`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'wrong protocol',
      value: `mysql://user:pw@db.${projectRef}.supabase.co:5432/postgres`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'wrong port',
      value: `postgres://user:pw@db.${projectRef}.supabase.co:443/postgres`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'missing explicit hosted port',
      value: `postgres://user:pw@db.${projectRef}.supabase.co/postgres`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'wrong hosted database',
      value: `postgres://user:pw@db.${projectRef}.supabase.co:5432/kohbor_s2`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'TLS-disabling query string',
      value: `postgres://user:pw@db.${projectRef}.supabase.co:5432/postgres?sslmode=disable`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'fragment',
      value: `postgres://user:pw@db.${projectRef}.supabase.co:5432/postgres#fragment`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'wrong project reference length',
      value: 'postgres://user:pw@db.abcdefghijklmnopqrs.supabase.co:5432/postgres',
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'trailing hostname dot',
      value: `postgres://user:pw@db.${projectRef}.supabase.co.:5432/postgres`,
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'percent-encoded hostname',
      value: 'postgres://user:pw@db.%61bcdefghijklmnopqrst.supabase.co:5432/postgres',
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'IPv6 zone id',
      value: 'postgresql://kohbor_s2@[fe80::1%25eth0]:5432/kohbor_s2',
      error: 'invalid local database configuration',
    },
    {
      name: 'wrong local role',
      value: 'postgres://other:pw@localhost:5432/kohbor_s2',
      error: 'database must be the local synthetic S2 database',
    },
    {
      name: 'invalid URL',
      value: 'this is not a database URL',
      error: 'invalid local database configuration',
    },
  ];

  for (const { name, value, target, error } of cases) {
    if (error) {
      assert.throws(() => classifyDatabaseTarget(value), { message: error }, name);
      assert.throws(() => brokerDatabaseUrl(value), { message: error }, name);
      continue;
    }

    assert.equal(classifyDatabaseTarget(value), target, name);
    if (target === 'local-synthetic') {
      assert.equal(brokerDatabaseUrl(value), value, name);
    } else {
      assert.throws(() => brokerDatabaseUrl(value), {
        message: hostedRefusalMessage,
      }, name);
    }
  }
});

test('database target errors never include URL components', () => {
  const password = 'sentinel-password-acc-db-2';
  const username = 'sentinel-user-acc-db-2';
  const hostname = 'evil.pooler.supabase.com.attacker.example';
  const values = [
    `postgres://${username}:${password}@${hostname}:5432/postgres?sslmode=disable`,
    `postgres://${username}:${password}@db.abcdefghijklmnopqrst.supabase.co:5432/postgres?sslmode=disable`,
    `malformed-${username}-${password}`,
  ];

  for (const value of values) {
    let error;
    try {
      classifyDatabaseTarget(value);
    } catch (caught) {
      error = caught;
    }

    assert.ok(error instanceof Error);
    assert.equal(error.message.includes(value), false);
    for (const part of [username, password, hostname, 'sslmode=disable']) {
      assert.equal(error.message.includes(part), false);
    }
  }
});

test('server database guard accepts local synthetic and refuses hosted targets', () => {
  const localUrl = 'postgres://kohbor_s2:local-secret@127.0.0.1:5432/kohbor_s2';
  assert.equal(brokerDatabaseUrl(localUrl), localUrl);

  assert.throws(() => brokerDatabaseUrl(hostedProjectUrl), {
    message: hostedRefusalMessage,
  });
});

test('node src/server.mjs exits with a message when BROKER_MODE is unset', async () => {
  const serviceDirectory = fileURLToPath(new URL('..', import.meta.url));
  const env = { ...process.env };
  delete env.BROKER_MODE;
  delete env.DATABASE_URL;

  const result = await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ['src/server.mjs'], {
      cwd: serviceDirectory,
      env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
    child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
    child.once('error', reject);
    child.once('close', (code, signal) => resolve({ code, signal, stdout, stderr }));
  });

  assert.equal(result.signal, null);
  assert.notEqual(result.code, 0);
  assert.equal(
    result.stderr.trim(),
    'kohbor broker failed to start; check local synthetic configuration and database readiness',
  );
  assert.equal(result.stdout, '');
});
