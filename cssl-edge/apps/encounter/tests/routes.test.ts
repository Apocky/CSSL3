import assert from "node:assert/strict";
import test from "node:test";

import { ENCOUNTER_ROUTES, encounterPath } from "../lib/routes";

test("the private encounter route contract covers every required operation", () => {
  assert.deepEqual(Object.keys(ENCOUNTER_ROUTES).sort(), [
    "create",
    "current",
    "end",
    "grantConsent",
    "historyDelete",
    "historyRead",
    "joinToken",
    "readiness",
    "revokeConsent",
    "understandingAcknowledge",
    "understandingCorrect",
    "understandingSubmit",
  ]);
});

test("session identifiers are encoded into dynamic route paths", () => {
  assert.equal(
    encounterPath(ENCOUNTER_ROUTES.readiness.path, "a/b"),
    "/api/encounters/a%2Fb/readiness",
  );
});
