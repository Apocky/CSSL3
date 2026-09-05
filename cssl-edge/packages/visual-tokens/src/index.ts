export const visualTokens = {
  color: {
    soil: "#271c18",
    bark: "#45312a",
    clay: "#9f5f43",
    ember: "#c66b47",
    amber: "#dcaa62",
    lichen: "#71805e",
    moss: "#49593e",
    parchment: "#f3eadb",
    linen: "#fbf7ef",
    moon: "#fffdf8",
  },
  radius: {
    small: "0.5rem",
    medium: "1rem",
    large: "1.75rem",
    round: "999rem",
  },
  measure: {
    prose: "68ch",
    wide: "82rem",
  },
  timing: {
    immediate: "80ms",
    gentle: "180ms",
  },
} as const;

export type VisualTokens = typeof visualTokens;
