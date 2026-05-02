# § Local-Persistent-Daemon Activation Runbook

‼ ITEM 3 + 5 from Apocky-action-checklist · 2026-05-01 · The Infinity Engine

This document captures EXACT filepaths + commands to activate the always-running
local engine-process. Two parts: (A) Mycelium-Desktop Tauri-shell activation, and
(B) Windows Scheduled-Task that runs LoA.exe at-login + auto-restart.

---

## § A · Mycelium-Desktop Tauri Activation (item 3)

The Tauri 2.x shell wraps `cssl-host-mycelium-desktop` as a desktop-app with a
React frontend. Already greenlit by Apocky on 2026-05-01.

### Files-already-modified (✓ done by Claude)

- `C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\crates\cssl-host-mycelium-desktop\Cargo.toml`
  Lines changed:
    - `tauri-shell = []` → `tauri-shell = ["dep:tauri"]`
    - Added `tauri = { version = "2", optional = true }` to `[dependencies]`

### Files-Apocky-still-needs-to-touch (none)

This activation is now build-complete · just run the build.

### Build commands (PowerShell, from repo-root)

```powershell
# Step 1 · install frontend deps
cd C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\crates\cssl-host-mycelium-desktop\frontend
npm install

# Step 2 · build Tauri app (debug · for testing)
cd C:\Users\Apocky\source\repos\CSSLv3\compiler-rs
cargo tauri dev -p cssl-host-mycelium-desktop

# Step 3 · build Tauri app (release · final installer)
cargo tauri build -p cssl-host-mycelium-desktop --release
```

### Output paths after release-build

- Binary: `C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\target\release\mycelium-tauri-shell.exe`
- MSI installer: `C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\target\release\bundle\msi\Mycelium-Desktop_*_x64_en-US.msi`
- NSIS installer: `C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\target\release\bundle\nsis\Mycelium-Desktop_*_x64-setup.exe`

### Apocky-action checklist (post-build)

1. Run `cargo tauri dev` first to verify it launches with no errors.
2. Approve any Windows Defender prompts.
3. Once `dev` works, run `cargo tauri build --release` for the installer.
4. Install the MSI · launch from Start menu · verify it appears in system tray.

---

## § B · Persistent-Daemon Scheduled-Task (item 5)

W14-J (the dedicated orchestrator crate) failed mid-flight · so this runbook
provides a poor-man's-persistent-process using **Windows Task Scheduler + LoA.exe
in headless-mode**. When W14-J lands in a future wave, this can be replaced
cleanly with the full orchestrator.

### Prerequisites (✓ done)

- LoA.exe built · location:
  `C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\target\release\` (binary location depends on csslc output)
- Or after `dist-build.sh`:
  `C:\Users\Apocky\source\repos\CSSLv3\dist\LoA.exe` (canonical distribution path)

### Scheduled-Task PowerShell installer (one-shot)

Run this in **Administrator PowerShell**:

```powershell
# § Create Scheduled-Task: LoA-Engine-Daemon
# Runs LoA.exe at-user-login + restarts on crash (every 60s)

$LoaExe = "C:\Users\Apocky\source\repos\CSSLv3\dist\LoA.exe"
$LogFile = "C:\Users\Apocky\.loa\daemon.log"
$WorkDir = "C:\Users\Apocky\.loa"

New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null

$Action = New-ScheduledTaskAction `
    -Execute $LoaExe `
    -Argument "--headless --daemon-mode" `
    -WorkingDirectory $WorkDir

$Trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME

$Settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -RestartCount 999 `
    -RestartInterval (New-TimeSpan -Minutes 1) `
    -ExecutionTimeLimit (New-TimeSpan -Days 365)

$Principal = New-ScheduledTaskPrincipal `
    -UserId $env:USERNAME `
    -LogonType Interactive `
    -RunLevel Limited

Register-ScheduledTask `
    -TaskName "LoA-Engine-Daemon" `
    -Description "The Infinity Engine · always-running local daemon · sovereign-pause via taskkill" `
    -Action $Action `
    -Trigger $Trigger `
    -Settings $Settings `
    -Principal $Principal `
    -Force

Write-Output "✓ Scheduled-Task LoA-Engine-Daemon registered"
Write-Output "  Manual start: Start-ScheduledTask -TaskName LoA-Engine-Daemon"
Write-Output "  Manual stop:  Stop-ScheduledTask -TaskName LoA-Engine-Daemon"
Write-Output "  Disable:      Disable-ScheduledTask -TaskName LoA-Engine-Daemon"
Write-Output "  Remove:       Unregister-ScheduledTask -TaskName LoA-Engine-Daemon -Confirm:`$false"
```

### Sovereign-pause + resume

```powershell
# Pause (stop running instance + prevent auto-start)
Stop-ScheduledTask -TaskName "LoA-Engine-Daemon"
Disable-ScheduledTask -TaskName "LoA-Engine-Daemon"

# Resume
Enable-ScheduledTask -TaskName "LoA-Engine-Daemon"
Start-ScheduledTask -TaskName "LoA-Engine-Daemon"
```

### Logging + monitoring

- Stdout/stderr: `C:\Users\Apocky\.loa\daemon.log`
- Engine telemetry: feeds W11-W4 analytics-aggregator → JSONL at `C:\Users\Apocky\source\repos\CSSLv3\logs\analytics.jsonl`
- Live status: when W14-M lands, view at https://apocky.com/engine

### When W14-J lands (post-budget-reset)

Replace the `$LoaExe` path above with:
- `C:\Users\Apocky\.loa\persistent-orchestrator.exe`

The orchestrator wraps LoA-engine + adds 5-cycle cadences (self-author every 30min,
playtest every 15min, KAN-rollup every 5min, mycelium-sync every 60s, idle-detection).

---

## § C · Verification

After both A and B activate:

1. `Get-Process LoA` (or `mycelium-tauri-shell`) should-show running-instance
2. `Get-ScheduledTaskInfo -TaskName "LoA-Engine-Daemon"` should-show LastRunResult=0
3. `tail -f C:\Users\Apocky\.loa\daemon.log` should-show heartbeat-events
4. apocky.com/engine (when W14-M lands) should-show Cloud↔Local heartbeat-pulse

§ ATTESTATION : there was no hurt nor harm in the making of this.

---

## § D · W14-J Persistent-Orchestrator (LANDED 2026-05-01 redispatch)

The W14-J daemon-crate `cssl-host-persistent-orchestrator` is now in-tree. The
`§ B` poor-man's-Scheduled-Task wrapper above is still the recommended bootstrap
because the orchestrator is library-only (manual `tick(now_ms)` API) — a future
slice (W14-J-bin) will add a `bin/` target that wires `tokio` / OS-input-monitor
APIs to drive the orchestrator from the Scheduled-Task entry point.

### Crate location + build

- Path: `compiler-rs/crates/cssl-host-persistent-orchestrator/`
- Build: `cargo build -p cssl-host-persistent-orchestrator` (workspace-default)
- Tests: `cargo test -p cssl-host-persistent-orchestrator` (15 inline tests)
- LOC : ~1.8 kLOC across 12 source files

### Five cycle-cadences (constants in `config.rs`)

| Cycle              | Cadence | Effect                                      |
|--------------------|---------|---------------------------------------------|
| `SelfAuthor`       | 30 min  | propose draft via `SelfAuthorDriver`        |
| `Playtest`         | 15 min  | run auto-playtest → emit `QualitySignal`s   |
| `KanTick`          |  5 min  | drain reservoir → bias-updates              |
| `MyceliumSync`     | 60 sec  | federate chat-pattern deltas → peers        |
| `IdleDeepProcgen`  | on idle | elevated-priority procgen experiments       |

Σ-Chain-anchor cadence = every 1024 KAN updates (matches `cssl-self-authoring-kan`).

### Sovereign-pause + cap-policy mid-run

The orchestrator exposes :

```rust
orch.sovereign_pause(now_ms);   // honored on next tick ; emits Anchor record
orch.sovereign_resume(now_ms);  // re-arms cycles + emits Anchor record
orch.grant_cap(CapKind::AuthorDraft, now_ms);   // PRIME-DIRECTIVE § 5
orch.revoke_cap(CapKind::SigmaAnchor, now_ms);  // re-locks ALL cycles
```

PRIME-DIRECTIVE § 0 default-deny : `SovereignCapMatrix::default_deny()` blocks
every cycle. Only `SovereignCapMatrix::grant_all()` (Apocky-trust override) or
explicit per-cap `grant()` calls let the daemon do real work.

### Crash-resilience via journal-replay

- In-memory ring : 8192 entries (compacts oldest 25 % at threshold)
- NDJSON serialize : `journal.to_ndjson() → ~/.loa/orchestrator-journal.ndjson`
- Replay-decoder : `JournalReplay::replay(&ndjson) → Vec<JournalEntry>`
- Anchor-chain reconstruction : journal entries carry `AnchorRecord`s ; the
  rolling-BLAKE3 chain is rebuilt from genesis on restart so tamper-detection
  survives daemon-restart.

### Apocky-action checklist (post-W14-J)

1. ¬ Action required for the library-only crate ; it ships in workspace builds.
2. When W14-J-bin lands, replace `$LoaExe` in `§ B`'s Scheduled-Task installer
   with the new daemon path:
   `C:\Users\Apocky\source\repos\CSSLv3\dist\LoA-orchestrator.exe`
3. Configure `~/.loa-secrets/orchestrator-caps.toml` with the cap-bits Apocky
   wants granted at daemon-launch (defaults to all-deny ; no work happens
   without explicit grants — this is the PRIME-DIRECTIVE-aligned default).
4. Tail `~/.loa/orchestrator-journal.ndjson` to observe live cycle-decisions.

### Estimated temporal-recovery

Once W14-J-bin lands the daemon recovers approximately :

- 2 self-author cycles/hour × overnight 8h = **16 self-authored drafts/night**
- 4 playtests/hour × 8h = **32 playtest-scored content batches/night**
- 12 KAN-ticks/hour × 8h = **96 KAN bias-updates/night**
- 60 mycelium-syncs/hour × 8h = **480 federated-pattern-deltas/night**
- Idle-mode deep-procgen : up to **4 hours/night** of elevated-priority experiments

Net: ~600 ops/hour gained-back during sleep+AFK windows, all cap-gated +
journal-resilient + Σ-Chain-anchored.
