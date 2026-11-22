export const HOSTED_DATABASE_NOT_SUPPORTED =
  'the broker does not serve against a hosted database yet';

const LOCAL_DATABASE_URL_INVALID = 'invalid local database configuration';
const LOCAL_DATABASE_REQUIRED = 'database must be the local synthetic S2 database';
const POOLER_HOST = /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.pooler\.supabase\.com$/;
const PROJECT_HOST = /^db\.[a-z]{20}\.supabase\.co$/;

function isLocalSynthetic(url) {
  const host = url.hostname.toLowerCase();
  const allowedHosts = new Set(['localhost', '127.0.0.1', '::1', '[::1]']);
  return ['postgres:', 'postgresql:'].includes(url.protocol) && allowedHosts.has(host) &&
    url.username === 'kohbor_s2' && (!url.port || url.port === '5432') &&
    !url.search && !url.hash && url.pathname === '/kohbor_s2';
}

function isSupabase(url) {
  const host = url.hostname.toLowerCase();
  return ['postgres:', 'postgresql:'].includes(url.protocol) &&
    (POOLER_HOST.test(host) || PROJECT_HOST.test(host)) &&
    ['5432', '6543'].includes(url.port) &&
    url.pathname === '/postgres' && !url.search && !url.hash;
}

export function classifyDatabaseTarget(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error(LOCAL_DATABASE_URL_INVALID);
  }

  if (isLocalSynthetic(url)) return 'local-synthetic';
  if (isSupabase(url)) return 'supabase';
  throw new Error(LOCAL_DATABASE_REQUIRED);
}
