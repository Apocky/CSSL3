// cssl-edge · tests/api/stripe-refund.test.ts
// Drives inline tests in pages/api/payments/stripe/refund-request.ts.

import {
  testRefundMissingFields,
  testRefundStubMode,
} from '@/pages/api/payments/stripe/refund-request';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testRefundMissingFields();
  await testRefundStubMode();
  // eslint-disable-next-line no-console
  console.log('stripe-refund.test : OK · 2 tests passed');
}

if (isMain) {
  runAll().catch((err: unknown) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
