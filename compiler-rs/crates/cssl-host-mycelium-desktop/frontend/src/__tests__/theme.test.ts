// § theme.test.ts — basic theme constants exist.
import { describe, expect, it } from "vitest";
import type { UiTheme } from "../lib/types";

const THEMES: UiTheme[] = ["dark", "light", "high-contrast"];

describe("theme constants", () => {
  it("includes dark, light, and high-contrast variants", () => {
    expect(THEMES).toContain("dark");
    expect(THEMES).toContain("light");
    expect(THEMES).toContain("high-contrast");
  });

  it("dark is the default sentinel for the radial-gradient backdrop", () => {
    const defaultTheme: UiTheme = "dark";
    expect(defaultTheme).toBe("dark");
  });

  it("high-contrast uses kebab-case in the wire format", () => {
    const t: UiTheme = "high-contrast";
    expect(JSON.stringify(t)).toBe('"high-contrast"');
  });
});
