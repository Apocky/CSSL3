// cssl-edge · tests/api/stripe-webhook.test.ts
// Drives inline tests in pages/api/payments/stripe/webhook.ts.

import {
  testWebhookMethodNotAllowed,
  testWebhookStubMode,
  testWebhookConfigShapeGuard,
  testWebhookIdempotencyStub,
} from '@/pages/api/payments/stripe/webhook';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testWebhookMethodNotAllowed();
  await testWebhookStubMode();
  await testWebhookConfigShapeGuard();
  await testWebhookIdempotencyStub();
  // eslint-disable-next-line no-console
  console.log('stripe-webhook.test : OK · 4 tests passed');
}

if (isMain) {
  runAll().catch((err: unknown) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
