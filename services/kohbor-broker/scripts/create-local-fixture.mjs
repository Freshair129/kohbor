import { generateKeyPairSync, randomBytes } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const outputPath = process.argv[2];
if (!outputPath) {
  process.stderr.write('usage: node scripts/create-local-fixture.mjs <absolute-env-file-outside-service-folder>\n');
  process.exit(2);
}

const serviceRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const resolvedOutput = resolve(outputPath);
const relativeOutput = relative(serviceRoot, resolvedOutput);
const isInsideService = relativeOutput === '' ||
  (!relativeOutput.startsWith(`..${sep}`) && relativeOutput !== '..' && !isAbsolute(relativeOutput));
if (isInsideService) {
  process.stderr.write('fixture environment must be written outside the service folder\n');
  process.exit(2);
}

const identity = generateKeyPairSync('rsa', {
  modulusLength: 2048,
  publicKeyEncoding: { type: 'spki', format: 'pem' },
  privateKeyEncoding: { type: 'pkcs8', format: 'pem' },
});
const broker = generateKeyPairSync('ed25519', {
  publicKeyEncoding: { type: 'spki', format: 'pem' },
  privateKeyEncoding: { type: 'pkcs8', format: 'pem' },
});

const env = [
  'BROKER_MODE=local-synthetic',
  'DATABASE_URL=postgresql://kohbor_s2@127.0.0.1:5432/kohbor_s2',
  'BROKER_IDENTITY_ISSUER=synthetic-issuer',
  'BROKER_IDENTITY_AUDIENCE=synthetic-client',
  `BROKER_IDENTITY_PUBLIC_KEY='${identity.publicKey.trimEnd()}'`,
  `BROKER_SIGNING_PRIVATE_KEY='${broker.privateKey.trimEnd()}'`,
  `BROKER_SUBJECT_PEPPER=${randomBytes(32).toString('base64url')}`,
  'HOST=127.0.0.1',
  'PORT=8080',
  '',
].join('\n');

await writeFile(resolvedOutput, env, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
process.stdout.write(`Wrote local synthetic fixture configuration: ${resolvedOutput}\n`);
