import assert from "node:assert/strict";
import test from "node:test";

import {
  privateJson,
  readBoundedJson,
  runPrivateRoute,
} from "../lib/private-route";

test("protected ops work never runs after authorization failure", async () => {
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
        Response.json({ ok: false }, { status: 403 }),
    },
  );
  assert.equal(ran, false);
  assert.equal(response.status, 403);
});

test("ops responses disable media and caching", () => {
  const response = privateJson({ ok: true });
  assert.match(response.headers.get("cache-control") ?? "", /no-store/);
  assert.match(
    response.headers.get("permissions-policy") ?? "",
    /camera=\(\).*microphone=\(\)/,
  );
  assert.equal(response.headers.get("x-frame-options"), "DENY");
});

test("oversized ops commands fail before JSON parsing", async () => {
  const request = new Request("https://ops.example/api/actions", {
    method: "POST",
    headers: { "content-length": "999999" },
    body: "{}",
  });
  await assert.rejects(() => readBoundedJson(request, 64), /exceeds/);
});
