import assert from "node:assert/strict";
import test from "node:test";

import {
  isOpsReadSurface,
  OPS_READ_SURFACES,
  OPS_ROUTES,
} from "../lib/routes";

test("ops exposes only evidence surfaces and one typed action route", () => {
  assert.deepEqual(OPS_READ_SURFACES, [
    "runtime",
    "sessions",
    "authority",
    "consent",
    "security",
    "deployment",
    "retention",
  ]);
  assert.equal(OPS_ROUTES.action.method, "POST");
  assert.equal(isOpsReadSurface("deployment"), true);
  assert.equal(isOpsReadSurface("deploy-now"), false);
});
