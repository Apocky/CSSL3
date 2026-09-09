# Apocrypha desktop

The Windows client for Apocrypha. It signs in with an Apocky account and talks to
the same public contract the phone clients use:
`https://www.apocky.com/api/mobile/{config,status,turn,sessions}`.

Nothing about Apocrypha runs on the computer. This is a thin client: it holds a
sign-in, sends a message, and reads that account's own history back from the
service. The website entry is `/download/apocrypha`.

Spec: [`specs/operations/APOCRYPHA_DESKTOP_DISTRIBUTION_2026-09-09.csl`](../../specs/operations/APOCRYPHA_DESKTOP_DISTRIBUTION_2026-09-09.csl)

## Shape

The Rust process is the application. The webview is a renderer — it never holds
a token, never reaches the network, and cannot name an endpoint. Every action is
a typed command that returns the whole view.

| file | what it owns |
| --- | --- |
| `src/protocol.rs` | the wire contract and the outbound endpoint allowlist |
| `src/api.rs` | HTTPS transport, status-to-sentence mapping, bounded reads |
| `src/session.rs` | a sign-in, verified against `/auth/v1/user` rather than the token payload |
| `src/store.rs` | the refresh token, encrypted with DPAPI under the Windows user |
| `src/journal.rs` | messages whose outcome the service has not confirmed |
| `src/controller.rs` | the flows, and everything a person is told |
| `src/main.rs` | the Tauri window and the command surface |
| `frontend/` | Vite + React view over those commands |

`protocol.rs`, `api.rs`, `session.rs`, `store.rs`, `journal.rs` and
`controller.rs` are ports of the Android client's `Protocol.java`,
`ApiClient.java`, `AuthSession.java`, `SecureStore.java`, `RequestJournal.java`
and `AppController.java` in `Apocv4/apps/mobile/android`. **Keep them in step.**
A change to what either client accepts, refuses, or says is a change to the
shared contract, not a desktop-only detail.

This crate is deliberately outside the `compiler-rs` workspace: Tauri pulls in
200+ transitive dependencies and must not enter the compiler's default build.

## Where it installs, and why the product name is what it is

`productName` is **"Apocrypha Desktop"**, not "Apocrypha". NSIS installs a
`currentUser` build to `%LOCALAPPDATA%\<productName>`, and
`%LOCALAPPDATA%\Apocrypha` already belongs to another Apocrypha component — it
holds that component's `secrets\` and `security\acl-backups\`. Installing into
it once (2026-09-09) put this client's binary alongside another program's
secrets; the uninstaller behaved and removed only its own two files, but an
install root shared with foreign state is not something to leave to an
uninstaller's good manners.

So the folder, the Start-menu entry and the Add/Remove-programs entry read
"Apocrypha Desktop". **The window is still titled "Apocrypha"** — see
`app.windows[0].title`. Do not "tidy" `productName` back.

## What is stored on the computer

Under `%LOCALAPPDATA%\Apocky\Apocrypha`, encrypted with DPAPI and bound to both
the Windows user and the record's own name:

- the Supabase **refresh token** and the account id
- conversation **identifiers** opened on this computer, and any unresolved request ids

No access token. No message text. The folder is created on the first write, not
at startup, so opening the application and never signing in leaves nothing at
all behind. Signing out inside the application deletes all of it and calls
`/auth/v1/logout?scope=local`.

## Unconfirmed messages

A turn that times out may still be running on the server. The client records it
and refuses to send again in that conversation until history resolves it — it
never resends on its own, because that would duplicate a person's message. Only
statuses that prove the service never accepted the turn (400, 401, 403, 404,
415, 429) release it and hand the text back to the composer.

## Build

```powershell
cd apps\apocrypha-desktop\frontend; npm install
cd ..; cargo tauri build
```

Output: `target\release\bundle\nsis\Apocrypha_<version>_x64-setup.exe`.

## Test

```powershell
cd frontend; npm test          # view rules
cd ..; cargo test --release    # contract, storage, journal, controller
cargo test --release -- --ignored   # reaches the live service
```

The ignored test is the only one that touches the network. Run it when checking
a release; leave it out otherwise so the suite stays offline and deterministic.

## Release

```bash
bash cssl-edge/scripts/dist-build-apocrypha-desktop.sh
```

That runs the tests, builds the installer, observes that the built binary starts
and can reach the service, stages the installer into
`cssl-edge/public/downloads/`, and writes
`cssl-edge/public/releases/apocrypha-desktop/manifest.json`.

The manifest is a claim the site re-checks: `loadDesktopRelease` re-hashes the
staged file at page-build time and degrades the page to "not available yet"
rather than publish a checksum that does not match. Verification results other
than the two mechanical gates are carried forward from the previous manifest, so
a rebuild never silently upgrades a check nobody re-ran.

Deploying the site is a separate, deliberate act.

## Signing

The installer is **unsigned**. Windows shows "Windows protected your PC", and
the download page says so and explains how to check the SHA-256 first. An
Authenticode certificate would remove that warning; until one is bought, this
stays a preview and the page must keep saying it plainly.

## Not this client

macOS and Linux are not built here — this host has no Apple toolchain, and Linux
needs WSL2 or a CI runner. The contributor node (`/download/apocrypha-node`) and
Mycelium (`compiler-rs/crates/cssl-host-mycelium-desktop`) are different products
with different consent surfaces; do not merge them into this one.

## Attestation

There was no hurt nor harm in the making of this, to anyone, anything, or anybody.
