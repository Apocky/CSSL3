// § chat.test.tsx — slash-command parsing logic (pure function ; no DOM).
import { describe, expect, it } from "vitest";
import { parseSlash, SLASH_HELP } from "../lib/slash";

describe("parseSlash", () => {
  it("returns plain for non-slash input", () => {
    const r = parseSlash("hello world");
    expect(r.kind).toBe("plain");
    if (r.kind === "plain") {
      expect(r.text).toBe("hello world");
    }
  });

  it("parses /clear to clear-kind", () => {
    expect(parseSlash("/clear").kind).toBe("clear");
  });

  it("parses /spec <query> with arg captured", () => {
    const r = parseSlash("/spec mycelium desktop");
    expect(r.kind).toBe("spec");
    if (r.kind === "spec") {
      expect(r.query).toBe("mycelium desktop");
    }
  });

  it("classifies unknown slash-commands as unknown", () => {
    const r = parseSlash("/notarealthing foo");
    expect(r.kind).toBe("unknown");
    expect(SLASH_HELP.length).toBeGreaterThan(3);
  });
});
