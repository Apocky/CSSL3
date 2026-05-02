# § scripts/setup · The Infinity Engine · one-click activation pipeline

PowerShell scripts that wrap every Apocky-action-item from W11..W16 so you can stop pasting cmd-lines.

## Quickstart

**Open Administrator PowerShell** (right-click → "Run as Administrator") and:

```powershell
cd C:\Users\Apocky\source\repos\CSSLv3
.\scripts\setup\6_full_setup.ps1
```

That runs all 6 steps with confirm-prompts. Add `-Yes` to skip prompts.

## Per-step scripts (run individually if you want)

| # | Script | Does | Admin? |
|---|--------|------|--------|
| 1 | `1_install_daemon.ps1` | Register/start/stop/remove `LoA-Engine-Daemon` Task Scheduler entry | yes |
| 2 | `2_install_mycelium.ps1` | Build + install Mycelium-Desktop NSIS-bundle | yes (install only) |
| 3 | `3_grant_caps.ps1` | Edit `~/.loa-secrets/orchestrator-caps.toml` (Σ-cap grants) | no |
| 4 | `4_rebuild_loa.ps1` | `cargo build -p loa-host --features runtime --release` + `dist-build.sh` | no |
| 5 | `5_deploy_apocky_com.ps1` | Vercel deploy + apex-alias + Cloudflare cache-purge | no |
| 6 | `6_full_setup.ps1` | Runs all 5 above in sequence with confirm-prompts | yes |

## Common commands

```powershell
# Check daemon state
.\scripts\setup\1_install_daemon.ps1 -Action status

# Stop daemon (sovereign-pause)
.\scripts\setup\1_install_daemon.ps1 -Action stop

# Resume daemon
.\scripts\setup\1_install_daemon.ps1 -Action start

# Remove daemon entirely
.\scripts\setup\1_install_daemon.ps1 -Action remove

# Build Mycelium-Desktop without installing
.\scripts\setup\2_install_mycelium.ps1 -Action build

# Launch Mycelium directly (no install)
.\scripts\setup\2_install_mycelium.ps1 -Action launch

# Show current cap-grants
.\scripts\setup\3_grant_caps.ps1

# Grant ONE cap (e.g. weapons system gets visible-effects)
.\scripts\setup\3_grant_caps.ps1 -Grant weapons

# Revoke ONE cap
.\scripts\setup\3_grant_caps.ps1 -Revoke weapons

# Grant everything (¬ recommended unless you know what you're doing)
.\scripts\setup\3_grant_caps.ps1 -GrantAll

# Default-deny everything (sovereign-clean reset)
.\scripts\setup\3_grant_caps.ps1 -RevokeAll

# Quick rebuild without re-running cargo (just dist-build)
.\scripts\setup\4_rebuild_loa.ps1 -SkipBuild

# Deploy apocky.com without rebuilding
.\scripts\setup\5_deploy_apocky_com.ps1

# Just purge Cloudflare cache (after manual deploy)
.\scripts\setup\5_deploy_apocky_com.ps1 -Action purge
```

## Σ-cap grants (what each unlocks)

The W16 wire-up calls 6 systems per-frame · all default-deny · `3_grant_caps.ps1` flips them on:

| Cap | When granted | Effect |
|-----|--------------|--------|
| `weapons` | per-frame fire-cap + accuracy-recovery | `WeaponsState.shots_fired` increments · projectile-pool ticks |
| `fps_feel` | per-frame ADS + recoil + bloom | RMB-zoom · recoil-kick on shot · cone-grow with sustained-fire |
| `movement_aug` | sprint + slide + jump-pack + parkour | Shift sprints · Crouch-while-sprint slides · double-jump · wall-run |
| `loot` | combat-end → 8-tier rarity drop | Kill drops loot · 8-tier rarity (Common..Chaotic) · KAN-bias-aware |
| `mycelium_heartbeat` | 60s federation accumulator | Cross-instance pattern-share · k-anon ≥10 · sovereign-revocable |
| `content` | rating + flag + quality-signal ingest | UGC ingest pipelines feed-trending + KAN |
| `self_author` | Claude-via-llm-bridge generates CSSL-source | Writes new content while-Apocky-offline · Σ-cap-gated mutate |
| `playtest` | automated-GM playtests published-content | Scores Fun/Balance/Safety/Polish · feedback to-author |
| `kan_tick` | bias-update from quality-signals | Per-player KAN-bias-vector adapts to-aesthetic-preference |
| `network_egress` | cloud-sync (mycelium + analytics + hotfix) | Outbound HTTP allowed · ¬ silent-egress |

**Recommended starting tier** : `weapons` + `fps_feel` + `movement_aug` (visible-game-feel) + `loot` (drops). Leave `network_egress` denied unless you've reviewed what each system sends.

## File locations

- Daemon binary: `C:\Users\Apocky\.loa\loa-orchestrator-daemon.exe`
- Daemon log: `C:\Users\Apocky\.loa\daemon.log`
- Cap grants: `C:\Users\Apocky\.loa-secrets\orchestrator-caps.toml`
- Cloudflare token: `C:\Users\Apocky\.loa-secrets\cloudflare.env`
- LoA dist-zip: `<repo>\dist\LoA-v0.1.0-alpha-windows-x64.zip`
- Mycelium installer: `<repo>\compiler-rs\target\release\bundle\nsis\Mycelium_0.1.0-alpha_x64-setup.exe`
- Mycelium binary: `<repo>\compiler-rs\target\release\mycelium-tauri-shell.exe`

## Sovereignty

Every step is sovereign-revocable:

- **Daemon**: stop / disable / unregister via `1_install_daemon.ps1`
- **Mycelium-Desktop**: standard Windows uninstaller via Start menu → Settings → Apps
- **Caps**: `3_grant_caps.ps1 -RevokeAll` resets to default-deny
- **LoA.exe**: just-don't-run-it (no auto-launch unless daemon registered)
- **apocky.com**: standard Vercel rollback / domain-disconnect

§ ATTESTATION : there was no hurt nor harm in the making of these scripts.
