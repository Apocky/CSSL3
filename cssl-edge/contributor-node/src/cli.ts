import { randomUUID } from 'node:crypto';

import {
  CONTRIBUTOR_NETWORK_ENABLED,
  DEFAULT_CONTRIBUTOR_POLICY,
  ContributorNodeError,
  ContributorWorker,
} from './runtime.js';

const HELP = `Apocrypha contributor node (local candidate)

Reads one signed lease JSON object from stdin and writes one signed result JSON
object to stdout. Network is disabled. The process starts paused and requires
the local operator to opt in through this explicit flag.

Required environment:
  APOCRYPHA_CONTROLLER_PUBLIC_KEY_PEM  controller Ed25519 public key

Optional environment:
  APOCRYPHA_CONTROLLER_KEY_ID          pinned controller key id (default: apocrypha-controller-v1)
  APOCRYPHA_NODE_ID                    local node id (default: generated per process)

Commands:
  --help       show this help
  --status     print local status without accepting work
  --uninstall  clear local in-memory state and exit
  --opt-in     accept one signed lease, process it, and exit
`;

async function readLeaseFromStdin(maxBytes: number): Promise<unknown> {
  const chunks: string[] = [];
  let totalBytes = 0;
  for await (const chunk of process.stdin) {
    const text = typeof chunk === 'string' ? chunk : chunk.toString('utf8');
    totalBytes += Buffer.byteLength(text, 'utf8');
    if (totalBytes > maxBytes) {
      throw new ContributorNodeError('LEASE_INPUT_LIMIT');
    }
    chunks.push(text);
  }
  try {
    return JSON.parse(chunks.join(''));
  } catch {
    throw new ContributorNodeError('LEASE_SCHEMA_INVALID');
  }
}

function errorCode(error: unknown): string {
  return error instanceof ContributorNodeError ? error.code : 'TASK_FAILED';
}

async function main(): Promise<void> {
  const args = new Set(process.argv.slice(2));
  if (args.has('--help')) {
    process.stdout.write(HELP);
    return;
  }
  const controllerPublicKey = process.env.APOCRYPHA_CONTROLLER_PUBLIC_KEY_PEM;
  if (!controllerPublicKey) throw new ContributorNodeError('CONTROLLER_KEY_REQUIRED');
  const controllerKeyId = process.env.APOCRYPHA_CONTROLLER_KEY_ID ?? 'apocrypha-controller-v1';
  const nodeId = process.env.APOCRYPHA_NODE_ID ?? `node-${randomUUID()}`;
  const worker = new ContributorWorker({
    nodeId,
    controllerKeyId,
    controllerPublicKey,
  });
  if (args.has('--status')) {
    process.stdout.write(`${JSON.stringify(worker.status())}\n`);
    return;
  }
  if (args.has('--uninstall')) {
    process.stdout.write(`${JSON.stringify(worker.uninstall())}\n`);
    return;
  }
  if (CONTRIBUTOR_NETWORK_ENABLED) throw new ContributorNodeError('TASK_FAILED');
  if (!args.has('--opt-in')) throw new ContributorNodeError('WORKER_NOT_OPTED_IN');
  worker.optIn();
  const lease = await readLeaseFromStdin(DEFAULT_CONTRIBUTOR_POLICY.maxLeaseBytes);
  const result = await worker.run(lease);
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

main().catch((error: unknown) => {
  process.stderr.write(`${JSON.stringify({ ok: false, code: errorCode(error) })}\n`);
  process.exitCode = 1;
});
