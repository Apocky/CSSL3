import { publicErrorCode } from '@/lib/akashic-telemetry/error-boundary';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(message);
}

const sensitiveMessage = 'request failed for user@example.com with token secret-value';
const error = new Error(sensitiveMessage);
error.stack = `Error: ${sensitiveMessage}\n    at render (https://www.apocky.com/_next/chunk.js:42:9)`;

const first = publicErrorCode(error, 'global');
const second = publicErrorCode(error, 'global');

assert(/^APX-RENDER-[A-F0-9]{16}$/.test(first), 'public code uses the stable APX render-code shape');
assert(first === second, 'the same normalized failure produces the same public code');
assert(!first.includes('example.com'), 'the public code excludes message PII');
assert(!first.includes('secret-value'), 'the public code excludes secret-like input');

// eslint-disable-next-line no-console
console.log('public-error-code.test : OK');
