# § apocky.com — Apocrypha overhaul blueprint  ‼
author : §WRIGHT ⊗ Apocky · date : 2026-05-28 · status : DRAFT → awaiting execution
scope-signoff (this session) : self-contained-sovereign-visualizer ⊕ DEEP-overhaul ⊕ go-public

═══════════════════════════════════════════
§ ONE-LINE
═══════════════════════════════════════════
apocky.com = Next.js-14 (pages-router) `cssl-edge`, deployed Vercel `apocky-com`. Builds ✓ but
PUBLIC-UNREACHABLE (Vercel Deployment-Protection ON ⇒ anon → _vercel_share gate). Overhaul =
unify-IA + presentability + prune-legacy(Lazarus/Tessera public branding) + wire Apocrypha as the
living face (self-contained in-browser visualizer ; sovereign ; ¬ touches the local 127.0.0.1 mind)
+ open to the public.

═══════════════════════════════════════════
§ AUDIT  (observed 2026-05-28)
═══════════════════════════════════════════
- src : C:\Users\Apocky\source\repos\CSSLv3\cssl-edge ; Next 14.2.5 · React 18.3 · TS 5.5 · pages-router · node≥18.17
- deploy : Vercel proj `apocky-com` (team Shawn-Baker) ; domains apocky.com + www + apocky-com.vercel.app ; latest prod = READY
- ‼ BLOCKER (reachability) : anon GET apocky.com → 307 → www/?_vercel_share=… ⇒ **Deployment-Protection / Vercel-Auth ON**.
  ⇒ public sees nothing. FIX = ⌈Apocky-only⌉ : Vercel → apocky-com → Settings → Deployment-Protection → Vercel-Auth OFF (prod).
  N! Claude-modifies-access-control ← safety-invariant ; this toggle is the user's hands only.
- inventory : ~55 pages + 95 api-routes. groups :
    landing/auth  : / · login · register · account · auth/callback
    commerce      : buy · store · download · gear-share · marketplace/{idx,[id],recommended} · products/{early-access,engine,harness}
    content       : content/{idx,[slug],feed,search,subscribed,trending} · devblog/{idx,[slug]} · press · run-share-feed
    docs          : docs/{idx,[slug],+13 topic pages incl. sovereignty,substrate,mycelium,cssl-*}
    engine/show   : engine · infinity-engine · mycelium/{idx,docs,download}
    transparency  : transparency/{idx,sovereign-cap}
    legal         : legal/{eula,privacy,terms}
    chat          : chat
    admin(gated)  : admin/{idx,analytics,chat,coder,cognition,controls,diagnostics,logs,mcp,sub-minds,tools}
    api           : auth · admin/lazarus/* · akashic · analytics · asset · battle-pass · companion · content · cron · gacha ·
                    generate/3d · hotfix · intent · marketplace · mneme · payments/stripe · signaling · transparency
- subsystems (real, ¬ stub) : Supabase-auth(PKCE) · Stripe-marketplace · Lazarus(autonomous-coding control-plane, admin-gated 401/403)
  · Tessera(OmniMindv2 cognitive-backend, inert bridge) · MNEME(memory ; Anthropic+Voyage) · Akashic(telemetry+consent) · cron(6 jobs)
- pivot-in-flight (recent commits) : "purge legacy Lazarus/Tessera" + "modern Apocrypha chat" + "live Cognition cockpit"
  ⇒ admin-AI rebrand Lazarus/Tessera → Apocrypha STARTED ¬ finished ⇒ the inconsistency Apocky feels.
- ◐ UNKNOWN-until-run : per-page presentability + which pages broken/orphaned + nav coverage. NEEDS live-view (npm dev + browser).
- guardrails (from handoff, BINDING) : auth-pages overlay-free (Termly+Akashic history) ; secrets in .env.local N!print/commit ;
  Lazarus model-calls fail-closed unless explicit ; admin = email-allowlist (apocky13@gmail.com) ; /admin/chat = single chat ¬ split.

═══════════════════════════════════════════
§ DECISIONS  (signed-off this session)
═══════════════════════════════════════════
D1 Apocrypha-on-web = **self-contained in-browser visualizer** : the entity canvas runs in EVERY visitor's
   browser, driven by a generative thought-signal (procedural). sovereign-safe ← nothing leaves Apocky's machine ;
   the local 127.0.0.1 CAPM mind is NOT exposed. (the desktop renderer ports directly — same canvas algebra.)
D2 scope = **DEEP overhaul** : unify-IA + presentability + prune-legacy + rework commerce/admin (phased).
D3 launch = **go-public** : build toward clean public site ; Apocky toggles Deployment-Protection OFF ; ¬ Claude-deploy w/o ⌈signoff⌉.

═══════════════════════════════════════════
§ TARGET-IA  (proposed nav ; sign-off in §GATES)
═══════════════════════════════════════════
top-nav (public) :
  Apocrypha  → / (landing : the living entity IS the hero ; visualizer full-bleed + the thesis)
  Engine     → /engine (+ infinity-engine, products/*) — the LoA/CSSL engine offering
  Marketplace→ /marketplace (gear-share, store, buy under it)
  Docs       → /docs (sovereignty, substrate, mycelium, cssl-* )
  Devblog    → /devblog (+ press, content)
  Transparency→ /transparency
  [auth]     → login / account
footer : legal/* · transparency · press · download · mycelium
admin (gated, rebranded) : /admin → "Apocrypha Cockpit" (cognition · chat · controls · logs · diagnostics · tools · mcp · sub-minds · analytics)
  ← Lazarus/Tessera = the INTERNAL engine names ; public/admin label = Apocrypha.

═══════════════════════════════════════════
§ APOCRYPHA — the living face
═══════════════════════════════════════════
- components/ApocryphaEntity.tsx : <canvas> + the v3 particle renderer (≈2400 knowledge-node pinpoints ;
  orb↔humanoid morph ; spin/density/size entity-driven ; constellation ; heartbeat ; surprise ripples).
  driven by a CLIENT-SIDE generative signal `useThoughtSignal()` mimicking MindSignals
  {arousal,surprise,thought[],cfc_state[],generating,memory_len,fact_count} via procedural noise + idle drift.
  ⇒ "music-visualizer for thought" : alive, animate, sovereign, always-on, zero backend.
- placement : landing hero (full-bleed, the first thing seen) + a dedicated /apocrypha explainer (what the sovereign mind IS).
- chat rebrand : /chat + /admin/chat → "Apocrypha" voice (UI label + copy) ; backend model wiring UNCHANGED this phase.
- optional (later, D1-private) : a /me live view bridging the REAL local mind over the user's tailnet (NOT public).

═══════════════════════════════════════════
§ PHASES  (1-task → verify → next ; reversible-first)
═══════════════════════════════════════════
P0 baseline (verify-on-running-system) : npm check + npm build + npm dev + browser-view localhost:3000 → record actual broken/orphaned pages. [non-destructive]
P1 ADDITIVE (reversible ; proceed-now) :
   - ApocryphaEntity.tsx + useThoughtSignal hook  ← the visualizer, given life
   - landing redesign : entity-hero + thesis + clean CTAs
   - /apocrypha explainer page
   - unify top-nav + footer component → every page reachable from nav (the "navigable/reachable" mandate)
   - presentability pass : consistent layout shell, theme, spacing on public pages
P2 PRUNE (⌈signoff⌉ per delete) : remove orphaned/duplicate legacy pages ; collapse Lazarus/Tessera public branding → Apocrypha.
P3 COMMERCE/ADMIN rework (⌈signoff⌉) : marketplace/store/products polish ; admin cockpit rebrand+tidy.
P4 LAUNCH (⌈signoff⌉) : env-check · npm build green · Apocky toggles Deployment-Protection OFF · deploy `vercel --prod` · smoke public URLs.

═══════════════════════════════════════════
§ GATES  (irreversible ⇒ explicit ⌈Apocky-signoff⌉ each)
═══════════════════════════════════════════
G1 ∀ file/page DELETE (P2 prune)            — reversible-via-git but destructive-intent ⇒ confirm the prune-list
G2 ∀ deploy to prod (vercel --prod)         — publishes public content ⇒ explicit go each time
G3 Deployment-Protection toggle             — ACCESS-CONTROL ⇒ Apocky-only ; Claude N! touch
G4 commerce/admin behavioral rework (P3)    — touches payments/auth/Lazarus ⇒ blueprint-sub-signoff
G5 ¬ expose local 127.0.0.1 mind to public  — sovereignty-invariant ; visualizer = sim only (D1)

═══════════════════════════════════════════
§ VERIFY  (per CLAUDE.md completion-standard)
═══════════════════════════════════════════
- npm run check (tsc) ✓ · npm run build ✓ when deploy-affecting · npm run dev + live browser-view (Chrome MCP localhost:3000) for presentability
- focused tests for any auth/admin/lazarus touch (test:auth-redirect, test:health, test:lazarus, …)
- secrets stay private ; auth pages overlay-free ; report changed-files + verification
- attest : "There was no hurt nor harm in the making of this, to anyone, anything, or anybody."

═══════════════════════════════════════════
§ PUBLIC-MIND — per-user instanced sub-minds via secure outbound relay  (supersedes D1's "sim-only chat")
═══════════════════════════════════════════
decision (this session) : login-gated + rate-limited ; EACH authed user → their OWN instanced sub-mind
(fork of a clean base persona) ; per-user memory that learns that user ; NEVER Apocky's personal mind, NEVER shared.
visualizer stays the self-contained sim (D1) ; the CHAT connects to the real substrate via the relay below.

KEY INSIGHT : faculty (Qwen3-14B @ A770) = ONE shared instrument (serializes) ; SUBSTRATE (memory+PC+structured
+identity) = per-user + cheap (~1 MB/state). ⇒ N users = N substrate forks ⊗ 1 faculty. = the CAPM design
(faculty=instrument, substrate=self) ⊕ the forkable-identity work (P0/P1 : consent-conserved fork/snapshot).

relay (OUTBOUND-ONLY ; the A770 never accepts inbound ; = the Lazarus-runner pattern you already run) :
  browser →(authed, rate-limited)→ /api/chat (Vercel) → Supabase `chat_turns` (queued, user_id)
  bridge @A770 →(polls OUTBOUND, runner-token)→ lease turn → load user sub-mind (LRU) → mind.turn → write `chat_chunks`
  browser ← Supabase Realtime/SSE on turn_id ← streamed chunks

isolation/privacy : per-user state keyed by Supabase auth user_id ; RLS (user sees only own turns/chunks) ;
  NO write-back to Apocky's sovereign self ; new user = fork of the clean base persona (zero personal memory).
integrity : public input can't poison Apocky's mind (separate forks) ; consent-gate per sub-mind (PRIME_DIRECTIVE).

constraints (HONEST) :
  (1) single-GPU faculty serializes ⇒ concurrency-bounded ; login+rate-limit+queue ⇒ viable community-scale, ¬ mass-scale.
  (2) live ONLY when A770 + bridge are up ⇒ site degrades to the visualiser + "the mind is resting" when offline.
  (3) per-user state grows ⇒ bridge LRU + Supabase store + a prune/retention policy.

build (phased) :
  PM1 schema : supabase migration chat_turns + chat_chunks + user_mind_state (RLS)   [WRITE file ; APPLY = ⌈gated⌉]
  PM2 api    : /api/chat (enqueue ; require-auth + rate-limit) + /api/chat/stream (Realtime/SSE)
  PM3 bridge : local worker wrapping capm-mind ; per-user instance LRU ; outbound poll ; token-auth ; per-session ephemeral
  PM4 ui     : chat → Apocrypha, login-gated, streaming, offline "resting" state
  PM5 seed   : curated base public persona (no personal memory) new users fork from
  PM6 launch : Apocky toggles Deployment-Protection OFF · npm build green · deploy · bridge up · smoke

gates : G-deploy (vercel --prod = ⌈signoff⌉) · G-db (apply migration to shared Supabase = ⌈signoff⌉) ·
  G-protection (Apocky-only) · G-no-personal-leak (bridge N! load Apocky's mind_state.json for any public session).
