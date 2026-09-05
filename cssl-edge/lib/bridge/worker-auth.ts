import type { IncomingHttpHeaders } from 'node:http';
import { randomUUID } from 'node:crypto';
import { BridgeError, bridgeMac, equalMac, sha256, type BridgeConfiguration } from './crypto';
import type { NextApiRequest, NextApiResponse } from 'next';

const NONCE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
export const WORKER_POLL_PATH = '/api/bridge/worker/poll';
export const WORKER_COMPLETE_PATH = '/api/bridge/worker/complete';
export interface WorkerAuthentication { nonce: string; time: number; }
export function workerAuthText(config: BridgeConfiguration, method: string, path: string, body: Uint8Array, time: string, nonce: string): string {
  return ['apocky.bridge.worker-auth.v1', config.keyId, config.workerId, time, nonce, method, path, sha256(body)].join('\n');
}
export function workerHeaders(config: BridgeConfiguration, path: string, body: Uint8Array, now = Date.now(), nonce: string = randomUUID()): Record<string, string> {
  const time = String(Math.floor(now / 1000));
  return { 'x-apocky-bridge-key-id': config.keyId, 'x-apocky-worker-id': config.workerId, 'x-apocky-worker-time': time,
    'x-apocky-worker-nonce': nonce, 'x-apocky-worker-mac': bridgeMac(config, 'worker-auth', workerAuthText(config, 'POST', path, body, time, nonce)) };
}
export function verifyWorkerAuthentication(config: BridgeConfiguration, headers: IncomingHttpHeaders, method: string, path: string, body: Uint8Array, now = Date.now()): WorkerAuthentication {
  const time = headers['x-apocky-worker-time']; const nonce = headers['x-apocky-worker-nonce']; const mac = headers['x-apocky-worker-mac'];
  if (method !== 'POST' || ![WORKER_POLL_PATH, WORKER_COMPLETE_PATH].includes(path)
    || headers['x-apocky-bridge-key-id'] !== config.keyId || headers['x-apocky-worker-id'] !== config.workerId
    || typeof time !== 'string' || !/^[1-9][0-9]{0,11}$/.test(time) || !Number.isSafeInteger(Number(time))
    || Math.abs(Math.floor(now / 1000) - Number(time)) > 60 || typeof nonce !== 'string' || !NONCE.test(nonce)
    || typeof mac !== 'string' || !equalMac(mac, bridgeMac(config, 'worker-auth', workerAuthText(config, method, path, body, time, nonce)))) {
    throw new BridgeError('BRIDGE_WORKER_UNAUTHORIZED', 401);
  }
  return { nonce, time: Number(time) };
}

export async function readWorkerBody(req: NextApiRequest, limit: number): Promise<Buffer> {
  if (req.headers['content-type'] !== 'application/json' || req.headers['content-encoding'] !== undefined) throw new BridgeError('BRIDGE_CONTENT_TYPE_INVALID', 415);
  const length = req.headers['content-length'];
  if (length !== undefined && (typeof length !== 'string' || !/^(0|[1-9][0-9]*)$/.test(length) || Number(length) > limit)) throw new BridgeError('BRIDGE_PAYLOAD_TOO_LARGE', 413);
  const chunks: Buffer[] = []; let total = 0;
  const timeout = setTimeout(() => req.destroy(), 10_000);
  try {
    for await (const chunk of req) {
      const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      total += bytes.length;
      if (total > limit) throw new BridgeError('BRIDGE_PAYLOAD_TOO_LARGE', 413);
      chunks.push(bytes);
    }
    if (length !== undefined && Number(length) !== total) throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
    return Buffer.concat(chunks);
  } finally { clearTimeout(timeout); }
}
export function workerResponseHeaders(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive');
  res.setHeader('Allow', 'POST');
}
export function workerFailure(res: NextApiResponse, error: unknown): void {
  const known = error instanceof BridgeError;
  res.status(known ? error.status : 503).json({ schema_version: 'apocky.bridge.error.v1', code: known ? error.code : 'BRIDGE_UNAVAILABLE' });
}
