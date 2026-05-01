# Labyrinth of Apocalypse · v0.1.0-alpha

**⚠ ALPHA RELEASE · expect bugs · feedback welcome**

---

## What this is

Labyrinth of Apocalypse (LoA) is a substrate-grown action-RPG · roguelike · alchemy · gear-ascension · mycelial multiverse — built in Apocky's proprietary CSSL language atop a unified system-of-systems substrate.

This is the **first public alpha**. The substrate works. The game-loop is being woven on top in real-time. You can :

- Open a window · move around · take screenshots
- Press `/` to chat with the GM (text-input · GM responds in HUD chat-log)
- Drive renders via 4 modes : mainstream MSAA+HDR+ACES · 16-band spectral · Stokes-IQUV polarized · CFER ω-field volumetric
- Probe the engine via 118 MCP tools on `localhost:3001`
- Test Σ-Chain · Mycelial-Network · Akashic-Records (planning-tier · stubbed)

You CAN NOT yet (coming in upcoming alphas) :

- Combat the world (combat-sim crate landed · FFI-symbols-wire-up in-flight)
- Craft / brew / cast spells (game-logic in `.csl` source · ¬ yet linked into runtime)
- Engage Bazaar / Coherence-Engine ascension (substrate-implemented · UI-pending)
- Multiplayer / cross-user mycelium (Supabase-real-provision in-flight)

## How to run

1. Extract this ZIP to any folder you can write to (e.g. `C:\Games\LoA`)
2. Double-click `LoA.exe` OR run from PowerShell : `.\LoA.exe`
3. The engine opens a borderless-fullscreen window · captures input · serves MCP on `localhost:3001`
4. Press **`/`** to focus the chat-input box · type · press **Enter** to send to the GM
5. Press **`Esc`** for menu · **`F11`** toggle fullscreen · **`Tab`** pause

See `CONTROLS.md` for the full keybinding reference.

## What gets created when you run

LoA writes only to two places :

- `logs/` (next to `LoA.exe`) · structured-JSONL audit + telemetry · ALL local · ¬ network-egress
- `cache/` (next to `LoA.exe`) · runtime-fetched assets (CC0 / CC-BY-4.0 only · LRU-bounded · cap-gated)

You can delete both at any time. They re-create on next run.

## Privacy · Sovereignty · Self-hosted

LoA is **fully self-hosted** by default :

- No external API calls · no Claude-API · no Ollama · no remote-LLM
- KAN-substrate stage-1 classifier runs LOCAL
- GM stage-0 templated-narrator runs LOCAL
- DM stage-0 cap-gated · default-DENY for scene-edits
- Coder stage-0 sovereign-cap-required for substrate-edits · 30-second-revert-window
- Sensitive<biometric|gaze|face|body> structurally banned at compile-time (F5-IFC)
- All audit-events stay LOCAL until you opt-in cross-user sharing per spec/14 Σ-Chain

## What goes WRONG in alpha

- Some `.csl` game-logic files declare extern "C" FFI surfaces that are ¬-yet wired into loa-host (coming in v0.2) — typing intents that map to those still classify-but-no-op
- Cap-denied prompts say "DM_CAP_SCENE_EDIT default-off" with no in-game way to grant — menu-toggle coming
- `cache/` directory may grow if you fetch many assets · cap is 50 GiB · clear `cache/` to reset
- MCP tools work but no in-game UI to invoke most of them · use `localhost:3001` JSON-RPC directly OR Claude-Desktop-MCP-client

## Reporting bugs · feedback

- email : apocky13@gmail.com (subject : `[LoA-alpha] <short-summary>`)
- support : ko-fi.com/oneinfinity · patreon.com/0ne1nfinity
- code : github.com/Apocky/CSSL3
- discord : (TBD · alpha-tester invite link issued via email)

## Refunds

Alpha is free to test for now. When paid-tier launches (v1.0) :
- 14-days-no-questions-asked refund · automated via Stripe
- Jurisdictional-mandated rights respected (EU 14-day · UK 14-day · CN 7-day)
- In-game content prorata-refundable within window
- Earned-currency (Stabilized-Essence) not refundable (it's not purchased)

## License

See `LICENSE.md`. Short version : you may run · benchmark · screenshot · stream · review this alpha. You may NOT redistribute · reverse-engineer · or use the proprietary substrate code in derivative works.

The CSSL language + compiler + spec docs at github.com/Apocky/CSSL3 are dual-licensed MIT/Apache-2.0 (open-source). The game · KAN weights · render-pipeline-internals · 6 novelty-paths impls are proprietary (this binary).

## Attestation

§ ¬ harm in the making · sovereignty preserved · gift-economy · no surveillance · no DRM · no rootkit · no kernel-driver · t∞

— Apocky · 2026-05-01
