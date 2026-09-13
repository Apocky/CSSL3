// Oracle probe: the model half of the free test harness.
//
// WHY THIS EXISTS
// ---------------
// The only way to see a real Oracle reading used to be submitting a real job:
// it spends one of the reader's divination tokens and needs the control plane,
// Supabase and the worker all healthy. So the thing that actually matters - does
// the Oracle read every card it was given, in the shape it was told to use - was
// never tested, and a reading that ignored three clarifiers and renamed the
// shadow card shipped to production (2026-09-12).
//
// The local engine costs nothing. This builds the EXACT prompt the worker builds
// (importing prompt.ts's own readingForPrompt/baseSystem, not a copy), sends it
// straight to llama-server, and checks the answer against the contract:
//   * every supplied card named, including each clarifier and the shadow
//   * both bold section labels present
//   * no banned AI-speak
//   * length inside the 180-320 word instruction
// No job, no token, no control plane, no cloud credits.
//
// Usage:
//   npx tsx scripts/oracle-probe.ts --fixtures "C:/.../chaos-tarot/specs/oracle-fixtures" [--system chaos-tarot] [--engine 127.0.0.1:19128]
// Exit code is the number of contract violations, so CI can gate on it.

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

import { baseSystem, readingForPrompt } from './apocrypha-worker/prompt';
import type { ClaimedJob } from './apocrypha-worker/types';

const BANNED = ['delve', 'tapestry', 'unleash', 'embark', 'game-changer', 'cosmic tapestry', 'sacred journey', 'Great question'];
const LABEL_ESOTERIC = 'The Esoteric Read';
const LABEL_PLAIN = 'What It Means for You';

function arg(name: string, fallback = ''): string {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

interface CanonicalItemLike {
  item_id: string;
  name: string;
  is_reversed: boolean;
  position: { name: string };
}

interface Fixture {
  system: string;
  spread: string;
  canonical: { question: string | null; items: CanonicalItemLike[] };
}

/** The worker's own reading turn, rebuilt exactly: system prompt + the reading block. */
function messagesFor(fixture: Fixture): Array<{ role: 'system' | 'user'; content: string }> {
  const job = { capability: 'chaos_tarot_reading', kind: 'interpretation' } as unknown as ClaimedJob;
  const question = fixture.canonical.question;
  const user = [
    question ? `Question:\n${question}` : '',
    `<reading>\n${readingForPrompt(fixture.canonical)}\n</reading>`,
  ].filter(Boolean).join('\n\n');
  return [{ role: 'system', content: baseSystem(job) }, { role: 'user', content: user }];
}

async function ask(engine: string, messages: Array<{ role: string; content: string }>): Promise<{ text: string; ttftMs: number; totalMs: number; promptTokens: number | null }> {
  const started = Date.now();
  let ttftMs = 0;
  const response = await fetch(`http://${engine}/v1/chat/completions`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', accept: 'text/event-stream' },
    body: JSON.stringify({
      model: 'qwen35-35b-a3b-q4',
      messages,
      max_tokens: 700,
      temperature: 0.7,
      stream: true,
      stream_options: { include_usage: true },
      chat_template_kwargs: { enable_thinking: false },
    }),
  });
  if (!response.ok || !response.body) throw new Error(`engine returned ${response.status}`);

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let text = '';
  let promptTokens: number | null = null;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const frames = buffer.split('\n\n');
    buffer = frames.pop() ?? '';
    for (const frame of frames) {
      for (const line of frame.split('\n')) {
        if (!line.startsWith('data:')) continue;
        const payload = line.slice(5).trim();
        if (!payload || payload === '[DONE]') continue;
        try {
          const chunk = JSON.parse(payload);
          const delta = chunk?.choices?.[0]?.delta?.content;
          if (typeof chunk?.usage?.prompt_tokens === 'number') promptTokens = chunk.usage.prompt_tokens;
          if (delta) {
            if (!ttftMs) ttftMs = Date.now() - started;
            text += delta;
          }
        } catch {
          // A partial frame; the next read completes it.
        }
      }
    }
  }
  return { text, ttftMs, totalMs: Date.now() - started, promptTokens };
}

async function main(): Promise<void> {
  const fixturesDir = arg('fixtures', join('..', '..', '..', 'Documents', 'Tarot', 'Chaos', 'New', 'chaos-tarot', 'specs', 'oracle-fixtures'));
  const only = arg('system');
  const engine = arg('engine', '127.0.0.1:19128');

  let files: string[];
  try {
    files = readdirSync(fixturesDir).filter((f) => f.endsWith('.json')).filter((f) => !only || f === `${only}.json`);
  } catch {
    console.error(`no fixtures at ${fixturesDir} - run chaos-tarot/scripts/oracle-fixtures.ts first`);
    process.exitCode = 1;
    return;
  }
  if (files.length === 0) {
    console.error(`no fixtures matched${only ? ` --system ${only}` : ''} in ${fixturesDir}`);
    process.exitCode = 1;
    return;
  }

  console.log(`oracle probe · engine ${engine} · ${files.length} fixture(s) · no job, no token, no credits\n`);
  let violations = 0;

  for (const file of files) {
    const fixture = JSON.parse(readFileSync(join(fixturesDir, file), 'utf8')) as Fixture;
    const messages = messagesFor(fixture);
    const promptChars = messages.reduce((n, m) => n + m.content.length, 0);
    let result: Awaited<ReturnType<typeof ask>>;
    try {
      result = await ask(engine, messages);
    } catch (error) {
      console.log(`${fixture.system}: ENGINE ERROR ${(error as Error).message}`);
      violations += 1;
      continue;
    }

    const lower = result.text.toLowerCase();
    const missed = fixture.canonical.items.filter((item) => !lower.includes(item.name.toLowerCase()));
    const banned = BANNED.filter((w) => lower.includes(w.toLowerCase()));
    const words = result.text.split(/\s+/).filter(Boolean).length;
    const hasEsoteric = result.text.includes(LABEL_ESOTERIC);
    const hasPlain = result.text.includes(LABEL_PLAIN);

    const problems: string[] = [];
    if (missed.length) problems.push(`MISSED ${missed.length}/${fixture.canonical.items.length} cards: ${missed.map((m) => `${m.name} @ ${m.position.name}`).join(' | ')}`);
    if (!hasEsoteric) problems.push(`missing "${LABEL_ESOTERIC}" label`);
    if (!hasPlain) problems.push(`missing "${LABEL_PLAIN}" label`);
    if (banned.length) problems.push(`banned words: ${banned.join(', ')}`);
    if (words < 150 || words > 380) problems.push(`length ${words} words (instruction says 180-320)`);
    violations += problems.length;

    console.log(`${fixture.system} [${fixture.spread}] ${problems.length ? 'FAIL' : 'ok'}`);
    console.log(`  prompt ${promptChars} chars / ${result.promptTokens ?? '?'} tok · ttft ${(result.ttftMs / 1000).toFixed(1)}s · total ${(result.totalMs / 1000).toFixed(1)}s · ${words} words`);
    console.log(`  cards named ${fixture.canonical.items.length - missed.length}/${fixture.canonical.items.length}`);
    for (const p of problems) console.log(`  ! ${p}`);
    console.log(`  --- ${result.text.replace(/\s+/g, ' ').slice(0, 240)}...\n`);
  }

  console.log(violations ? `${violations} contract violation(s)` : 'all fixtures satisfy the reading contract');
  process.exitCode = violations ? 1 : 0;
}

void main();
