import {
  buildAdminChatSystemPrompt,
  callAdminChatModel,
  resolveAdminChatProvider,
  sanitizeAdminChatMessages,
} from '@/lib/admin-chat';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

export function testProviderResolutionDoesNotExposeKeys(): void {
  const status = resolveAdminChatProvider({ DEEPSEEK_API_KEY: 'secret-value' });
  assert(status.ready, 'deepseek key must make provider ready');
  assert(status.provider === 'deepseek', `expected deepseek, got ${status.provider}`);
  assert(status.model === 'deepseek-reasoner', `expected default deepseek model, got ${String(status.model)}`);
  assert(!('secret-value' in status), 'status must not expose key material');
}

export function testSanitizeMessages(): void {
  const messages = sanitizeAdminChatMessages([
    { role: 'user', content: 'hello' },
    { role: 'assistant', content: 'answer' },
    { role: 'bad', content: 'drop' },
    { role: 'system', content: '  keep  ' },
  ]);
  assert(messages.length === 3, `expected 3 clean messages, got ${messages.length}`);
  assert(messages[2]?.content === 'keep', 'system content should trim');
}

export function testSystemPromptDocumentsUnifiedChat(): void {
  const prompt = buildAdminChatSystemPrompt();
  assert(prompt.includes('one unified chat surface'), 'system prompt must describe one unified chat surface');
}

export async function testDeepSeekCallUsesInjectedFetch(): Promise<void> {
  const fakeFetch = (async () => new Response(JSON.stringify({
    choices: [{ message: { content: 'live helper response' } }],
    usage: { total_tokens: 7 },
  }), { status: 200, headers: { 'Content-Type': 'application/json' } })) as typeof fetch;
  const result = await callAdminChatModel({
    messages: [{ role: 'user', content: 'plan the next scene' }],
    env: { DEEPSEEK_API_KEY: 'secret-value' },
    fetchImpl: fakeFetch,
  });
  assert(result.provider === 'deepseek', `expected deepseek provider, got ${result.provider}`);
  assert(result.text === 'live helper response', `unexpected response: ${result.text}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  void (async () => {
    try {
    testProviderResolutionDoesNotExposeKeys();
    testSanitizeMessages();
    testSystemPromptDocumentsUnifiedChat();
    await testDeepSeekCallUsesInjectedFetch();
    // eslint-disable-next-line no-console
    console.log('admin-chat.test : OK · 4 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
  })();
}