// § main.tsx — React 19 root entry-point.
// § Loaded by index.html ; mounts <App /> at #root.
import React from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./lib/theme.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("Mycelium: #root container not found in index.html");
}

createRoot(container).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
