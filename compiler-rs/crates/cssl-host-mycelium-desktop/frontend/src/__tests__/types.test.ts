// § types.test.ts — type-discriminator tag tests.
//   Tag-based discrimination is critical for narrowing IpcResponse / IpcCommand.
import { describe, expect, it } from "vitest";
import type { IpcCommand, IpcResponse, AppConfig } from "../lib/types";
import { TOOL_NAMES } from "../lib/types";

describe("IpcCommand tag discrimination", () => {
  it("every IpcCommand variant has a 'type' tag in snake_case", () => {
    const cases: IpcCommand[] = [
      { type: "start_session" },
      { type: "send_message", content: "hi" },
      { type: "cancel" },
      { type: "get_history", limit: 10 },
      { type: "grant_cap", tool: "file_read", mode: "auto" },
      { type: "revoke_cap", tool: "file_read" },
      { type: "revoke_all_sovereign_caps" },
      { type: "open_settings" },
      { type: "query_substrate", query: "x", top_k: 3 },
      { type: "get_config" },
      { type: "get_substrate_doc_count" },
    ];
    for (const c of cases) {
      expect(typeof c.type).toBe("string");
      expect(c.type).toMatch(/^[a-z_]+$/);
    }
  });
});

describe("IpcResponse tag discrimination", () => {
  it("error variant carries machine-readable code", () => {
    const r: IpcResponse = { type: "error", message: "x", code: "loop" };
    expect(r.type).toBe("error");
    if (r.type === "error") {
      expect(r.code).toBe("loop");
    }
  });
});

describe("TOOL_NAMES list", () => {
  it("contains the 12 canonical tool names", () => {
    expect(TOOL_NAMES.length).toBe(12);
    expect(TOOL_NAMES).toContain("file_read");
    expect(TOOL_NAMES).toContain("spec_query");
    // Verify AppConfig caps field exists in the type (compile-time check via cast).
    const cfg = { caps: "default" } as unknown as Pick<AppConfig, "caps">;
    expect(cfg.caps).toBe("default");
  });
});
