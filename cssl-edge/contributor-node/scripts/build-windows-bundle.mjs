import {
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  sign as cryptoSign,
} from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

import { createDeterministicArchive } from './release-artifacts.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const outputDir = resolve(process.env.OUTPUT_DIR ?? join(root, 'candidate-dist'));
const outputName = process.env.OUTPUT_NAME ?? 'apocrypha-contributor-node-0.1.0-windows-x64.zip';
const nodeRuntime = resolve(process.env.NODE_RUNTIME_EXE ?? process.execPath);

async function file(name, source = join(root, name)) {
  return { name, data: await readFile(source) };
}

const entries = [
  { name: 'node.exe', data: await readFile(nodeRuntime) },
  await file('README.md'),
  await file('RELEASE.csl'),
  await file('package.json'),
  await file('controller-public-key.pem'),
  await file('run-once-apocrypha-node.cmd'),
  await file('start-apocrypha-node.cmd'),
  await file('dist/cli.js'),
  await file('dist/client.js'),
  await file('dist/runtime.js'),
];

const archive = createDeterministicArchive(entries);
await mkdir(outputDir, { recursive: true });
const archivePath = join(outputDir, outputName);
await writeFile(archivePath, archive.bytes);

const signer = process.env.RELEASE_SIGNER_PRIVATE_KEY_PEM
  ? (() => {
      const privateKey = createPrivateKey(process.env.RELEASE_SIGNER_PRIVATE_KEY_PEM);
      return { privateKey, publicKey: createPublicKey(privateKey) };
    })()
  : generateKeyPairSync('ed25519');
const signature = cryptoSign(null, archive.bytes, signer.privateKey).toString('base64url');
const publicKeyPem = signer.publicKey.export({ type: 'spki', format: 'pem' }).toString();
const signaturePath = `${archivePath}.sig`;
const publicKeyPath = `${archivePath}.signer.pub`;
await writeFile(signaturePath, `${signature}\n`, 'utf8');
await writeFile(publicKeyPath, publicKeyPem, 'utf8');

process.stdout.write(JSON.stringify({
  archive: {
    filename: basename(archivePath),
    path: archivePath,
    bytes: archive.size,
    sha256: archive.sha256,
    entries: archive.entries,
  },
  signature: { path: signaturePath, value: signature },
  signer_public_key: { path: publicKeyPath, pem: publicKeyPem },
  node_runtime: { path: nodeRuntime, bytes: entries[0].data.length },
}) + '\n');
