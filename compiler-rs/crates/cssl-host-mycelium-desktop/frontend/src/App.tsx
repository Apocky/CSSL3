// § App.tsx — top-level shell. 5 pages routed via simple state.
// § Pages : Chat · Settings · FileTree · ContextPane · Logs.
// § per spec/grand-vision/23 § UI § MAIN-WINDOW + § CHAT-PANE.
import { useEffect, useState } from "react";
import Chat from "./pages/Chat";
import Settings from "./pages/Settings";
import FileTree from "./pages/FileTree";
import ContextPane from "./pages/ContextPane";
import Logs from "./pages/Logs";

type Page = "chat" | "settings" | "files" | "context" | "logs";

const PAGES: ReadonlyArray<{ id: Page; label: string; glyph: string }> = [
  { id: "chat", label: "Chat", glyph: "§" },
  { id: "files", label: "Files", glyph: "∀" },
  { id: "context", label: "Context", glyph: "⊗" },
  { id: "logs", label: "Logs", glyph: "✓" },
  { id: "settings", label: "Settings", glyph: "⊘" },
];

export default function App() {
  const [page, setPage] = useState<Page>("chat");

  // Sovereign-revoke hot-key : Ctrl+Shift+Alt+S → trigger break-glass.
  useEffect(() => {
    function onKey(ev: KeyboardEvent) {
      if (ev.ctrlKey && ev.shiftKey && ev.altKey && ev.key.toLowerCase() === "s") {
        ev.preventDefault();
        // Best-effort dispatch ; ipc is async so we fire-and-forget.
        void import("./lib/ipc").then((m) => m.revokeAllSovereignCaps());
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "200px 1fr",
        height: "100vh",
        gap: 0,
      }}
    >
      <aside
        style={{
          background: "var(--bg-surface)",
          borderRight: "1px solid var(--border-subtle)",
          padding: "var(--space-md)",
        }}
      >
        <h1 style={{ fontSize: 18, margin: 0, marginBottom: "var(--space-lg)" }}>
          <span className="glyph">§</span>Mycelium
        </h1>
        <nav style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {PAGES.map((p) => (
            <button
              key={p.id}
              onClick={() => setPage(p.id)}
              className={p.id === page ? "primary" : ""}
              style={{ textAlign: "left" }}
            >
              <span className="glyph">{p.glyph}</span>
              {p.label}
            </button>
          ))}
        </nav>
      </aside>
      <main style={{ overflow: "auto", padding: "var(--space-lg)" }}>
        {page === "chat" && <Chat />}
        {page === "files" && <FileTree />}
        {page === "context" && <ContextPane />}
        {page === "logs" && <Logs />}
        {page === "settings" && <Settings />}
      </main>
    </div>
  );
}
