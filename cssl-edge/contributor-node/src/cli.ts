import { randomUUID } from 'node:crypto';

import {
  ContributorClientError,
  DEFAULT_CONTROLLER_PUBLIC_KEY_PEM,
  ContributorNetworkClient,
  loadOrCreateIdentity,
} from './client.js';

import {
  CONTRIBUTOR_NETWORK_ENABLED,
  DEFAULT_CONTRIBUTOR_POLICY,
  ContributorNodeError,
  ContributorWorker,
} from './runtime.js';

const HELP = `Apocrypha contributor node (opt-in)

Reads one signed lease JSON object from stdin and writes one signed result JSON
object to stdout, or use --network to enroll, poll, execute, and submit one
approved public-capsule task over the authenticated Apocrypha transport.

Network uses the pinned production controller key bundled in the package.
Override only when independently verifying a different controller:
  APOCRYPHA_CONTROLLER_PUBLIC_KEY_PEM  controller Ed25519 public key

Optional environment:
  APOCRYPHA_CONTROLLER_KEY_ID          pinned controller key id (default: apocrypha-controller-v1)
  APOCRYPHA_NODE_ID                    local node id (default: generated per process)

Commands:
  --help       show this help
  --status     print local status without accepting work
  --uninstall  clear local in-memory state and exit
  --opt-in     accept one signed lease, process it, and exit

Network command:
  --network    use the live Apocrypha transport (requires --opt-in)
  --loop       repeat bounded tasks until interrupted (requires --network)
  --base-url   controller base URL (default: https://www.apocky.com)
  --data-dir   identity directory (default: platform state directory)

Network environment:
  APOCRYPHA_CONTROLLER_PUBLIC_KEY_PEM  pinned controller Ed25519 public key
  APOCRYPHA_CONTROLLER_KEY_ID          pinned controller key id
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
  return error instanceof ContributorNodeError || error instanceof ContributorClientError ? error.code : 'TASK_FAILED';
}

function option(args: string[], name: string): string | undefined {
  const index = args.indexOf(name);
  if (index < 0) return undefined;
  const value = args[index + 1];
  return value && !value.startsWith('--') ? value : undefined;
}

function platformTarget(): 'windows-x64' | 'macos-arm64' | 'linux-x64' | 'android' | 'ios' {
  if (process.platform === 'win32') return 'windows-x64';
  if (process.platform === 'darwin') return 'macos-arm64';
  return 'linux-x64';
}

async function runNetwork(args: string[]): Promise<void> {
  if (!args.includes('--opt-in')) throw new ContributorClientError('WORKER_NOT_OPTED_IN');
  const controllerPublicKey = process.env.APOCRYPHA_CONTROLLER_PUBLIC_KEY_PEM ?? DEFAULT_CONTROLLER_PUBLIC_KEY_PEM;
  const controllerKeyId = process.env.APOCRYPHA_CONTROLLER_KEY_ID ?? 'apocrypha-controller-v1';
  const baseUrl = option(args, '--base-url') ?? process.env.APOCRYPHA_NODE_BASE_URL ?? 'https://www.apocky.com';
  const dataDir = option(args, '--data-dir') ?? process.env.APOCRYPHA_NODE_DATA_DIR;
  const identity = await loadOrCreateIdentity(dataDir);
  const client = new ContributorNetworkClient({
    baseUrl,
    controllerKeyId,
    controllerPublicKey,
    identity,
    platform: platformTarget(),
    dataDir: dataDir ?? '',
  });
  const task = { kind: 'vector_dot' as const, left: [1, 2, 3], right: [4, 5, 6] };
  const first = await client.runOnce(task);
  process.stdout.write(`${JSON.stringify(first)}\n`);
  if (!args.includes('--loop')) return;
  // Loop remains explicit and bounded: every iteration re-enrolls and gets a
  // fresh signed lease; no background service or hidden auto-start is created.
  while (true) {
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    const next = await client.runOnce(task);
    process.stdout.write(`${JSON.stringify(next)}\n`);
  }
}

async function main(): Promise<void> {
  const args = new Set(process.argv.slice(2));
  const rawArgs = process.argv.slice(2);
  if (args.has('--help')) {
    process.stdout.write(HELP);
    return;
  }
  if (args.has('--network')) {
    await runNetwork(rawArgs);
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
