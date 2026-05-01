// cssl-edge · lib/sse.ts
// Server-Sent-Events writer wrapper. Encapsulates the SSE framing rules so
// per-route handlers don't have to reimplement the wire format.
//
// SSE wire format :
//   - one event = optional `event: <name>\n` + `data: <payload>\n` + blank line
//   - terminator sentinel : `data: [DONE]\n\n` (Anthropic + OpenAI convention)
//   - payloads are JSON-serialized · newlines are illegal mid-payload
//
// The writer is intentionally minimal — it accepts a Node-style `res.write` /
// `res.end` pair (NextApiResponse satisfies this) AND returns plain strings
// from the test helpers so unit-tests don't need a real socket.

interface WritableLike {
  write(chunk: string): boolean;
  end(): void;
}

export class SseWriter {
  private res: WritableLike;
  private closed = false;

  constructor(res: WritableLike) {
    this.res = res;
  }

  // Format-only helper · returns the wire-bytes for a `data:` event.
  static formatData(data: unknown): string {
    return `data: ${JSON.stringify(data)}\n\n`;
  }

  // Format-only helper · returns the wire-bytes for a named event.
  static formatEvent(name: string, data: unknown): string {
    return `event: ${name}\ndata: ${JSON.stringify(data)}\n\n`;
  }

  // Wire-bytes terminator. Clients of Anthropic + OpenAI streaming detect
  // `[DONE]` literal sentinel (NOT JSON-quoted) — match that convention.
  static formatDone(): string {
    return `data: [DONE]\n\n`;
  }

  writeData(data: unknown): void {
    if (this.closed) return;
    this.res.write(SseWriter.formatData(data));
  }

  writeEvent(name: string, data: unknown): void {
    if (this.closed) return;
    this.res.write(SseWriter.formatEvent(name, data));
  }

  writeDone(): void {
    if (this.closed) return;
    this.res.write(SseWriter.formatDone());
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.res.end();
  }
}

// ─── inline tests · framework-agnostic ─────────────────────────────────────
// Run via `npx tsx lib/sse.ts`.

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. writeData emits `data: <json>\n\n`.
export function testWriteDataNewlineFormat(): void {
  const wire = SseWriter.formatData({ hello: 'world' });
  assert(
    wire === 'data: {"hello":"world"}\n\n',
    `expected data:json + double-newline, got ${JSON.stringify(wire)}`
  );
}

// 2. writeEvent emits `event: <name>\ndata: <json>\n\n`.
export function testWriteEventTypePrefix(): void {
  const wire = SseWriter.formatEvent('chunk', { idx: 0, text: 'a' });
  assert(
    wire === 'event: chunk\ndata: {"idx":0,"text":"a"}\n\n',
    `expected event:name + data:json + newlines, got ${JSON.stringify(wire)}`
  );
}

// 3. close() flushes via res.end(); subsequent writes are no-ops.
export function testCloseFlushes(): void {
  const seen: string[] = [];
  const flag: { ended: boolean } = { ended: false };
  const fakeRes: WritableLike = {
    write(c: string) {
      seen.push(c);
      return true;
    },
    end() {
      flag.ended = true;
    },
  };
  const w = new SseWriter(fakeRes);
  w.writeData({ a: 1 });
  w.close();
  // After close, further writes must be no-ops (no extra entries in `seen`).
  w.writeData({ b: 2 });
  w.writeDone();
  assert(seen.length === 1, `expected exactly 1 write before close, got ${seen.length}`);
  assert(flag.ended === true, 'expected close() → res.end() called');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testWriteDataNewlineFormat();
  testWriteEventTypePrefix();
  testCloseFlushes();
  // eslint-disable-next-line no-console
  console.log('sse.ts : OK · 3 inline tests passed');
}
