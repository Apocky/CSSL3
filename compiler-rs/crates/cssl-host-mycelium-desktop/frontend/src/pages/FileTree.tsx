// § FileTree.tsx — git-decorated tree, click-to-include-in-context.
// § Stage-0 stub : reads sandbox-paths from config + renders a flat list.
//   Wave-C2 wires real file-tree + git-status decoration.
import { useEffect, useState } from "react";
import * as ipc from "../lib/ipc";

interface FileEntry {
  path: string;
  included: boolean;
  status: "tracked" | "modified" | "untracked";
}

export default function FileTree() {
  const [entries, setEntries] = useState<FileEntry[]>([]);

  useEffect(() => {
    void ipc.getConfig().then((resp) => {
      if (resp.type === "config") {
        setEntries(
          resp.config.sandbox_paths.map((p) => ({
            path: p,
            included: false,
            status: "tracked" as const,
          }))
        );
      }
    });
  }, []);

  function toggle(path: string) {
    setEntries((prev) =>
      prev.map((e) => (e.path === path ? { ...e, included: !e.included } : e))
    );
  }

  return (
    <div>
      <h2 style={{ marginTop: 0 }}>Files</h2>
      <p style={{ color: "var(--fg-muted)" }}>
        Sandbox paths from config. Click a path to include in next-turn context.
        Stage-0 placeholder ; wave-C2 wires git-decoration + tree.
      </p>
      {entries.length === 0 && (
        <div style={{ color: "var(--fg-muted)" }}>
          No sandbox paths configured. Add some via Settings.
        </div>
      )}
      <ul style={{ listStyle: "none", padding: 0 }}>
        {entries.map((e) => (
          <li
            key={e.path}
            onClick={() => toggle(e.path)}
            style={{
              padding: "var(--space-xs) var(--space-sm)",
              cursor: "pointer",
              background: e.included ? "var(--bg-surface)" : "transparent",
              borderLeft: `2px solid ${
                e.included ? "var(--accent-purple)" : "transparent"
              }`,
            }}
          >
            <span className="glyph">
              {e.status === "modified" ? "◐" : e.status === "untracked" ? "○" : "✓"}
            </span>
            {e.path}
          </li>
        ))}
      </ul>
    </div>
  );
}
