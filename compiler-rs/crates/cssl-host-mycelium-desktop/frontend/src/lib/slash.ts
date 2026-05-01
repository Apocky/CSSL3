// § slash.ts — pure-function slash-command parsing for the chat pane.
// § Tests live in src/__tests__/chat.test.tsx (no DOM ; pure logic).

export type SlashCommand =
  | { kind: "plain"; text: string }
  | { kind: "clear" }
  | { kind: "cancel" }
  | { kind: "settings" }
  | { kind: "revoke_all" }
  | { kind: "spec"; query: string }
  | { kind: "help" }
  | { kind: "unknown"; raw: string };

/**
 * Parse a chat input. Slash-commands match `/<word>(<space><rest>)?`.
 * Anything that doesn't start with `/` is `plain`.
 */
export function parseSlash(input: string): SlashCommand {
  const trimmed = input.trim();
  if (!trimmed.startsWith("/")) {
    return { kind: "plain", text: trimmed };
  }
  const rest = trimmed.slice(1);
  const spaceIdx = rest.indexOf(" ");
  const cmd = spaceIdx === -1 ? rest : rest.slice(0, spaceIdx);
  const arg = spaceIdx === -1 ? "" : rest.slice(spaceIdx + 1).trim();

  switch (cmd) {
    case "clear":
      return { kind: "clear" };
    case "cancel":
      return { kind: "cancel" };
    case "settings":
      return { kind: "settings" };
    case "revoke-all":
    case "revoke_all":
      return { kind: "revoke_all" };
    case "spec":
      return { kind: "spec", query: arg };
    case "help":
    case "?":
      return { kind: "help" };
    default:
      return { kind: "unknown", raw: trimmed };
  }
}

/** Stable list of slash-commands for the help overlay. */
export const SLASH_HELP: ReadonlyArray<{ cmd: string; desc: string }> = [
  { cmd: "/clear", desc: "Clear chat history (visual only ; session retained)" },
  { cmd: "/cancel", desc: "Cancel the in-flight turn" },
  { cmd: "/settings", desc: "Open the settings pane" },
  { cmd: "/revoke-all", desc: "Sovereign-revoke all caps · downgrade to paranoid" },
  { cmd: "/spec <query>", desc: "Top-K substrate-knowledge query" },
  { cmd: "/help", desc: "Show this help overlay" },
];
