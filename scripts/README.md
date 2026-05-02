# § scripts/ · The Infinity Engine · Automation Hub

Every Apocky-action-item from the W11..W16 waves wrapped as a runnable script.
Stop pasting cmd-lines · double-click batch-files · sovereignty-clean.

## Top-level launcher

**`<repo>/setup.bat`** — double-click · auto-elevates to Admin · runs the full setup pipeline.

## Folders

| Folder | Purpose |
|--------|---------|
| `scripts/setup/` | First-time activation (daemon · Mycelium · caps · LoA-rebuild · deploy) |
| `scripts/dev/` | Developer-tools (test-all · clean · csslc-check-all) |
| `scripts/keys/` | Ed25519 cap-key generation + rotation |
| `scripts/db/` | Apocky-Hub Supabase utilities (status · migrate · drop-test) |
| `scripts/git/` | Wave-W11..W16 commit-survey + branch-state |
| `scripts/health/` | Comprehensive engine + cloud health-check |

## Most-useful one-liners

```powershell
# DOUBLE-CLICKABLE
.\setup.bat                              # full setup (Admin auto-elevate)
.\scripts\setup\status.bat               # status check (no admin)
.\scripts\dev\test_all.ps1               # run every test-suite

# SETUP
.\scripts\setup\1_install_daemon.ps1 -Action register
.\scripts\setup\2_install_mycelium.ps1 -Action build
.\scripts\setup\3_grant_caps.ps1 -Grant weapons,fps_feel,movement_aug,loot
.\scripts\setup\4_rebuild_loa.ps1
.\scripts\setup\5_deploy_apocky_com.ps1
.\scripts\setup\6_full_setup.ps1 -Yes    # auto-confirm everything

# DEV
.\scripts\dev\test_all.ps1 -Only cargo   # only cargo tests
.\scripts\dev\clean.ps1 -All -Yes        # full clean
.\scripts\dev\csslc_check_all.ps1 -Verbose   # which-.csl-files-pass-csslc

# KEYS
.\scripts\keys\generate_cap.ps1 -Role A   # generate cap-A
.\scripts\keys\generate_cap.ps1 -Role D -Rotate   # backup-then-replace cap-D

# HEALTH
.\scripts\health\engine_health.ps1        # full report
.\scripts\health\engine_health.ps1 -Quick # local-only (no network)

# GIT
.\scripts\git\wave_status.ps1             # all-waves commit-summary
.\scripts\git\wave_status.ps1 -Wave W14 -Detailed  # W14 detailed

# DB
.\scripts\db\supabase.ps1 -Action status  # connection check
.\scripts\db\supabase.ps1 -Action shell   # psql shell
```

## Sovereignty-clean reset

```powershell
.\scripts\setup\3_grant_caps.ps1 -RevokeAll
.\scripts\setup\1_install_daemon.ps1 -Action remove
.\scripts\dev\clean.ps1 -Targets logs -Yes
```

That returns the engine to default-deny + no-running-daemon + clean-logs.
Keys + dist-zip + Mycelium-installer remain (manual remove if you want true-cold-state).

## Common workflow examples

**"I just changed `loa-host` source · rebuild + redeploy"**
```powershell
.\scripts\dev\test_all.ps1 -Only cargo
.\scripts\setup\4_rebuild_loa.ps1
```

**"I just changed `cssl-edge` source · re-deploy apocky.com"**
```powershell
.\scripts\setup\5_deploy_apocky_com.ps1
```

**"My engine isn't doing anything · debug"**
```powershell
.\scripts\health\engine_health.ps1
.\scripts\setup\3_grant_caps.ps1                        # check what's granted
.\scripts\setup\1_install_daemon.ps1 -Action status
Get-Content $env:USERPROFILE\.loa\daemon.log -Tail 20   # latest logs
```

**"I want to start fresh · wipe and rebuild"**
```powershell
.\scripts\dev\clean.ps1 -All -Yes
.\setup.bat
```

§ ATTESTATION : there was no hurt nor harm in the making of these scripts.
