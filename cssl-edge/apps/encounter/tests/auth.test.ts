import assert from "node:assert/strict";
import test from "node:test";

import {
  privateJson,
  readBoundedJson,
  runPrivateRoute,
} from "../lib/private-route";

test("private encounter work never runs after authorization failure", async () => {
  let ran = false;
  const response = await runPrivateRoute(
    () => {
      ran = true;
      return privateJson({ ok: true });
    },
    {
      authorize: async () => {
        throw new Error("denied");
      },
      accessError: () =>
        Response.json({ ok: false }, { status: 401 }),
    },
  );
  assert.equal(ran, false);
  assert.equal(response.status, 401);
});

test("encounter responses are private and grant only local media permission", () => {
  const response = privateJson({ ok: true }, { allowMedia: true });
  assert.match(response.headers.get("cache-control") ?? "", /no-store/);
  assert.match(
    response.headers.get("permissions-policy") ?? "",
    /camera=\(self\).*microphone=\(self\)/,
  );
  assert.equal(response.headers.get("x-frame-options"), "DENY");
});

test("oversized private commands fail before JSON parsing", async () => {
  const request = new Request("https://encounter.example/api", {
    method: "POST",
    headers: { "content-length": "999999" },
    body: "{}",
  });
  await assert.rejects(() => readBoundedJson(request, 64), /exceeds/);
});
