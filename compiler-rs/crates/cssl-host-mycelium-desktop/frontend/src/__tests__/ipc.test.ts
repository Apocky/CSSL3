// § ipc.test.ts — IPC type-safety mocks + send wrapper round-trip.
import { describe, expect, it, beforeEach } from "vitest";
import * as ipc from "../lib/ipc";
import type { IpcCommand, IpcResponse } from "../lib/types";

describe("ipc.send", () => {
  beforeEach(() => {
    ipc.__setMockInvoke(null);
  });

  it("sends a start_session command and returns the typed response", async () => {
    ipc.__setMockInvoke(async (_cmd, _args) => {
      return { type: "session_started", session_id: "session-42" } satisfies IpcResponse;
    });
    const resp = await ipc.send({ type: "start_session" });
    expect(resp.type).toBe("session_started");
    if (resp.type === "session_started") {
      expect(resp.session_id).toBe("session-42");
    }
  });

  it("forwards command JSON shape verbatim to invoke", async () => {
    let captured: { cmd: string; args?: Record<string, unknown> } = { cmd: "" };
    ipc.__setMockInvoke(async (cmd, args) => {
      captured = { cmd, args };
      return { type: "cancelled" };
    });
    const command: IpcCommand = { type: "send_message", content: "hello" };
    await ipc.send(command);
    expect(captured.cmd).toBe("ipc_dispatch");
    expect(captured.args).toEqual({ command });
  });

  it("propagates error responses (does not throw)", async () => {
    ipc.__setMockInvoke(async () => {
      return { type: "error", message: "boom", code: "session" } satisfies IpcResponse;
    });
    const resp = await ipc.send({ type: "cancel" });
    expect(ipc.isError(resp)).toBe(true);
    if (resp.type === "error") {
      expect(resp.code).toBe("session");
    }
  });

  it("expectMessageReply throws on non-message_reply response", async () => {
    ipc.__setMockInvoke(async () => ({ type: "cancelled" }));
    const resp = await ipc.send({ type: "send_message", content: "x" });
    expect(() => ipc.expectMessageReply(resp)).toThrow();
  });

  it("convenience wrappers compose to the right command shapes", async () => {
    let lastCommand: IpcCommand | null = null;
    ipc.__setMockInvoke(async (_cmd, args) => {
      lastCommand = (args as { command: IpcCommand }).command;
      return { type: "all_sovereign_revoked" };
    });
    await ipc.revokeAllSovereignCaps();
    expect(lastCommand).toEqual({ type: "revoke_all_sovereign_caps" });
    await ipc.querySubstrate("hello", 7);
    expect(lastCommand).toEqual({ type: "query_substrate", query: "hello", top_k: 7 });
  });
});
