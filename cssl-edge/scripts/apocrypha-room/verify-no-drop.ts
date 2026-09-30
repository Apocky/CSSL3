// Dropped-post verifier for the room loop (R02-1b gate). READ ONLY.
// Site GETs (pull/tail) + one local cursor read. Never posts, never writes.
//
//   node --env-file=C:\Apocrypha\apocrypha-runtime.env --import tsx \
//     scripts/apocrypha-room/verify-no-drop.ts [--cursor <path>] [--tail 20]
//
// Verdict JSON on stdout; exit 0 PASS, 1 FAIL. A FAIL names the gap so the
// cutover can be rolled back to the previous commit + split restart.
// Vacuity proof: --cursor <stale-file> must FAIL (proves the check bites).

const SITE = (process.env.APOCRYPHA_CONTROL_PLANE_URL ?? '').replace(/\/+$/, '');
const TOKEN = process.env.APOCRYPHA_WORKER_TOKEN ?? '';
const NODE_ID = process.env.APOCRYPHA_WORKER_NODE_ID ?? '';
const CURSOR_DEFAULT = process.env.APOCRYPHA_ROOM_CURSOR_PATH
  ?? 'C:\\Apocrypha\\rooms\\site-room.cursor.json';

const args = process.argv.slice(2);
function flag(name: string): string | undefined {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}
const cursorPath = flag('--cursor') ?? CURSOR_DEFAULT;
const tailN = Number(flag('--tail') ?? 20);

interface RoomEvent {
  id: number; room: string; author: string; kind: string; body: string; created_at: string;
}

async function siteGet(path: string): Promise<unknown> {
  const res = await fetch(`${SITE}${path}`, {
    headers: {
      authorization: `Bearer ${TOKEN}`,
      'x-apocrypha-node-id': NODE_ID,
      'user-agent': 'apocrypha-verify-no-drop/1.0',
    },
    signal: AbortSignal.timeout(15_000),
  });
  const text = await res.text();
  const payload = JSON.parse(text) as Record<string, unknown>;
  if (!res.ok || payload.ok !== true) throw new Error(`site ${path} -> HTTP ${res.status}`);
  return payload;
}

async function tail(room: string): Promise<RoomEvent[]> {
  const payload = await siteGet(`/api/room/worker/pull?room=${room}&tail=${tailN}`) as { events?: RoomEvent[] };
  return Array.isArray(payload.events) ? payload.events : [];
}

async function pullAfter(after: number): Promise<RoomEvent[]> {
  const payload = await siteGet(`/api/room/worker/pull?after=${after}&limit=50`) as { events?: RoomEvent[] };
  return Array.isArray(payload.events) ? payload.events : [];
}

async function main(): Promise<void> {
  if (!SITE || !TOKEN || !NODE_ID) throw new Error('SITE/TOKEN/NODE_ID required');
  const { existsSync, readFileSync } = await import('node:fs');
  let cursorAfter = 0;
  try {
    if (existsSync(cursorPath)) {
      cursorAfter = Number((JSON.parse(readFileSync(cursorPath, 'utf8')) as { after?: number }).after ?? 0);
    }
  } catch { cursorAfter = -1; }
  const failures: string[] = [];
  const info: Record<string, unknown> = { cursorAfter, cursorPath };

  // 1. Cursor gap: every pulled row at/above the cursor must be processed.
  const fresh = await pullAfter(cursorAfter);
  const unprocessed = fresh.filter((r) => r.id > cursorAfter && r.author !== 'apocrypha');
  info.unprocessedUserRows = unprocessed.map((r) => r.id);
  if (unprocessed.length > 0) failures.push(`cursor gap: ${unprocessed.length} user rows past cursor ${cursorAfter}`);

  // 2. Pairing: every apocrypha utterance in the window needs a thought row
  // before it with matching meta (the loop posts thought BEFORE utterance;
  // presence spam between them makes row-distance rules brittle, so match
  // on meta instead: same reply_to, or both unprompted).
  const metaKey = (r: RoomEvent) => {
    const meta = (r as unknown as { meta?: Record<string, unknown> }).meta ?? {};
    return `reply_to=${String(meta.reply_to ?? '')};unprompted=${String(meta.unprompted ?? '')}`;
  };
  let utterances = 0;
  let unpaired = 0;
  for (const room of ['lobby', 'owner']) {
    const rows = await tail(room);
    info[`${room}TailMax`] = rows.length ? (rows[rows.length - 1] as RoomEvent).id : null;
    rows.forEach((row, i) => {
      if (row.author !== 'apocrypha' || row.kind !== 'utterance') return;
      utterances += 1;
      const window = rows.slice(Math.max(0, i - 25), i);
      const match = window.some((r) => r.author === 'apocrypha' && r.kind === 'thought' && metaKey(r) === metaKey(row));
      if (!match) unpaired += 1;
    });
  }
  info.utterances = utterances;
  info.unpaired = unpaired;
  if (unpaired > 0) failures.push(`pairing: ${unpaired}/${utterances} utterances without a preceding thought row`);

  // 3. Presence freshness: a live loop says idle/thinking every ~20 s.
  let latestPresence = 0;
  for (const room of ['lobby', 'owner']) {
    for (const row of await tail(room)) {
      if (row.author === 'apocrypha' && row.kind === 'presence') {
        latestPresence = Math.max(latestPresence, Date.parse(row.created_at));
      }
    }
  }
  const presenceAgeS = latestPresence ? Math.round((Date.now() - latestPresence) / 1000) : -1;
  info.presenceAgeS = presenceAgeS;
  if (presenceAgeS < 0 || presenceAgeS > 600) failures.push(`presence stale: ${presenceAgeS}s since last presence row`);

  const verdict = { pass: failures.length === 0, failures, ...info };
  console.log(JSON.stringify(verdict));
  process.exit(verdict.pass ? 0 : 1);
}

main().catch((error) => {
  console.log(JSON.stringify({ pass: false, failures: [`verifier crashed: ${(error as Error).message}`] }));
  process.exit(1);
});
