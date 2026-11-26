import { readFile } from 'node:fs/promises';
import { rootCertificates } from 'node:tls';
import { classifyDatabaseTarget } from '../src/database-target.mjs';
import { EXPECTED_MIGRATIONS, EXPECTED_PUBLIC_TABLES } from '../src/migrate.mjs';

const CHECK_FAILURE_MESSAGE = 'hosted database check failed';
const EXPOSED_SCHEMAS = Object.freeze(['public', 'graphql_public']);
const ALLOWED_GRAPHQL_FUNCTION = Object.freeze({
  schemaName: 'graphql_public',
  functionName: 'graphql',
  argumentTypes: 'text, text, jsonb, jsonb',
});

// Tables that exist only in the hosted Supabase project and are not broker
// migrations. Owner decision 2026-11-22: alpha_invites (alpha download gate).
// Owner request 2026-11-26: alpha_waitlist (Google sign-up queue) and alpha_download_links (shared no-code link)
// is allowed in public, but it must still have row level security on and no
// anon/authenticated grants, like every broker table.
export const SUPABASE_ONLY_PUBLIC_TABLES = Object.freeze(['alpha_invites', 'alpha_waitlist', 'alpha_download_links', 'alpha_admins']);

const TRANSACTION_STATE_SQL = `
  SELECT current_setting('transaction_read_only') = 'on' AS read_only,
         current_setting('statement_timeout')::interval = interval '5 seconds' AS statement_timeout_ok
`;

const EXPOSED_RELATIONS_SQL = `
  SELECT namespace.nspname AS schema_name,
         relation.relname AS name,
         relation.relkind AS relation_kind,
         relation.relrowsecurity AS row_level_security
    FROM pg_catalog.pg_class AS relation
    JOIN pg_catalog.pg_namespace AS namespace
      ON namespace.oid = relation.relnamespace
   WHERE namespace.nspname = ANY($1::text[])
     AND relation.relkind IN ('r', 'p', 'f', 'v', 'm')
   ORDER BY namespace.nspname, relation.relname
`;

const MIGRATIONS_SQL = `
  SELECT version, name
    FROM public.schema_migrations
   ORDER BY version
`;

const TABLE_GRANTS_SQL = `
  SELECT grant_role.rolname AS role_name,
         namespace.nspname AS schema_name,
         relation.relname AS table_name,
         privilege.privilege_type,
         CASE
           WHEN pg_catalog.has_table_privilege(
             grant_role.oid,
             relation.oid,
             privilege.privilege_type
           ) THEN 'table'
           ELSE 'column'
         END AS grant_type
    FROM pg_catalog.pg_roles AS grant_role
    JOIN pg_catalog.pg_class AS relation
      ON relation.relkind IN ('r', 'p', 'f', 'v', 'm')
    JOIN pg_catalog.pg_namespace AS namespace
      ON namespace.oid = relation.relnamespace
    CROSS JOIN (VALUES
      ('SELECT'), ('INSERT'), ('UPDATE'), ('DELETE'),
      ('TRUNCATE'), ('REFERENCES'), ('TRIGGER')
    ) AS privilege(privilege_type)
   WHERE grant_role.rolname IN ('anon', 'authenticated')
     AND namespace.nspname = ANY($1::text[])
     AND (
       pg_catalog.has_table_privilege(
         grant_role.oid, relation.oid, privilege.privilege_type
       ) OR CASE
         WHEN privilege.privilege_type IN ('SELECT', 'INSERT', 'UPDATE', 'REFERENCES')
           THEN pg_catalog.has_any_column_privilege(
           grant_role.oid, relation.oid, privilege.privilege_type
         )
         ELSE false
       END
     )
`;

const FUNCTION_PRIVILEGES_SQL = `
  SELECT namespace.nspname AS schema_name,
         routine.proname AS function_name,
         pg_catalog.oidvectortypes(routine.proargtypes) AS argument_types,
         routine.prosecdef AS security_definer,
         pg_catalog.has_function_privilege(
           anon_role.oid, routine.oid, 'EXECUTE'
         ) AS anon_execute,
         pg_catalog.has_function_privilege(
           authenticated_role.oid, routine.oid, 'EXECUTE'
         ) AS authenticated_execute
    FROM pg_catalog.pg_proc AS routine
    JOIN pg_catalog.pg_namespace AS namespace
      ON namespace.oid = routine.pronamespace
    CROSS JOIN pg_catalog.pg_roles AS anon_role
    CROSS JOIN pg_catalog.pg_roles AS authenticated_role
   WHERE namespace.nspname = ANY($1::text[])
     AND routine.prokind IN ('f', 'p')
     AND anon_role.rolname = 'anon'
     AND authenticated_role.rolname = 'authenticated'
`;

const SEQUENCE_GRANTS_SQL = `
  SELECT grant_role.rolname AS role_name,
         namespace.nspname AS schema_name,
         sequence_relation.relname AS sequence_name,
         privilege.privilege_type
    FROM pg_catalog.pg_roles AS grant_role
    JOIN pg_catalog.pg_class AS sequence_relation
      ON sequence_relation.relkind = 'S'
    JOIN pg_catalog.pg_namespace AS namespace
      ON namespace.oid = sequence_relation.relnamespace
    CROSS JOIN (VALUES ('USAGE'), ('SELECT'), ('UPDATE'))
      AS privilege(privilege_type)
   WHERE grant_role.rolname IN ('anon', 'authenticated')
     AND namespace.nspname = ANY($1::text[])
     AND pg_catalog.has_sequence_privilege(
       grant_role.oid, sequence_relation.oid, privilege.privilege_type
     )
`;

const ROW_COUNT_ACCESS_SQL = `
  SELECT role_info.rolbypassrls AS bypass_rls,
         count(relation.oid) = 2 AS has_both_tables,
         COALESCE(bool_and(relation.relowner = role_info.oid), false) AS owns_tables,
         COALESCE(bool_and(NOT relation.relforcerowsecurity), false)
           AS row_security_not_forced
    FROM pg_catalog.pg_roles AS role_info
    LEFT JOIN pg_catalog.pg_class AS relation
      ON relation.relnamespace = (
        SELECT namespace.oid
          FROM pg_catalog.pg_namespace AS namespace
         WHERE namespace.nspname = 'public'
      )
     AND relation.relkind = 'r'
     AND relation.relname IN ('accounts', 'login_identities')
   WHERE role_info.rolname = current_user
   GROUP BY role_info.oid, role_info.rolbypassrls
`;

const COUNTS_SQL = `
  SELECT (SELECT count(*)::text FROM public.accounts) AS accounts,
         (SELECT count(*)::text FROM public.login_identities) AS login_identities
`;

export function attachNoopErrorListener(emitter) {
  emitter.on('error', () => {});
}

export function installCheckProcessErrorHandlers(processObject = process) {
  const failSafely = () => {
    processObject.stderr.write(`${CHECK_FAILURE_MESSAGE}\n`);
    processObject.exitCode = 1;
  };
  processObject.on('uncaughtException', failSafely);
  processObject.on('unhandledRejection', failSafely);
  return () => {
    processObject.off('uncaughtException', failSafely);
    processObject.off('unhandledRejection', failSafely);
  };
}

export function buildCheckPoolOptions(connectionString, ssl) {
  return {
    connectionString,
    ssl,
    max: 1,
    connectionTimeoutMillis: 5000,
    query_timeout: 5000,
    application_name: 'kohbor-supabase-read-only-check',
  };
}

function isCount(value) {
  return (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) ||
    (typeof value === 'string' && /^\d+$/.test(value));
}

function expectedMigrationsMatch(rows) {
  if (!Array.isArray(rows) || rows.length !== EXPECTED_MIGRATIONS.length) return false;
  const byVersion = new Map();
  for (const row of rows) {
    if (!Number.isInteger(row?.version) || typeof row.name !== 'string' ||
        byVersion.has(row.version)) return false;
    byVersion.set(row.version, row.name);
  }
  return EXPECTED_MIGRATIONS.every(({ version, name }) => byVersion.get(version) === name);
}

function tableRowsMatch(rows) {
  if (!Array.isArray(rows)) return false;
  const names = rows.map(row => row?.name);
  const actual = new Set(names);
  if (actual.size !== names.length) return false;
  const allowed = new Set([...EXPECTED_PUBLIC_TABLES, ...SUPABASE_ONLY_PUBLIC_TABLES]);
  return EXPECTED_PUBLIC_TABLES.every(name => actual.has(name)) &&
    names.every(name => allowed.has(name));
}

function isAllowlistedGraphqlFunction(row) {
  return row?.schema_name === ALLOWED_GRAPHQL_FUNCTION.schemaName &&
    row?.function_name === ALLOWED_GRAPHQL_FUNCTION.functionName &&
    row?.argument_types === ALLOWED_GRAPHQL_FUNCTION.argumentTypes;
}

export function compareSupabaseSnapshot(snapshot) {
  const tables = Array.isArray(snapshot?.tables) ? snapshot.tables : [];
  const publicViewsAvailable = Array.isArray(snapshot?.publicViews);
  const publicViews = publicViewsAvailable ? snapshot.publicViews : [];
  const tableByName = new Map(tables.map(row => [row?.name, row]));
  const rlsEnabledCount = EXPECTED_PUBLIC_TABLES.filter(
    name => tableByName.get(name)?.rowLevelSecurity === true,
  ).length;
  const tableGrantCount = Array.isArray(snapshot?.tableGrants)
    ? snapshot.tableGrants.length
    : null;
  const functionsAvailable = Array.isArray(snapshot?.functions);
  const functions = functionsAvailable ? snapshot.functions : [];
  const functionRowsValid = functionsAvailable && functions.every(row =>
    typeof row?.schema_name === 'string' &&
    typeof row?.function_name === 'string' &&
    typeof row?.argument_types === 'string' &&
    typeof row?.security_definer === 'boolean' &&
    typeof row?.anon_execute === 'boolean' &&
    typeof row?.authenticated_execute === 'boolean'
  );
  const functionExecuteGrantCount = functionsAvailable
    ? functions.reduce((count, row) => count +
      (row?.anon_execute === true ? 1 : 0) +
      (row?.authenticated_execute === true ? 1 : 0), 0)
    : null;
  const functionPrivilegesOk = functionRowsValid && functions.every(row => {
    const hasClientExecute = row.anon_execute || row.authenticated_execute;
    return (!hasClientExecute || isAllowlistedGraphqlFunction(row)) &&
      (!isAllowlistedGraphqlFunction(row) || row.security_definer === false);
  });
  const sequenceGrantCount = Array.isArray(snapshot?.sequenceGrants)
    ? snapshot.sequenceGrants.length
    : null;
  const graphqlPublicRelationCount = Array.isArray(snapshot?.graphqlPublicRelations)
    ? snapshot.graphqlPublicRelations.length
    : null;
  const countAccessVerified = snapshot?.rowCountAccess?.canRead === true;
  const accounts = countAccessVerified && isCount(snapshot?.counts?.accounts)
    ? snapshot.counts.accounts
    : null;
  const loginIdentities = countAccessVerified && isCount(snapshot?.counts?.login_identities)
    ? snapshot.counts.login_identities
    : null;
  const supabaseOnlyPresent = SUPABASE_ONLY_PUBLIC_TABLES.filter(name => tableByName.has(name));
  const supabaseOnlyRlsOk = supabaseOnlyPresent.every(
    name => tableByName.get(name)?.rowLevelSecurity === true,
  );
  const migrationCount = Array.isArray(snapshot?.migrations) ? snapshot.migrations.length : 0;
  const tableCount = tables.length;
  const failedChecks = [];

  if (snapshot?.transaction?.readOnly !== true) failedChecks.push('read-only transaction');
  if (snapshot?.transaction?.statementTimeoutOk !== true) failedChecks.push('statement timeout');
  if (!expectedMigrationsMatch(snapshot?.migrations)) failedChecks.push('migrations');
  if (!tableRowsMatch(tables)) failedChecks.push('public tables');
  if (!publicViewsAvailable || publicViews.length > 0) failedChecks.push('public views');
  if (rlsEnabledCount !== EXPECTED_PUBLIC_TABLES.length) failedChecks.push('row level security');
  if (!supabaseOnlyRlsOk) failedChecks.push('Supabase-only table row level security');
  if (tableGrantCount !== 0) failedChecks.push('anon/authenticated table grants');
  if (!functionPrivilegesOk) failedChecks.push('anon/authenticated function EXECUTE grants');
  if (sequenceGrantCount !== 0) failedChecks.push('anon/authenticated sequence grants');
  if (graphqlPublicRelationCount !== 0) failedChecks.push('graphql_public relations');
  if (accounts === null || loginIdentities === null) failedChecks.push('row counts');

  return {
    ok: failedChecks.length === 0,
    failedChecks,
    migrationCount,
    tableCount,
    publicViewCount: publicViewsAvailable ? publicViews.length : null,
    rlsEnabledCount,
    supabaseOnlyPresent,
    supabaseOnlyRlsOk,
    tableGrantCount,
    functionExecuteGrantCount,
    sequenceGrantCount,
    graphqlPublicRelationCount,
    counts: { accounts, login_identities: loginIdentities },
  };
}

async function readSnapshot(client) {
  const transactionState = (await client.query(TRANSACTION_STATE_SQL)).rows[0] ?? {};
  const relations = (await client.query(EXPOSED_RELATIONS_SQL, [EXPOSED_SCHEMAS])).rows;
  const publicRelations = relations.filter(row => row.schema_name === 'public');
  const tables = publicRelations.filter(row => ['r', 'p', 'f'].includes(row.relation_kind));
  const publicViews = publicRelations
    .filter(row => ['v', 'm'].includes(row.relation_kind))
    .map(row => row.name);
  const graphqlPublicRelations = relations.filter(row => row.schema_name === 'graphql_public');
  const tableByName = new Map(tables.map(row => [row.name, row]));
  const migrations = tableByName.get('schema_migrations')?.relation_kind === 'r'
    ? (await client.query(MIGRATIONS_SQL)).rows
    : [];
  const tableGrants = (await client.query(TABLE_GRANTS_SQL, [EXPOSED_SCHEMAS])).rows;
  const functions = (await client.query(FUNCTION_PRIVILEGES_SQL, [EXPOSED_SCHEMAS])).rows;
  const sequenceGrants = (await client.query(SEQUENCE_GRANTS_SQL, [EXPOSED_SCHEMAS])).rows;
  const accessRow = (await client.query(ROW_COUNT_ACCESS_SQL)).rows[0] ?? {};
  const rowCountAccess = {
    canRead: accessRow.bypass_rls === true ||
      (accessRow.has_both_tables === true && accessRow.owns_tables === true &&
        accessRow.row_security_not_forced === true),
  };
  const counts = rowCountAccess.canRead &&
      tableByName.get('accounts')?.relation_kind === 'r' &&
      tableByName.get('login_identities')?.relation_kind === 'r'
    ? (await client.query(COUNTS_SQL)).rows[0] ?? null
    : null;

  return {
    transaction: {
      readOnly: transactionState.read_only === true,
      statementTimeoutOk: transactionState.statement_timeout_ok === true,
    },
    migrations,
    tables: tables.map(row => ({
      name: row.name,
      rowLevelSecurity: row.row_level_security === true,
    })),
    publicViews,
    graphqlPublicRelations,
    tableGrants,
    functions,
    sequenceGrants,
    rowCountAccess,
    counts,
  };
}

function printSummary(result) {
  const migrations = EXPECTED_MIGRATIONS.map(({ version, name }) => `${version}:${name}`).join(', ');
  console.log(`expected migrations: ${migrations}; observed rows: ${result.migrationCount}`);
  console.log(`expected public tables: ${EXPECTED_PUBLIC_TABLES.join(', ')}; observed: ${result.tableCount}`);
  console.log(`public views/materialized views: ${result.publicViewCount ?? 'unavailable'}`);
  console.log(`row level security enabled: ${result.rlsEnabledCount}/${EXPECTED_PUBLIC_TABLES.length}`);
  console.log(`allowed Supabase-only tables present: ${result.supabaseOnlyPresent.join(', ') || 'none'}; row level security: ${result.supabaseOnlyRlsOk ? 'on' : 'OFF'}`);
  console.log(`anon/authenticated table grants: ${result.tableGrantCount ?? 'unavailable'}`);
  console.log(`anon/authenticated function EXECUTE grants: ${result.functionExecuteGrantCount ?? 'unavailable'}`);
  console.log(`anon/authenticated sequence grants: ${result.sequenceGrantCount ?? 'unavailable'}`);
  console.log(`graphql_public relations: ${result.graphqlPublicRelationCount ?? 'unavailable'}`);
  console.log(`accounts rows: ${result.counts.accounts ?? 'unavailable'}`);
  console.log(`login_identities rows: ${result.counts.login_identities ?? 'unavailable'}`);
  console.log(`check: ${result.ok ? 'PASS' : `FAIL (${result.failedChecks.join(', ')})`}`);
}

export async function inspectPool(pool) {
  let client;
  let inTransaction = false;
  try {
    client = await pool.connect();
    attachNoopErrorListener(client);
    await client.query('BEGIN TRANSACTION READ ONLY');
    inTransaction = true;
    await client.query("SET LOCAL statement_timeout = '5000ms'");
    const snapshot = await readSnapshot(client);
    await client.query('COMMIT');
    inTransaction = false;
    return compareSupabaseSnapshot(snapshot);
  } catch (error) {
    if (inTransaction && client) await client.query('ROLLBACK').catch(() => {});
    if (client) {
      client.release(error);
      client = undefined;
    }
    throw error;
  } finally {
    client?.release();
  }
}

async function main() {
  let pool;
  try {
    const databaseUrl = process.env.SUPABASE_DATABASE_URL;
    if (classifyDatabaseTarget(databaseUrl) !== 'supabase') {
      throw new Error('hosted database URL is required');
    }

    const ssl = { rejectUnauthorized: true };
    if (process.env.SUPABASE_CA_FILE) {
      let extraCa;
      try {
        extraCa = await readFile(process.env.SUPABASE_CA_FILE, 'utf8');
      } catch {
        throw new Error('hosted database CA file could not be read');
      }
      ssl.ca = [...rootCertificates, extraCa];
    }

    const { Pool } = await import('pg');
    pool = new Pool(buildCheckPoolOptions(databaseUrl, ssl));
    attachNoopErrorListener(pool);
    const result = await inspectPool(pool);
    printSummary(result);
    if (!result.ok) process.exitCode = 1;
  } catch {
    process.stderr.write(`${CHECK_FAILURE_MESSAGE}\n`);
    process.exitCode = 1;
  } finally {
    await pool?.end().catch(() => {});
  }
}

async function run() {
  const removeProcessErrorHandlers = installCheckProcessErrorHandlers();
  try {
    await main();
  } catch {
    process.stderr.write(`${CHECK_FAILURE_MESSAGE}\n`);
    process.exitCode = 1;
  } finally {
    removeProcessErrorHandlers();
  }
}

if (import.meta.main) {
  void run();
}
