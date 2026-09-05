import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  authorizeOwnerClaims,
  createConfirmationDigest,
  parseOwnerAllowlist,
  validateMutationRequest,
} from "../src/policy";
import {
  buildContentSecurityPolicy,
  privateNoStoreHeaders,
  securityHeaders,
} from "../src/headers";

describe("owner authorization", () => {
  const allowlist = parseOwnerAllowlist({
    userIds: "11111111-1111-1111-1111-111111111111",
    emails: "owner@example.com",
  });

  it("requires both verified identity gates to identify one owner", () => {
    const owner = authorizeOwnerClaims(
      {
        sub: "cf-owner",
        email: "owner@example.com",
        aud: "audience",
        exp: 9_999_999_999,
        iss: "https://team.cloudflareaccess.com",
      },
      {
        sub: "11111111-1111-1111-1111-111111111111",
        email: "owner@example.com",
      },
      allowlist,
    );
    assert.equal(owner?.cloudflareSubject, "cf-owner");
  });

  it("fails when Cloudflare and Supabase emails diverge", () => {
    assert.equal(
      authorizeOwnerClaims(
        {
          sub: "cf-owner",
          email: "other@example.com",
          aud: "audience",
          exp: 9_999_999_999,
          iss: "https://team.cloudflareaccess.com",
        },
        {
          sub: "11111111-1111-1111-1111-111111111111",
          email: "owner@example.com",
        },
        allowlist,
      ),
      null,
    );
  });

  it("rejects an incomplete owner allowlist", () => {
    assert.throws(
      () => parseOwnerAllowlist({ userIds: "", emails: "" }),
      /required/,
    );
  });
});

describe("consequential mutation confirmation", () => {
  const unsigned = {
    action: "retained-history.delete",
    target: "session:one",
    nonce: "n".repeat(24),
    confirmedAt: "2026-07-25T06:00:00.000Z",
  };
  const confirmation = {
    ...unsigned,
    digest: createConfirmationDigest(unsigned),
  };

  it("requires same-origin request plus matching body and header digest", () => {
    const request = new Request("https://ops.apocky.com/api/history", {
      method: "DELETE",
      headers: {
        origin: "https://ops.apocky.com",
        "sec-fetch-site": "same-origin",
        "x-apocky-confirmation-digest": confirmation.digest,
      },
    });
    assert.equal(
      validateMutationRequest(request, {
        allowedOrigins: new Set(["https://ops.apocky.com"]),
        action: unsigned.action,
        target: unsigned.target,
        confirmation,
        now: new Date("2026-07-25T06:01:00.000Z"),
      }).digest,
      confirmation.digest,
    );
  });

  it("rejects cross-site mutation even with a valid confirmation", () => {
    const request = new Request("https://ops.apocky.com/api/history", {
      method: "DELETE",
      headers: {
        origin: "https://attacker.example",
        "sec-fetch-site": "cross-site",
        "x-apocky-confirmation-digest": confirmation.digest,
      },
    });
    assert.throws(
      () =>
        validateMutationRequest(request, {
          allowedOrigins: new Set(["https://ops.apocky.com"]),
          action: unsigned.action,
          target: unsigned.target,
          confirmation,
          now: new Date("2026-07-25T06:01:00.000Z"),
        }),
      /origin/,
    );
  });
});

describe("browser security headers", () => {
  it("denies framing and disables private caching", () => {
    const headers = securityHeaders({ allowMedia: false });
    assert.equal(headers["X-Frame-Options"], "DENY");
    assert.match(headers["Content-Security-Policy"]!, /frame-ancestors 'none'/);
    assert.match(privateNoStoreHeaders["Cache-Control"]!, /no-store/);
  });

  it("only enables media for an explicitly media-capable surface", () => {
    assert.match(
      buildContentSecurityPolicy({ allowMedia: false }),
      /media-src 'none'/,
    );
    assert.match(
      buildContentSecurityPolicy({
        allowMedia: true,
        liveKitUrl: "wss://media.example.com",
      }),
      /media-src 'self' blob:/,
    );
  });
});
