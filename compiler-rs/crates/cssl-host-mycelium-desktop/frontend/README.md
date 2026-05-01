# Mycelium frontend (Tauri 2.x · React 19 · Vite)

This directory holds the source for the Mycelium app's React frontend.
It is **source code, not built artifacts** — the files are tracked in git
and built locally by Apocky when ready to ship `Mycelium.exe`.

## Status

- Frontend source : tracked in git (this directory).
- Tauri runtime : feature-gated behind `--features tauri-shell` in the
  Cargo.toml of the parent crate. **The default workspace `cargo build`
  does NOT pull Tauri** (200+ transitive crates avoided).
- Apocky-actions to enable the real desktop build are below.

## Apocky-action checklist · enable the real Tauri runtime

1. **Edit `../Cargo.toml`** — uncomment / add the optional Tauri dep :
   ```toml
   [dependencies]
   tauri = { version = "2", optional = true }
   ```
   then change the `tauri-shell` feature line to :
   ```toml
   tauri-shell = ["dep:tauri"]
   ```

2. **Install Node deps** (one-time per clone) :
   ```bash
   cd frontend
   npm install
   ```

3. **Install Tauri CLI** (one-time per machine) :
   ```bash
   cargo install tauri-cli --version "^2.0"
   ```

4. **Live-reload dev** :
   ```bash
   cargo tauri dev
   ```

5. **Build installer** :
   ```bash
   cargo tauri build
   ```
   Output : `target/release/bundle/nsis/Mycelium_0.1.0-alpha_x64-setup.exe`.

## Notes

- **WebView2 runtime** is required at user-runtime ; Windows 11 ships it
  pre-installed, so no end-user action.
- **Code-signing certificate** is deferred — post-W10 Apocky-action.
  Until then, Windows SmartScreen will flag the unsigned binary on first
  launch ; users click "More info → Run anyway".
- **CSP** is locked down in `../tauri.conf.json` to `'self'` plus the
  Anthropic API + Supabase domains. Any new domain must be added there.

## Test

Vitest unit tests (`src/__tests__/`) cover :
- IPC type-safety mocks (5 tests) — `ipc.test.ts`
- Theme constants (3 tests) — `theme.test.ts`
- Slash-command parsing (4 tests) — `chat.test.tsx`
- Type-discriminator tags (3 tests) — `types.test.ts`

Run :
```bash
npm run test
```

These tests do not depend on the Tauri runtime ; they validate the IPC
type contract + pure helper functions.
