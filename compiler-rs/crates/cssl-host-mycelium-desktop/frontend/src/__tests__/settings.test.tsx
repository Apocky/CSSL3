// § settings.test.tsx — Settings-component flow for Anthropic-key save.
// § T11-W17-C : exercise the full UI round-trip via the IPC mock surface.
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import * as ipc from "../lib/ipc";
import type { IpcCommand, IpcResponse } from "../lib/types";
import Settings from "../pages/Settings";

/// Build a minimal valid `AppConfig` for the GetConfig response.
function fakeConfig() {
  return {
    llm: {
      mode: "external_anthropic" as const,
      anthropic_model: "claude-opus-4-7",
      ollama_endpoint: "http://localhost:11434",
      ollama_model: "qwen2.5-coder:32b",
      max_tokens: 4096,
      temperature: 0.7,
      simulate_delay: false,
    },
    caps: "default" as const,
    sandbox_paths: ["/projects"],
    ui_theme: "dark" as const,
    auto_audit: true,
    revert_window_secs: 30,
    knowledge_top_k: 5,
    context_token_budget: 50000,
  };
}

describe("Settings — Anthropic API key flow", () => {
  beforeEach(() => {
    ipc.__setMockInvoke(null);
  });
  afterEach(() => {
    cleanup();
    ipc.__setMockInvoke(null);
  });

  it("renders 'not configured' when host returns null masked-key", async () => {
    ipc.__setMockInvoke(async (_cmd, args) => {
      const command = (args as { command: IpcCommand }).command;
      if (command.type === "get_config") {
        return { type: "config", config: fakeConfig() } satisfies IpcResponse;
      }
      if (command.type === "load_anthropic_key_masked") {
        return { type: "anthropic_key_masked", masked: null } satisfies IpcResponse;
      }
      return { type: "error", message: "unexpected", code: "command" };
    });

    await act(async () => {
      render(<Settings />);
    });

    await waitFor(() => screen.getByTestId("anthropic-key-status"));
    const status = screen.getByTestId("anthropic-key-status");
    expect(status.textContent).toContain("not configured");
  });

  it("renders 'configured (masked)' when host returns sk-...XXXX", async () => {
    ipc.__setMockInvoke(async (_cmd, args) => {
      const command = (args as { command: IpcCommand }).command;
      if (command.type === "get_config") {
        return { type: "config", config: fakeConfig() } satisfies IpcResponse;
      }
      if (command.type === "load_anthropic_key_masked") {
        return {
          type: "anthropic_key_masked",
          masked: "sk-...wxyz",
        } satisfies IpcResponse;
      }
      return { type: "error", message: "unexpected", code: "command" };
    });

    await act(async () => {
      render(<Settings />);
    });
    await waitFor(() => screen.getByTestId("anthropic-key-status"));
    const status = screen.getByTestId("anthropic-key-status");
    expect(status.textContent).toContain("configured");
    expect(status.textContent).toContain("sk-...wxyz");
  });

  it("Save button is disabled when input is empty + enabled after typing", async () => {
    ipc.__setMockInvoke(async (_cmd, args) => {
      const command = (args as { command: IpcCommand }).command;
      if (command.type === "get_config") {
        return { type: "config", config: fakeConfig() };
      }
      if (command.type === "load_anthropic_key_masked") {
        return { type: "anthropic_key_masked", masked: null };
      }
      return { type: "error", message: "unexpected", code: "command" };
    });

    await act(async () => {
      render(<Settings />);
    });
    await waitFor(() => screen.getByTestId("anthropic-key-save"));
    const button = screen.getByTestId("anthropic-key-save") as HTMLButtonElement;
    expect(button.disabled).toBe(true);

    const input = screen.getByTestId("anthropic-key-input") as HTMLInputElement;
    await act(async () => {
      fireEvent.change(input, { target: { value: "sk-ant-test-key-12345" } });
    });
    expect(button.disabled).toBe(false);
  });

  it("save flow forwards key to host + clears input + updates indicator", async () => {
    let lastCommand: IpcCommand | null = null;
    let savedKey: string | null = null;
    ipc.__setMockInvoke(async (_cmd, args) => {
      const command = (args as { command: IpcCommand }).command;
      lastCommand = command;
      if (command.type === "get_config") {
        return { type: "config", config: fakeConfig() };
      }
      if (command.type === "load_anthropic_key_masked") {
        return { type: "anthropic_key_masked", masked: null };
      }
      if (command.type === "save_anthropic_key") {
        savedKey = command.key;
        return { type: "anthropic_key_saved", masked: "sk-...zzzz" };
      }
      return { type: "error", message: "unexpected", code: "command" };
    });

    await act(async () => {
      render(<Settings />);
    });
    await waitFor(() => screen.getByTestId("anthropic-key-input"));
    const input = screen.getByTestId("anthropic-key-input") as HTMLInputElement;
    const button = screen.getByTestId("anthropic-key-save") as HTMLButtonElement;

    await act(async () => {
      fireEvent.change(input, { target: { value: "sk-ant-secret-zzzz" } });
    });
    await act(async () => {
      fireEvent.click(button);
    });

    await waitFor(() => {
      const status = screen.getByTestId("anthropic-key-status");
      expect(status.textContent).toContain("sk-...zzzz");
    });
    expect(savedKey).toBe("sk-ant-secret-zzzz");
    expect((lastCommand as IpcCommand | null)?.type).toBe("save_anthropic_key");
    // § Plaintext key MUST NOT linger in the React input after save.
    expect(input.value).toBe("");
  });
});
