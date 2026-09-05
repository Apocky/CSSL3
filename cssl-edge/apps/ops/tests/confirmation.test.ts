import assert from "node:assert/strict";
import test from "node:test";

import {
  buildConfirmationPhrase,
  OpsCommandSchema,
} from "../lib/confirmation";

const target = "e5685c0d-b4f0-43f2-8992-90626547d9ef";
const expectedDigest = `sha256:${"c".repeat(64)}`;

test("confirmation binds an exact action, target, and observed digest", () => {
  const action = "delete_retained_history" as const;
  const confirmation = buildConfirmationPhrase(
    action,
    target,
    expectedDigest,
  );
  assert.equal(
    confirmation,
    `CONFIRM DELETE RETAINED HISTORY ${target} ${expectedDigest}`,
  );
  assert.equal(
    OpsCommandSchema.parse({
      action,
      target,
      expectedDigest,
      phrase: confirmation,
      confirmation: {},
    }).target,
    target,
  );
});

test("a stale digest or approximate phrase fails closed", () => {
  const action = "end_encounter" as const;
  const confirmation = buildConfirmationPhrase(
    action,
    target,
    expectedDigest,
  );
  assert.throws(() =>
    OpsCommandSchema.parse({
      action,
      target,
      expectedDigest: `sha256:${"d".repeat(64)}`,
      phrase: confirmation,
      confirmation: {},
    }),
  );
  assert.throws(() =>
    OpsCommandSchema.parse({
      action,
      target,
      expectedDigest,
      phrase: confirmation.toLowerCase(),
      confirmation: {},
    }),
  );
});
