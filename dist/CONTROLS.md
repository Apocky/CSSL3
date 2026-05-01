# Labyrinth of Apocalypse · Controls Reference (alpha v0.1.0)

## Movement (default)

| key | action |
|---|---|
| `W` `A` `S` `D` | walk forward · left · back · right |
| `Space` | jump (when wired · alpha-stub) |
| `Shift` | hold-to-sprint (when wired · alpha-stub) |
| `Ctrl` | crouch (when wired · alpha-stub) |
| Mouse | look around (yaw + pitch) |

## Chat with the GM/DM/Coder

| key | action |
|---|---|
| `/` | focus the chat-input box (the "/ chat with the GM" pill at bottom-center activates) |
| `Enter` | submit your message · GM responds in HUD chat-log |
| `Esc` | unfocus chat-input (returns to game-input) |
| `Backspace` | delete one character |
| `Left` `Right` | move cursor within input |
| `Home` `End` | cursor to start / end |

### Chat prefix routing

| prefix | routes to | cap required |
|---|---|---|
| (none) | GM (default · stage-0 templated narrator) | `GM_CAP_TEXT_EMIT` (default-on) |
| `/gm <text>` | GM (explicit) | same as default |
| `/dm <text>` | DM (scene-arbiter) | `DM_CAP_SCENE_EDIT` (default-OFF · cap-denied prompt in chat-log until you grant via Settings) |
| `/code <text>` | Coder (AST-edit-proposer) | `CODER_CAP_AST_EDIT` (default-OFF · sovereign-cap-required for substrate-edits) |

## Render mode switching

| key | mode |
|---|---|
| `F1` | mainstream MSAA + HDR + Mailbox + ACES tonemap (default) |
| `F2` | 16-band hyperspectral KAN-BRDF |
| `F3` | Stokes-IQUV polarized · 16 Mueller presets · `P` cycles |
| `F4` | CFER ω-field volumetric raymarch · 1107-cell sample |

## Capture / record

| key | action |
|---|---|
| `F5` | screenshot (saves to `cache/screenshots/<timestamp>.png`) |
| `F6` | start / stop burst-capture (8 frames at 8fps) |
| `F7` | start / stop video-record (LFRC-format · CRC32-verified) |

## System / debug

| key | action |
|---|---|
| `Esc` | menu (resume · settings · controls · quit) · also unfocuses chat-input |
| `F11` | toggle borderless-fullscreen ↔ windowed |
| `Tab` | pause (engine-tick suspends · render continues) |
| `F12` | toggle debug-overlay (telemetry · histograms · MCP-traffic) |

## MCP (Model-Context-Protocol) server

LoA exposes 118 MCP tools on `localhost:3001` (TCP JSON-RPC) :
- `localhost:3001` · default-bind-localhost-only (per `cssl-host-config` policy)
- 118 tools across : `world.*` · `render.*` · `sense.*` · `audit.*` · `attestation.*` · `coder.*` · `intent.*` · `spontaneous.*` · `multiplayer.*` · `bazaar.*` (some are stubs · expanding wave-by-wave)

Use Claude-Desktop with the cssl-edge MCP-bridge OR any MCP-capable client to invoke these.

## Window / display

LoA opens a borderless-fullscreen window at native-resolution by default. Resize / window-mode controls in the menu (Esc).

## Sovereign-cap (sovereignty escape-hatch)

Hold `Ctrl + Shift + Alt + S` for 2 seconds at any-time to revoke ALL active caps :
- All cross-user mycelium-egress halts immediately
- All Akashic-imprint emission halts
- All multiplayer signaling disconnects
- Coder pending-edits cancel · stage-0 fallback engages
- Game-state remains intact · LOCAL-only

This is the "panic button" · always-effective · always-recoverable.

---

§ controls subject-to-change in alpha · final v1.0 keybindings will be remappable via Settings menu (cssl-host-omni-input crate · landing in W8-A3)

§ ATTESTATION ¬ harm · ¬ surveillance · ¬ DRM · sovereignty-preserved · t∞
