// cssl-edge · tests/api/stripe-checkout.test.ts
// Drives inline tests in pages/api/payments/stripe/checkout.ts.

import {
  testCheckoutMissingProductId,
  testCheckoutStubModeShape,
  testCheckoutCapDenied,
} from '@/pages/api/payments/stripe/checkout';

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testCheckoutMissingProductId();
  await testCheckoutStubModeShape();
  await testCheckoutCapDenied();
  // eslint-disable-next-line no-console
  console.log('stripe-checkout.test : OK · 3 tests passed');
}

if (isMain) {
  runAll().catch((err: unknown) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
