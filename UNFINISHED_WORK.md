
## 2026-09-24 room build (agent) -- PARK
- in flight before this prompt: nothing of mine in this tree (other lanes' uncommitted edits present: apps/apocrypha-desktop/*, cssl-edge/scripts/apocrypha-discord/bridge.ts -- NOT mine, left untouched)
- this task: apocky.com/room living chatroom (api/room/*, pages/room.tsx, scripts/apocrypha-room/loop.ts, tests/room-api.spec.ts) + apocrypha-core branch room/site-loop
- resume: git -C C:\Users\Apocky\source\repos\CSSLv3-wt-worklane-prod log --oneline -8 ; cssl-edge/scripts/apocrypha-room/loop.ts
- CLOSED 2026-09-24: room build committed (see git log room commits); loop NOT run, site NOT deployed, table NOT verified live (Apocky creates it). next: create table, deploy, `apocrypha` launcher on branch room/site-loop

## 2026-09-24 living-room deploy run (claude, session ec75f4ad) -- PARK
- in flight before this prompt: nothing of mine. other lanes' uncommitted edits present since 09-16/09-20:
  apps/apocrypha-desktop/* (local Work lane), cssl-edge/scripts/apocrypha-discord/bridge.ts, graphify-out/* (regenerated graph + 11 dated snapshot dirs ~800 MB untracked)
- this task: push worklane-prod -> merge origin/main (84c91bc) -> room = only chat surface -> bubbles/tooltips/plus-menu -> flagship lane -> migration 0057 on hub -> deploy + prove -> ff main
- resume: git -C C:\Users\Apocky\source\repos\CSSLv3-wt-worklane-prod log --oneline -12 ; git ls-remote origin worklane-prod
- next: commit the foreign edits as named slices (always-commit), push HEAD:worklane-prod

## 2026-09-24 steering: "make room messages pass through the jobs queue as intended" -- PARK
- in flight: `git merge --no-commit --no-ff origin/main` UNRESOLVED in this tree (15 conflicts: DU AccountChat/ChatThread/apocrypha.tsx,
  UU member-chat.ts package.json index.tsx worker/{prompt,qwen,worker}.ts 6 tests tsbuildinfo). origin/worklane-prod = e7b799e (pre-merge).
- resume: git status --porcelain | grep -E '^(UU|DU|UD|AA) ' ; abort path = git merge --abort
- supersedes step 4 "wire the room loop": room say -> enqueue job -> Vercel runner (flagship) / PC worker (free) -> answer row in the river.

## 2026-09-24 status check from Apocky ("where are the bubbles...") -- PARK
- in flight: merge of origin/main resolved + staged (tsc 0, npm test 0); committing + pushing now.
- NOT done yet: step 3 UI (bubbles/tooltips/plus menu), room->jobs queue, migrations (0057 fn half + room jobs), deploy.
- resume: git -C C:\Users\Apocky\source\repos\CSSLv3-wt-worklane-prod log --oneline -3 ; next = components/room/* UI pass

## 2026-09-25 flagship optimization request (max reasoning, max context, why Opus refused) -- PARK
- in flight: nothing uncommitted of mine (last: 1a571f5 memory tools). live: worker on hosted lane via AI_GATEWAY_API_KEY,
  Opus 5.5 intermittently 429 "No access to this model at this time" -> Sonnet 5 fallback.
- next: research gateway access/limits + Opus 5.5 specs; set effort max + context max on worker hosted lane + runner.
- CLOSED 2026-09-25: flagship optimization shipped in 9323a67 (effort max, 1M ctx, 64K out, gateway fallbacks). Opus refusal = per-team gateway limit (providerAttemptCount 0), not credits.

## 2026-09-25 post-compaction resume -- PARK
- in flight 1: desktop direct mode slice 1 (uncommitted): cssl-edge/lib/direct/{mode,services}.ts + direct-owner branch in
  lib/room/turn.ts resolveSpeaker + viewer.direct in pages/api/room/events.ts. next: tsc, then pages/api/direct/{health,services,stt,tts}.ts,
  room UI (mic, speak, notify, services panel), hidden local server 127.0.0.1:19141 + logon task.
- in flight 2: thesis job 9b505f2e-3874-4d1c-ac35-f6f7cf1336aa (background shell bgh068euo polls hub). next: report receipt.
- in flight 3: chaos-tarot frontier-for-paid-members; Explore agent adb2f0a1610f68494 mapping the pipeline. next: implement from its map.
- undeployed: main 9323a67 (and everything after) not yet on Vercel; deploy via Vercel MCP create_deployment only.
- later: Tauri app v0.2.0 (tray/notify/Work/direct+fallback), Tailscale serve (user must sign in), plan step-7 extras.
- CLOSED 2026-09-25: desktop direct mode + Apocrypha Desktop 0.2.0 (see commit "desktop: direct mode"); open: Tailscale serve
  (needs Apocky to start/sign in to Tailscale), uninstall check of the 0.2.0 installer.
