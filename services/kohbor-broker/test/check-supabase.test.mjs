import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { EventEmitter } from 'node:events';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import {
  attachNoopErrorListener,
  buildCheckPoolOptions,
  compareSupabaseSnapshot,
  inspectPool,
  installCheckProcessErrorHandlers,
} from '../scripts/check-supabase.mjs';
import { attachBrokerPoolErrorListener } from '../src/server.mjs';
import { EXPECTED_MIGRATIONS, EXPECTED_PUBLIC_TABLES } from '../src/migrate.mjs';

function validSnapshot() {
  return {
    transaction: { readOnly: true, statementTimeoutOk: true },
    migrations: EXPECTED_MIGRATIONS.map(({ version, name }) => ({ version, name })),
    tables: EXPECTED_PUBLIC_TABLES.map(name => ({ name, rowLevelSecurity: true })),
    publicViews: [],
    graphqlPublicRelations: [],
    tableGrants: [],
    functions: [{
      schema_name: 'graphql_public',
      function_name: 'graphql',
      argument_types: 'text, text, jsonb, jsonb',
      security_definer: false,
      anon_execute: true,
      authenticated_execute: true,
    }],
    sequenceGrants: [],
    rowCountAccess: { canRead: true },
    counts: { accounts: '0', login_identities: '0' },
  };
}

function fakeSnapshotClient({
  relations = EXPECTED_PUBLIC_TABLES.map(name => ({
    schema_name: 'public',
    name,
    relation_kind: 'r',
    row_level_security: true,
  })),
  accessRow = {
    bypass_rls: true,
    has_both_tables: true,
    owns_tables: false,
    row_security_not_forced: false,
  },
} = {}) {
  const queries = [];
  const client = new EventEmitter();
  client.query = async (statement, values) => {
    queries.push({ statement, values });
    if (statement === 'BEGIN TRANSACTION READ ONLY' ||
        statement === "SET LOCAL statement_timeout = '5000ms'" ||
        statement === 'COMMIT') return { rows: [] };
    if (statement.includes("current_setting('transaction_read_only')")) {
      return { rows: [{ read_only: true, statement_timeout_ok: true }] };
    }
    if (statement.includes('relation.relkind AS relation_kind')) return { rows: relations };
    if (statement.includes('FROM public.schema_migrations')) {
      return { rows: EXPECTED_MIGRATIONS.map(({ version, name }) => ({ version, name })) };
    }
    if (statement.includes('has_table_privilege')) return { rows: [] };
    if (statement.includes('has_function_privilege')) {
      return {
        rows: [{
          schema_name: 'graphql_public',
          function_name: 'graphql',
          argument_types: 'text, text, jsonb, jsonb',
          security_definer: false,
          anon_execute: true,
          authenticated_execute: true,
        }],
      };
    }
    if (statement.includes('has_sequence_privilege')) return { rows: [] };
    if (statement.includes('relation.relforcerowsecurity')) return { rows: [accessRow] };
    if (statement.includes('SELECT (SELECT count(*)::text FROM public.accounts)')) {
      return { rows: [{ accounts: '0', login_identities: '0' }] };
    }
    throw new Error('unhandled synthetic query');
  };
  client.release = () => {};
  return { pool: { connect: async () => client }, queries };
}

test('Supabase comparison accepts the expected read-only database state', () => {
  const result = compareSupabaseSnapshot(validSnapshot());
  assert.equal(result.ok, true);
  assert.deepEqual(result.failedChecks, []);
  assert.equal(result.migrationCount, 3);
  assert.equal(result.tableCount, 14);
  assert.equal(result.rlsEnabledCount, 14);
  assert.equal(result.tableGrantCount, 0);
  assert.equal(result.functionExecuteGrantCount, 2);
  assert.equal(result.sequenceGrantCount, 0);
  assert.equal(result.graphqlPublicRelationCount, 0);
  assert.deepEqual(result.counts, { accounts: '0', login_identities: '0' });
});

test('Supabase comparison accepts graphql_public.graphql with its expected signature', () => {
  const snapshot = validSnapshot();
  const result = compareSupabaseSnapshot(snapshot);

  assert.equal(result.ok, true);
  assert.equal(result.functionExecuteGrantCount, 2);
});

test('Supabase comparison rejects a missing table', () => {
  const snapshot = validSnapshot();
  snapshot.tables = snapshot.tables.filter(row => row.name !== 'share_entries');
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('public tables'));
});

test('Supabase comparison rejects an extra table', () => {
  const snapshot = validSnapshot();
  snapshot.tables.push({ name: 'unexpected_table', rowLevelSecurity: true });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('public tables'));
});

test('Supabase comparison accepts the allowed Supabase-only alpha_invites table with RLS on', () => {
  const snapshot = validSnapshot();
  snapshot.tables.push({ name: 'alpha_invites', rowLevelSecurity: true });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, true);
  assert.deepEqual(result.failedChecks, []);
  assert.equal(result.tableCount, 15);
  assert.deepEqual(result.supabaseOnlyPresent, ['alpha_invites']);
});

test('Supabase comparison rejects alpha_invites when a broker table is missing', () => {
  const snapshot = validSnapshot();
  snapshot.tables = snapshot.tables.filter(row => row.name !== 'share_entries');
  snapshot.tables.push({ name: 'alpha_invites', rowLevelSecurity: true });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('public tables'));
});

test('Supabase comparison rejects alpha_invites with RLS off', () => {
  const snapshot = validSnapshot();
  snapshot.tables.push({ name: 'alpha_invites', rowLevelSecurity: false });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('Supabase-only table row level security'));
});

test('Supabase comparison rejects a client grant on alpha_invites', () => {
  const snapshot = validSnapshot();
  snapshot.tables.push({ name: 'alpha_invites', rowLevelSecurity: true });
  snapshot.tableGrants.push({
    role_name: 'anon',
    table_name: 'alpha_invites',
    privilege_type: 'SELECT',
  });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated table grants'));
});

test('Supabase comparison rejects a column-level grant on alpha_invites', () => {
  const snapshot = validSnapshot();
  snapshot.tables.push({ name: 'alpha_invites', rowLevelSecurity: true });
  snapshot.tableGrants.push({
    role_name: 'authenticated',
    schema_name: 'public',
    table_name: 'alpha_invites',
    privilege_type: 'SELECT',
    grant_type: 'column',
    column_name: 'invite_code',
  });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated table grants'));
});

test('Supabase comparison rejects an anon-executable function in public', () => {
  const snapshot = validSnapshot();
  snapshot.functions.push({
    schema_name: 'public',
    function_name: 'unsafe_rpc',
    argument_types: '',
    security_definer: false,
    anon_execute: true,
    authenticated_execute: false,
  });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated function EXECUTE grants'));
});

test('Supabase comparison rejects an authenticated-executable function in public', () => {
  const snapshot = validSnapshot();
  snapshot.functions.push({
    schema_name: 'public',
    function_name: 'unsafe_rpc',
    argument_types: '',
    security_definer: false,
    anon_execute: false,
    authenticated_execute: true,
  });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated function EXECUTE grants'));
});

test('Supabase comparison rejects a different executable function in graphql_public', () => {
  const snapshot = validSnapshot();
  snapshot.functions.push({
    schema_name: 'graphql_public',
    function_name: 'other_rpc',
    argument_types: 'text',
    security_definer: false,
    anon_execute: true,
    authenticated_execute: false,
  });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated function EXECUTE grants'));
});

test('Supabase comparison does not allow an executable graphql overload', () => {
  const snapshot = validSnapshot();
  snapshot.functions.push({
    schema_name: 'graphql_public',
    function_name: 'graphql',
    argument_types: 'text',
    security_definer: false,
    anon_execute: true,
    authenticated_execute: false,
  });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated function EXECUTE grants'));
});

test('Supabase comparison rejects an allowlisted graphql signature that is SECURITY DEFINER', () => {
  const snapshot = validSnapshot();
  snapshot.functions[0].security_definer = true;

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated function EXECUTE grants'));
});

test('Supabase comparison rejects a duplicate table name', () => {
  const snapshot = validSnapshot();
  snapshot.tables.push({ name: 'accounts', rowLevelSecurity: true });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('public tables'));
});

test('Supabase comparison rejects row-level security being off', () => {
  const snapshot = validSnapshot();
  snapshot.tables[0].rowLevelSecurity = false;
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('row level security'));
});

test('Supabase comparison rejects a grant to anon or authenticated', () => {
  const snapshot = validSnapshot();
  snapshot.tableGrants.push({
    role_name: 'anon',
    table_name: 'accounts',
    privilege_type: 'SELECT',
  });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated table grants'));
});

test('Supabase comparison rejects a relation grant in graphql_public', () => {
  const snapshot = validSnapshot();
  snapshot.tableGrants.push({
    role_name: 'anon',
    schema_name: 'graphql_public',
    table_name: 'unexpected_relation',
    privilege_type: 'SELECT',
  });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated table grants'));
});

test('Supabase comparison rejects a table in graphql_public', () => {
  const snapshot = validSnapshot();
  snapshot.graphqlPublicRelations.push({ name: 'unexpected_table', relation_kind: 'r' });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('graphql_public relations'));
});

test('Supabase comparison rejects a sequence grant', () => {
  const snapshot = validSnapshot();
  snapshot.sequenceGrants.push({
    role_name: 'authenticated',
    schema_name: 'public',
    sequence_name: 'unexpected_id_seq',
    privilege_type: 'USAGE',
  });

  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.equal(result.sequenceGrantCount, 1);
  assert.ok(result.failedChecks.includes('anon/authenticated sequence grants'));
});

test('Supabase comparison rejects an unexpected migration name', () => {
  const snapshot = validSnapshot();
  snapshot.migrations[1].name = 'wrong-migration-name';
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('migrations'));
});

test('Supabase comparison rejects a duplicate migration version', () => {
  const snapshot = validSnapshot();
  snapshot.migrations.push({ ...snapshot.migrations[0] });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('migrations'));
});

test('Supabase comparison rejects an unavailable grants result', () => {
  const snapshot = validSnapshot();
  snapshot.tableGrants = null;
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated table grants'));
});

test('Supabase comparison rejects a transaction that is not read-only', () => {
  const snapshot = validSnapshot();
  snapshot.transaction.readOnly = false;
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('read-only transaction'));
});

test('Supabase comparison rejects missing account counts', () => {
  const snapshot = validSnapshot();
  snapshot.counts = { accounts: '0' };
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('row counts'));
});

test('Supabase comparison rejects a column-level grant', () => {
  const snapshot = validSnapshot();
  snapshot.tableGrants.push({
    role_name: 'authenticated',
    table_name: 'accounts',
    privilege_type: 'SELECT',
    grant_type: 'column',
    column_name: 'display_name',
  });
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('anon/authenticated table grants'));
});

test('Supabase comparison rejects a public view', () => {
  const snapshot = validSnapshot();
  snapshot.publicViews.push('account_view');
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('public views'));
});

test('Supabase comparison marks RLS-hidden counts unavailable and fails closed', () => {
  const snapshot = validSnapshot();
  snapshot.rowCountAccess = { canRead: false };
  snapshot.counts = null;
  const result = compareSupabaseSnapshot(snapshot);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('row counts'));
  assert.deepEqual(result.counts, { accounts: null, login_identities: null });
});

test('FORCE RLS keeps counts unavailable for an owning role without BYPASSRLS', async () => {
  const fake = fakeSnapshotClient({
    accessRow: {
      bypass_rls: false,
      has_both_tables: true,
      owns_tables: true,
      row_security_not_forced: false,
    },
  });

  const result = await inspectPool(fake.pool);
  assert.equal(result.ok, false);
  assert.ok(result.failedChecks.includes('row counts'));
  assert.deepEqual(result.counts, { accounts: null, login_identities: null });
  assert.ok(fake.queries.some(({ statement }) => statement.includes('relation.relforcerowsecurity')));
  assert.equal(
    fake.queries.some(({ statement }) => statement.includes('SELECT (SELECT count(*)::text FROM public.accounts)')),
    false,
  );
});

test('snapshot reading keeps graphql_public relations out of public table expectations', async () => {
  const fake = fakeSnapshotClient({
    relations: [
      ...EXPECTED_PUBLIC_TABLES.map(name => ({
        schema_name: 'public',
        name,
        relation_kind: 'r',
        row_level_security: true,
      })),
      { schema_name: 'public', name: 'unexpected_view', relation_kind: 'v', row_level_security: false },
      { schema_name: 'graphql_public', name: 'unexpected_table', relation_kind: 'r', row_level_security: false },
    ],
  });

  const result = await inspectPool(fake.pool);
  assert.equal(result.tableCount, EXPECTED_PUBLIC_TABLES.length);
  assert.equal(result.publicViewCount, 1);
  assert.equal(result.graphqlPublicRelationCount, 1);
  assert.ok(result.failedChecks.includes('graphql_public relations'));
  assert.ok(fake.queries.some(({ statement, values }) =>
    statement.includes('namespace.nspname = ANY($1::text[])') &&
    JSON.stringify(values) === JSON.stringify([['public', 'graphql_public']])),
  );
});

test('check Pool options enforce query_timeout at five seconds', () => {
  const options = buildCheckPoolOptions('synthetic database URL', { rejectUnauthorized: true });
  assert.equal(options.query_timeout, 5000);
  assert.deepEqual(options.ssl, { rejectUnauthorized: true });
});

test('pool and client socket error listeners consume errors without output', () => {
  const pool = new EventEmitter();
  const client = new EventEmitter();
  attachNoopErrorListener(pool);
  attachNoopErrorListener(client);

  assert.doesNotThrow(() => pool.emit('error', new Error('secret host and password')));
  assert.doesNotThrow(() => client.emit('error', new Error('secret host and password')));
});

test('check process handlers print a fixed error and remove themselves afterward', () => {
  const processLike = new EventEmitter();
  const messages = [];
  processLike.stderr = { write: message => messages.push(message) };
  processLike.exitCode = 0;
  const removeHandlers = installCheckProcessErrorHandlers(processLike);

  processLike.emit('uncaughtException', new Error('secret host and password'));
  assert.equal(processLike.exitCode, 1);
  processLike.exitCode = 0;
  processLike.emit('unhandledRejection', new Error('secret host and password'));
  assert.equal(processLike.exitCode, 1);
  assert.deepEqual(messages, [
    'hosted database check failed\n',
    'hosted database check failed\n',
  ]);

  removeHandlers();
  assert.equal(processLike.listenerCount('uncaughtException'), 0);
  assert.equal(processLike.listenerCount('unhandledRejection'), 0);
});

test('server pool error listener writes only its fixed safe message', () => {
  const pool = new EventEmitter();
  const messages = [];
  attachBrokerPoolErrorListener(pool, message => messages.push(message));

  pool.emit('error', new Error('secret host and password'));
  assert.deepEqual(messages, ['kohbor broker database connection failed\n']);
});

test('inspection destroys the client after a query failure', async () => {
  const socketError = new Error('synthetic socket failure');
  const releasedWith = [];
  const client = new EventEmitter();
  client.query = async statement => {
    if (statement === 'BEGIN TRANSACTION READ ONLY') return { rows: [] };
    if (statement === "SET LOCAL statement_timeout = '5000ms'") return { rows: [] };
    if (statement === 'ROLLBACK') return { rows: [] };
    throw socketError;
  };
  client.release = error => releasedWith.push(error);
  const pool = { connect: async () => client };

  await assert.rejects(inspectPool(pool), error => error === socketError);
  assert.deepEqual(releasedWith, [socketError]);
});

test('checker entry exits with its fixed message when its URL is absent', async () => {
  const serviceDirectory = fileURLToPath(new URL('..', import.meta.url));
  const env = { ...process.env };
  delete env.SUPABASE_DATABASE_URL;
  delete env.SUPABASE_CA_FILE;

  const result = await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ['scripts/check-supabase.mjs'], {
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
  assert.equal(result.code, 1);
  assert.equal(result.stdout, '');
  assert.equal(result.stderr.trim(), 'hosted database check failed');
});
