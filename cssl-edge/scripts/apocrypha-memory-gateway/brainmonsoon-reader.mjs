#!/usr/bin/env node

import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import {
  lstat, open, readdir, realpath,
} from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { TextDecoder } from 'node:util';

const REQUEST_SCHEMA = 'apocrypha.memory-gateway.brainmonsoon-reader-request.v1';
const RESPONSE_SCHEMA = 'apocrypha.memory-gateway.brainmonsoon-reader-response.v1';
const NATIVE_REQUEST_SCHEMA = 'apocrypha.brainmonsoon.service-request.v1';
const NATIVE_RESPONSE_SCHEMA = 'apocrypha.brainmonsoon.service-response.v1';

const MAX_INPUT_BYTES = 16_384;
const MAX_NATIVE_OUTPUT_BYTES = 393_216;
const MAX_NATIVE_ERROR_BYTES = 16_384;
const MAX_RESPONSE_BYTES = 524_288;
const MAX_STATE_FILES = 4_096;
const MAX_STATE_DIRECTORIES = 1_024;
const MAX_STATE_BYTES = 67_108_864;
const MAX_STATE_DEPTH = 16;
const MIN_DEADLINE_MS = 100;
const MAX_DEADLINE_MS = 30_000;
const MAX_REQUESTED_LIMIT = 8;
const FILE_READ_BYTES = 65_536;
const SHA256 = /^[0-9a-f]{64}$/u;
const REQUEST_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/u;
const ALLOWED_METHODS = new Set(['health', 'status', 'recall']);
const REQUEST_KEYS = new Set([
  'schema_version', 'request_id', 'method',
  'executable_path', 'executable_sha256',
  'registry_path', 'registry_sha256',
  'csl_path', 'csl_sha256',
  'nil_path', 'nil_sha256',
  'cssl_path', 'cssl_sha256',
  'state_root', 'query', 'limit', 'deadline_ms',
]);

class ReaderError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

function fail(code, message) {
  throw new ReaderError(code, message);
}

function objectRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function checkDeadline(deadlineAt) {
  if (Date.now() >= deadlineAt) fail('BRAIN_READER_DEADLINE', 'read-only operation exceeded its deadline');
}

function remainingMs(deadlineAt) {
  checkDeadline(deadlineAt);
  return Math.max(1, deadlineAt - Date.now());
}

async function readOneFrame() {
  const chunks = [];
  let length = 0;
  for await (const value of process.stdin) {
    const chunk = Buffer.from(value);
    length += chunk.length;
    if (length > MAX_INPUT_BYTES) fail('BRAIN_READER_FRAME_LIMIT', 'request exceeds the fixed frame limit');
    chunks.push(chunk);
  }
  const bytes = Buffer.concat(chunks, length);
  if (bytes.length < 2 || bytes.at(-1) !== 0x0a || bytes.includes(0x0d)
      || bytes.subarray(0, -1).includes(0x0a)) {
    fail('BRAIN_READER_FRAME_INVALID', 'one LF-terminated JSON line followed by EOF is required');
  }
  try {
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, -1));
    return JSON.parse(text);
  } catch {
    fail('BRAIN_READER_FRAME_INVALID', 'request is not valid UTF-8 JSON');
  }
}

function validateRequest(value) {
  if (!objectRecord(value)) fail('BRAIN_READER_REQUEST_INVALID', 'request must be a JSON object');
  const keys = Object.keys(value);
  if (keys.length !== REQUEST_KEYS.size || keys.some((key) => !REQUEST_KEYS.has(key))) {
    fail('BRAIN_READER_REQUEST_INVALID', 'request does not match the closed schema');
  }
  if (value.schema_version !== REQUEST_SCHEMA || typeof value.request_id !== 'string'
      || !REQUEST_ID.test(value.request_id)) {
    fail('BRAIN_READER_IDENTITY_INVALID', 'schema or request identifier is invalid');
  }
  if (typeof value.method !== 'string' || !ALLOWED_METHODS.has(value.method)) {
    fail('BRAIN_READER_METHOD_DENIED', 'only health, status, and recall are admitted');
  }
  for (const name of ['executable_path', 'registry_path', 'csl_path', 'nil_path', 'cssl_path', 'state_root']) {
    if (typeof value[name] !== 'string' || value[name].length === 0 || value[name].length > 1_024
        || value[name].includes('\0') || !path.isAbsolute(value[name])) {
      fail('BRAIN_READER_PATH_INVALID', 'all native paths must be bounded absolute paths');
    }
  }
  for (const name of ['executable_sha256', 'registry_sha256', 'csl_sha256', 'nil_sha256', 'cssl_sha256']) {
    if (typeof value[name] !== 'string' || !SHA256.test(value[name])) {
      fail('BRAIN_READER_PIN_INVALID', 'all executable and contract pins must be lowercase SHA-256 values');
    }
  }
  if (typeof value.query !== 'string' || Buffer.byteLength(value.query, 'utf8') > 64
      || (value.method === 'health' ? value.query !== '' : !SHA256.test(value.query))) {
    fail('BRAIN_READER_QUERY_INVALID', 'health requires an empty query; status and recall require one lineage SHA-256');
  }
  if (!Number.isSafeInteger(value.limit) || value.limit < 1 || value.limit > MAX_REQUESTED_LIMIT) {
    fail('BRAIN_READER_LIMIT_INVALID', `limit must be an integer from 1 through ${MAX_REQUESTED_LIMIT}`);
  }
  if (!Number.isSafeInteger(value.deadline_ms)
      || value.deadline_ms < MIN_DEADLINE_MS || value.deadline_ms > MAX_DEADLINE_MS) {
    fail('BRAIN_READER_DEADLINE_INVALID', `deadline_ms must be ${MIN_DEADLINE_MS}-${MAX_DEADLINE_MS}`);
  }
  return value;
}

function sameIdentity(left, right) {
  return left.dev === right.dev && left.ino === right.ino && left.size === right.size
    && left.mtimeNs === right.mtimeNs && left.ctimeNs === right.ctimeNs;
}

async function stableFileSha256(filePath, maximumBytes, deadlineAt) {
  const before = await lstat(filePath, { bigint: true });
  if (!before.isFile() || before.isSymbolicLink() || before.size > BigInt(maximumBytes)) {
    fail('BRAIN_READER_PATH_INVALID', 'native artifact must be a bounded regular file');
  }
  const handle = await open(filePath, 'r');
  const hash = createHash('sha256');
  const buffer = Buffer.allocUnsafe(FILE_READ_BYTES);
  try {
    let position = 0;
    while (true) {
      checkDeadline(deadlineAt);
      const { bytesRead } = await handle.read(buffer, 0, buffer.length, position);
      if (bytesRead === 0) break;
      hash.update(buffer.subarray(0, bytesRead));
      position += bytesRead;
    }
    const openAfter = await handle.stat({ bigint: true });
    const pathAfter = await lstat(filePath, { bigint: true });
    if (!sameIdentity(before, openAfter) || !sameIdentity(openAfter, pathAfter)) {
      fail('BRAIN_READER_PIN_CHANGED', 'native artifact changed during verification');
    }
    return { sha256: hash.digest('hex'), stat: pathAfter };
  } finally {
    await handle.close();
  }
}

async function canonicalNoLink(candidate, expected) {
  const direct = await lstat(candidate, { bigint: true });
  if (direct.isSymbolicLink()
      || (expected === 'file' && !direct.isFile())
      || (expected === 'directory' && !direct.isDirectory())) {
    fail('BRAIN_READER_PATH_INVALID', `native ${expected} path is not a direct ${expected}`);
  }
  return realpath(candidate);
}

function inside(root, candidate) {
  const relative = path.relative(root, candidate);
  return relative !== '' && !relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative);
}

async function validatePaths(request, deadlineAt) {
  const executable = await canonicalNoLink(request.executable_path, 'file');
  const packageRoot = path.dirname(path.dirname(executable));
  if (path.basename(executable).toLowerCase() !== 'apocrypha-brainmonsoon-organ.exe') {
    fail('BRAIN_READER_EXECUTABLE_INVALID', 'executable name is not the admitted Brainmonsoon organ');
  }
  const artifacts = [
    ['executable_path', 'executable_sha256', 134_217_728],
    ['registry_path', 'registry_sha256', 8_388_608],
    ['csl_path', 'csl_sha256', 8_388_608],
    ['nil_path', 'nil_sha256', 8_388_608],
    ['cssl_path', 'cssl_sha256', 8_388_608],
  ];
  const resolved = {};
  for (const [pathName, hashName, maximumBytes] of artifacts) {
    checkDeadline(deadlineAt);
    const canonical = await canonicalNoLink(request[pathName], 'file');
    if (pathName !== 'executable_path' && !inside(packageRoot, canonical)) {
      fail('BRAIN_READER_PACKAGE_INVALID', 'contracts and registry must reside inside the executable package');
    }
    const verified = await stableFileSha256(canonical, maximumBytes, deadlineAt);
    if (verified.sha256 !== request[hashName]) fail('BRAIN_READER_PIN_MISMATCH', 'native artifact hash differs from its pin');
    resolved[pathName] = canonical;
  }
  const stateRoot = await canonicalNoLink(request.state_root, 'directory');
  const state = await lstat(stateRoot, { bigint: true });
  if (!state.isDirectory() || state.isSymbolicLink() || path.basename(stateRoot).toLowerCase() !== 'brainmonsoon') {
    fail('BRAIN_READER_STATE_INVALID', 'state_root must be the existing Brainmonsoon state directory');
  }
  if (inside(packageRoot, stateRoot) || stateRoot === packageRoot) {
    fail('BRAIN_READER_STATE_INVALID', 'protected state must be separate from executable artifacts');
  }
  return { ...resolved, state_root: stateRoot, packageRoot };
}

async function snapshotState(root, deadlineAt) {
  const hash = createHash('sha256');
  let files = 0;
  let directories = 0;
  let bytes = 0;
  const visit = async (directory, relativeDirectory, depth) => {
    checkDeadline(deadlineAt);
    if (depth > MAX_STATE_DEPTH) fail('BRAIN_READER_STATE_LIMIT', 'state directory depth exceeds the read-only bound');
    directories += 1;
    if (directories > MAX_STATE_DIRECTORIES) fail('BRAIN_READER_STATE_LIMIT', 'state directory count exceeds the read-only bound');
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name, 'en'));
    for (const entry of entries) {
      checkDeadline(deadlineAt);
      const relative = relativeDirectory ? path.posix.join(relativeDirectory, entry.name) : entry.name;
      const absolute = path.join(directory, entry.name);
      const state = await lstat(absolute, { bigint: true });
      if (state.isSymbolicLink()) fail('BRAIN_READER_STATE_INVALID', 'symbolic links are not admitted in protected state');
      if (state.isDirectory()) {
        hash.update(`D\0${relative}\0`);
        await visit(absolute, relative, depth + 1);
      } else if (state.isFile()) {
        files += 1;
        bytes += Number(state.size);
        if (files > MAX_STATE_FILES || bytes > MAX_STATE_BYTES) {
          fail('BRAIN_READER_STATE_LIMIT', 'protected state exceeds the read-only verification bound');
        }
        const verified = await stableFileSha256(absolute, MAX_STATE_BYTES, deadlineAt);
        if (!sameIdentity(state, verified.stat)) {
          fail('BRAIN_READER_STATE_CHANGED', 'protected state changed during verification');
        }
        hash.update(`F\0${relative}\0${state.size.toString()}\0${verified.sha256}\0`);
      } else {
        fail('BRAIN_READER_STATE_INVALID', 'protected state contains a non-file entry');
      }
    }
  };
  await visit(root, '', 0);
  return { sha256: hash.digest('hex'), files, directories, bytes };
}

function closedEnvironment() {
  const environment = { NODE_ENV: 'production' };
  for (const name of ['SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'USERPROFILE', 'LOCALAPPDATA', 'APPDATA']) {
    if (process.env[name]) environment[name] = process.env[name];
  }
  return environment;
}

function nativeFrame(request) {
  const frame = {
    schema_version: NATIVE_REQUEST_SCHEMA,
    request_id: request.request_id,
    method: request.method,
  };
  if (request.method !== 'health') frame.lineage_key_sha256 = request.query;
  return frame;
}

async function invokeNative(request, resolved, deadlineAt) {
  const args = [
    'serve',
    '--registry', resolved.registry_path,
    '--csl', resolved.csl_path,
    '--nil', resolved.nil_path,
    '--cssl', resolved.cssl_path,
    '--state-root', resolved.state_root,
  ];
  const frame = `${JSON.stringify(nativeFrame(request))}\n`;
  return new Promise((resolve, reject) => {
    const child = spawn(resolved.executable_path, args, {
      cwd: resolved.packageRoot,
      env: closedEnvironment(),
      shell: false,
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    const stdout = [];
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let timedOut = false;
    let settled = false;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill('SIGKILL');
    }, remainingMs(deadlineAt));
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (error) reject(error); else resolve(value);
    };
    child.once('error', () => finish(new ReaderError('BRAIN_READER_NATIVE_UNAVAILABLE', 'native reader could not start')));
    child.stdout.on('data', (value) => {
      const chunk = Buffer.from(value);
      stdoutBytes += chunk.length;
      if (stdoutBytes > MAX_NATIVE_OUTPUT_BYTES) {
        child.kill('SIGKILL');
        finish(new ReaderError('BRAIN_READER_NATIVE_OUTPUT_LIMIT', 'native response exceeded the fixed bound'));
      } else {
        stdout.push(chunk);
      }
    });
    child.stderr.on('data', (value) => {
      stderrBytes += Buffer.byteLength(value);
      if (stderrBytes > MAX_NATIVE_ERROR_BYTES) {
        child.kill('SIGKILL');
        finish(new ReaderError('BRAIN_READER_NATIVE_ERROR_LIMIT', 'native diagnostics exceeded the fixed bound'));
      }
    });
    child.once('close', (code) => {
      if (settled) return;
      if (timedOut) return finish(new ReaderError('BRAIN_READER_DEADLINE', 'native reader exceeded its deadline'));
      if (code !== 0) return finish(new ReaderError('BRAIN_READER_NATIVE_FAILED', 'native reader exited unsuccessfully'));
      const bytes = Buffer.concat(stdout, stdoutBytes);
      if (bytes.length < 2 || bytes.at(-1) !== 0x0a || bytes.includes(0x0d)
          || bytes.subarray(0, -1).includes(0x0a)) {
        return finish(new ReaderError('BRAIN_READER_NATIVE_FRAME_INVALID', 'native reader returned an invalid frame'));
      }
      try {
        const payload = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, -1)));
        if (!objectRecord(payload) || payload.schema_version !== NATIVE_RESPONSE_SCHEMA
            || payload.request_id !== request.request_id || payload.method !== request.method
            || typeof payload.ok !== 'boolean') {
          return finish(new ReaderError('BRAIN_READER_NATIVE_RESPONSE_INVALID', 'native response identity is invalid'));
        }
        return finish(undefined, payload);
      } catch (error) {
        return finish(error instanceof ReaderError ? error
          : new ReaderError('BRAIN_READER_NATIVE_FRAME_INVALID', 'native reader returned invalid JSON'));
      }
    });
    child.stdin.once('error', () => {
      child.kill('SIGKILL');
      finish(new ReaderError('BRAIN_READER_NATIVE_INPUT_FAILED', 'native request could not be delivered'));
    });
    child.stdin.end(frame, 'utf8');
  });
}

function safeIdentity(value) {
  if (!objectRecord(value)) return { request_id: null, method: null };
  return {
    request_id: typeof value.request_id === 'string' && REQUEST_ID.test(value.request_id) ? value.request_id : null,
    method: typeof value.method === 'string' && ALLOWED_METHODS.has(value.method) ? value.method : null,
  };
}

function errorPayload(identity, error) {
  const admitted = error instanceof ReaderError ? error : new ReaderError('BRAIN_READER_FAILED', 'read-only reader failed');
  return {
    schema_version: RESPONSE_SCHEMA,
    request_id: identity.request_id,
    method: identity.method,
    ok: false,
    read_only: true,
    authority: 'read_only_analysis',
    error: { code: admitted.code, message: admitted.message },
  };
}

async function execute() {
  let untrusted;
  try {
    untrusted = await readOneFrame();
    const request = validateRequest(untrusted);
    const deadlineAt = Date.now() + request.deadline_ms;
    const resolved = await validatePaths(request, deadlineAt);
    const before = await snapshotState(resolved.state_root, deadlineAt);
    const native = await invokeNative(request, resolved, deadlineAt);
    const after = await snapshotState(resolved.state_root, deadlineAt);
    if (before.sha256 !== after.sha256 || before.files !== after.files
        || before.directories !== after.directories || before.bytes !== after.bytes) {
      fail('BRAIN_READER_STATE_CHANGED', 'protected state changed during a read-only request');
    }
    return {
      schema_version: RESPONSE_SCHEMA,
      request_id: request.request_id,
      method: request.method,
      ok: native.ok === true,
      read_only: true,
      authority: 'read_only_analysis',
      requested_limit: request.limit,
      effective_limit: 1,
      state: {
        before_sha256: before.sha256,
        after_sha256: after.sha256,
        unchanged: true,
        files: after.files,
        directories: after.directories,
        bytes: after.bytes,
      },
      native_response: native,
    };
  } catch (error) {
    return errorPayload(safeIdentity(untrusted), error);
  }
}

function writeOneFrame(value) {
  let line = JSON.stringify(value);
  if (Buffer.byteLength(line, 'utf8') + 1 > MAX_RESPONSE_BYTES) {
    line = JSON.stringify(errorPayload(safeIdentity(value),
      new ReaderError('BRAIN_READER_RESPONSE_LIMIT', 'response exceeded the fixed frame limit')));
  }
  process.stdout.write(`${line}\n`);
}

writeOneFrame(await execute());
