import type { NextApiRequest, NextApiResponse } from 'next';
import { BridgeError, exactObject } from '@/lib/bridge/crypto';
import { configuredBridgeQueue, type BridgeQueue } from '@/lib/bridge/queue';
import { readWorkerBody, verifyWorkerAuthentication, workerFailure, workerResponseHeaders, WORKER_COMPLETE_PATH } from '@/lib/bridge/worker-auth';
export const config = { api: { bodyParser: false, responseLimit: '4mb' }, maxDuration: 30 };
export function createWorkerCompleteHandler(queueFactory: () => BridgeQueue = configuredBridgeQueue) {
  return async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
    workerResponseHeaders(res);
    if (req.method !== 'POST') { workerFailure(res, new BridgeError('BRIDGE_METHOD_NOT_ALLOWED', 405)); return; }
    let queue: BridgeQueue | undefined;
    try {
      if (Object.keys(req.query).length || req.url !== WORKER_COMPLETE_PATH) throw new BridgeError('BRIDGE_REQUEST_INVALID', 400);
      const raw = await readWorkerBody(req, 4 * 1024 * 1024);
      queue = queueFactory();
      const auth = verifyWorkerAuthentication(queue.configuration, req.headers, 'POST', WORKER_COMPLETE_PATH, raw, queue.now());
      let body: unknown;
      try { body = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(raw)); } catch { throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400); }
      if (!exactObject(body, ['schema_version', 'job_id', 'lease_id', 'response']) || body.schema_version !== 'apocky.bridge.complete.v1' || typeof body.job_id !== 'string' || typeof body.lease_id !== 'string') throw new BridgeError('BRIDGE_PAYLOAD_INVALID', 400);
      await queue.consumeAuthentication(auth);
      await queue.complete(body.job_id, body.lease_id, body.response);
      res.status(200).json({ schema_version: 'apocky.bridge.complete-result.v1', accepted: true });
    } catch (error) { workerFailure(res, error); } finally { queue?.configuration.key.fill(0); }
  };
}
export default createWorkerCompleteHandler();
