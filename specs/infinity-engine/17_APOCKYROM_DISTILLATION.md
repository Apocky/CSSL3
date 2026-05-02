# § 17 · ApockyROM Distillation for The-Infinity-Engine

§ META
  scope     : ApockyROM-repo @ `C:\Users\Apocky\source\repos\ApockyROM\`
  date      : 2026-05-01
  agent     : W14-F · ApockyROM-spelunker
  parent    : Infinity-Engine R&D
  status    : extracted ◐ — ApockyROM = OS-product · Infinity-Engine = game-OS-fabric · structural-parallels harvestable

---

## § 1 · Core-thesis · what-is-ApockyROM

  t∞: ApockyROM = bespoke-Android-OS for-OnePlus-9-Pro-5G (LE2125 "lemonadep")
  t∞: ¬community-ROM ← single-device · single-philosophy
  t∞: base = LineageOS-23.2 (Android-16)
  t∞: hardware = SM8350 Lahaina (Snapdragon-888 · Adreno-660 · Samsung-E4-LTPO)
  t∞: privacy-stance = MicroG default · ¬GMS

  § PHILOSOPHY
    zero-bloat       : every-package W! justified-presence
    consent-first    : ¬silent-telemetry · ¬background-data · permissions-surfaced
    OxygenOS-classic : visual-language ← old-OOS ¬ ColorOS-adjacent-OOS-14
    hardware-honest  : SD888-throttles · Hasselblad=proprietary · document+work-with ¬ around-with-lies
    full-source      : ¬ pre-built+Magisk · source-built → custom-SystemServices+kernel-mods

  § STACK
    layer₅ : User-Applications
    layer₄ : ApockyROM-Custom-Layer ⌈branding+thermal-policy+display-tuning+camera-overlays+consent-framework⌉
    layer₃ : LineageOS-23.2-Framework ⌈AOSP+LineageOS-patches+MicroG⌉
    layer₂ : Qualcomm-Vendor-HALs ⌈camera.qcom+audio.primary.lahaina+sensors+fingerprint.goodix · preserved-from-OOS⌉
    layer₁ : Linux-5.4-msm-5.4-QGKI-Kernel ⌈OnePlusOSS-fork+lahaina-qgki_defconfig⌉
    layer₀ : Qualcomm-Firmware ⌈modem+ADSP+CDSP+SLPI+TZ · untouched⌉

---

## § 2 · Key-primitives · ApockyROM

### § 2.1 · Three-zone-thermal-policy
  ⟨Gaming/Media/Idle⟩ ← single-tile-toggle ← `ApockyThermalService`
  Gaming : skin≤45°C · FPS-uncapped · touch=360Hz · Prime=2.84GHz · charge-limit=2200mA
  Media  : skin≤42°C · FPS=60 · touch=240Hz · Prime=parked
  Idle   : skin≤38°C · FPS=60 · touch=120Hz · Silver-only
  surface : `thermal-engine.conf` overlay + sysfs-writes + cgroup-cpuset
  setting : `Settings.System.APOCKY_THERMAL_ZONE` persistent
  broadcast : `ACTION_THERMAL_ZONE_CHANGED`

### § 2.2 · Vendor-blob-preservation-policy
  W! preserve-vendor-partition-completely ← ∵ camera+sensor+display-calibration co-resident
  N! reformat-vendor ← ∵ losing-Hasselblad-LUTs simultaneously-loses sensor-fusion+color-calibration
  primary-source : OOS-14.0.0.1901 (EX01)
  read-only : vendor-blobs · tuning-via `camxoverridesettings.txt` text-config ¬ binary-patch

### § 2.3 · Pixelworks-X5-Pro display-stack
  pipeline : SoC-DSI(4-lane) → Pixelworks-X5-Pro(2-lane-passthrough+processing) → S6E3HC3-DDIC → Samsung-E4-LTPO-AMOLED
  features : MEMC + HDR-upscaling + Comfort-Tone + 8192-brightness-gradients
  LTPO     : 1Hz-AOD ↔ 24Hz-video ↔ 60Hz-UI ↔ 120Hz-game · ADFR sync GPU-render-completion
  controls : `set_idle_timer_ms=250` + `set_touch_timer_ms=300` + `use_content_detection_for_refresh_rate=true`

### § 2.4 · Hasselblad-CHI-camera-pipeline
  framework : CamX (Qualcomm-generic) + CHI (device-pipeline-graphs)
  override  : `com.qti.chi.override.so` ← Hasselblad-color-nodes wired-in-here
  components: ~30 CHI-nodes ⌈eisv3+remosaic+swregistration+mfnrblend+gpu+hvx+depth+dewarp+stiphdr⌉
  tuning    : 3D-LUTs + custom-S-curve + WB-calibration ← all-software ¬ hardware

### § 2.5 · Phased-roadmap A→G
  A : LineageOS-baseline-builds-clean
  B : ApockyROM-branding+package-manifest
  C : Custom-thermal-policy
  D : Camera-pipeline-optimization
  E : Display-tuning
  F : Custom-kernel-tweaks
  G : OTA-infrastructure
  ¬goal : AI-system-service+on-device-MCP+eBPF-governor+consent-as-OS-framework ← deferred-Phase-H+

### § 2.6 · MicroG-substitution
  ¬GMS · ¬Google-Play-Services
  GmsCore : `com.google.android.gms` stub w/ fake-signature
  Vending : `com.android.vending` stub
  push    : unified-push via GmsCore
  location: Mozilla/Apple/custom-backends ¬ Google
  signature-spoofing : enabled-via-LineageOS-patcher | framework-patch

### § 2.7 · Vendor-overlay-system `vendor/apocky/`
  layer-mechanism : Android-overlay + product-makefile-system
  contents : branding + package-manifest + thermal-engine.conf + camxoverridesettings.txt + display-properties + kernel-tweaks + OTA-config
  principle : minimize-framework-patches ← maintenance-burden ∝ patch-count
  rule      : every-`vendor/apocky/` addition W! comment-explaining-why ← silence ⇒ forgot

---

## § 3 · Reusable-pieces for-Infinity-Engine

### § 3.1 · Three-zone-thermal-policy → Engine-runtime-mode-switch
  pattern   : single-tile-toggle ⇒ multi-axis-config-flip
  parallel  : Gaming/Media/Idle ↔ Engine-modes ⌈Combat/Exploration/Hub⌉
  reuse     : per-mode-allocation-of FPS+touch-rate+CPU-cluster+GPU-floor+charging-current
  insight   : ¬ throttle-by-default-only ← ALSO throttle-by-mode · user-intent-aware
  to-IE     : Infinity-Engine W! similar mode-broadcast (`MODE_CHANGED`) · per-mode resource-budgets
  applies-to: KAN-eval-budget · LLM-inference-token-budget · procgen-asset-fetch-bandwidth

### § 3.2 · Vendor-preservation > vendor-replacement
  pattern   : when-vendor-pipeline-is-proprietary+excellent → preserve+overlay ¬ rewrite
  parallel  : Hasselblad-CHI ↔ proprietary-LLM-providers (OpenAI · Anthropic · etc)
  reuse     : Engine W! treat external-providers as preserved-vendor-HALs
              ⊗ thin-overlay-layer (mycelium-LLM-bridge)
              ⊗ text-config-tuning (¬ binary-patching)
              ⊗ documented-fallback (GCam ↔ stage-0-self-sufficient)
  insight   : full-source ≠ from-scratch-everywhere · accept-proprietary-where-quality-demands

### § 3.3 · `camxoverridesettings.txt` pattern → Engine-runtime-config
  pattern   : single-text-key-value-file gates-all-meaningful-behavior @ runtime-load
  parallel  : Engine W! `engine_override_settings.csl` for-runtime-tuning
  candidates: KAN-grid-resolution · agent-loop-tick-rate · procgen-budget · cache-eviction-aggression
  insight   : text-only ¬ binary ← preserves-modifiability + survives-restarts + greppable

### § 3.4 · Phased-roadmap A→G discipline
  pattern   : 7-phases · sequential · each-phase ⊗ ⟨deliverable + DoD + risks + duration⟩
  parallel  : Infinity-Engine W! similar-phase-discipline ⌈substrate→procgen→LLM-bridge→agent-loop→hot-reload→OTA⌉
  insight   : phase-A = "baseline-builds-clean" ← simplest-possible-thing-first
              phase-G = "OTA" ← distribution-last
              middle-phases = quality-improvements

### § 3.5 · Consent-first + transparency baked-into-OS
  pattern   : ¬silent-telemetry · ¬background-data · permissions-surfaced
  parallel  : PRIME-DIRECTIVE §4 TRANSPARENCY + §5 CONSENT-ARCHITECTURE
  reuse     : Infinity-Engine inherits-this-by-default ← already PRIME-DIRECTIVE-bound
  to-IE     : when-engine-fetches-online-assets · when-LLM-bridge-uses-cloud-provider →
              user W! see-it · know-cost · revoke-anytime
  pattern-name : "consent-aware-runtime"

### § 3.6 · QGKI-vendor-module-pattern → Infinity-Engine-plugin-pattern
  pattern   : core-image generic + vendor-modules in /vendor/lib/modules/
  reuse     : Engine W! similar plugin-architecture ← core-engine generic · per-game-modules in `engine/plugins/`
  insight   : updates-decoupled · core-can-update-independently-of-plugins (with-version-match)

### § 3.7 · OTA-infrastructure pattern
  pattern   : signed-ZIP + JSON-manifest + LineageOS-Updater pointed-to-our-server
  parallel  : Engine-content-pack-OTA ↔ ApockyROM-firmware-OTA
  reuse     : per-game LoA-content-packs flow-via signed-bundle + manifest
  signing   : production-signing-keys secured · `sign_target_files_apks.py`
  to-IE     : substrate-Σ-Chain already-provides cryptographic-verification ← stronger-than-AOSP-signing

### § 3.8 · Hardware-honesty principle
  pattern   : "hardware-honest · document-realities · work-with ¬ around-with-lies"
  parallel  : Engine W! be-honest-about ⌈token-cost · inference-latency · cache-hit-rate · procgen-quality⌉
  to-IE     : surface-stats UI · expose ⌈KAN-eval-time · LLM-tokens-burned · asset-fetch-bandwidth⌉
  anti-pattern : marketing-claims-vs-reality (e.g. "10-bit HDR video" not-actually-supported)

### § 3.9 · Old/-archive-discipline
  observation : repo-has `Old/` archived-previous-session-files ← prior-iteration kept-but-segregated
  reuse       : Infinity-Engine specs/ W! similar deprecated/ ← old-iterations-kept-traceable
  insight     : delete = lossy · archive = preserves-context for-future-spelunking

### § 3.10 · Quickstart 4-command pattern
  observation : `make setup` → `make sync` → `make build` → `make flash` ← entire-onboarding-in-4-commands
  reuse       : Infinity-Engine W! similar 4-command-bootstrap ⌈init+sync+build+run⌉
  insight     : friction-reduction = adoption-multiplier · canonical-Make-targets ¬ scattered-scripts

---

## § 4 · Anti-patterns to-avoid · learned-from-ApockyROM

  N! reformat-vendor-partition ← simultaneously-destroys camera+sensor+display-calibration
  N! disable `com.oplus.battery` w/o replacement ← removes-real-safety-features (overtemp-shutdown)
  N! patch `frameworks/base` casually ← maintenance-burden across-version-upgrades
  N! ship-feature w/o `vendor/apocky/` comment-explaining-why ← silence ⇒ forgot
  N! "feature-list designed-for-XDA-votes" ← optimization-target wrong
  N! use-aggressive-throttling-as-only-thermal-strategy ← user-intent-aware multi-zone wins

---

## § 5 · Files-spelunked

  `C:\Users\Apocky\source\repos\ApockyROM\README.md` (143 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\PRIME_DIRECTIVE.md` (424 LOC · same-canonical-doc)
  `C:\Users\Apocky\source\repos\ApockyROM\Makefile` (74 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\README.md` (73 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\architecture.md` (185 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\thermal_policy.md` (206 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\display_tuning.md` (256 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\camera_pipeline.md` (258 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\roadmap.md` (205 LOC)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\hardware_bible.md` (head-100-of-larger-file)
  `C:\Users\Apocky\source\repos\ApockyROM\docs\blob_manifest.md` (head-80-of-larger-file)
  unread : `flashing_guide.md` · `build_guide.md` ← procedural-only · low Infinity-Engine-relevance

---

## § 6 · Bottom-line · paragraph-summary

ApockyROM is Apocky's bespoke single-device Android OS — a LineageOS-23.2 (Android-16) fork tuned for the OnePlus-9-Pro-5G (lemonadep), with a custom overlay layer (`vendor/apocky/`) that adds a three-zone thermal policy, Pixelworks-X5-Pro display tuning, Hasselblad-CHI camera-pipeline preservation via `camxoverridesettings.txt`, and MicroG-substitution for de-Googled Play-compat. The repo's core thesis — "preserve proprietary excellence where quality demands, but layer transparent consent-first overlay above" — translates cleanly to The-Infinity-Engine's design space: external LLM providers and online procgen-assets are the "vendor BLOBs" of an AI-game OS, and the right answer is a thin mycelium-bridge overlay (¬ rewrite-everything), runtime text-config (¬ binary-patch), phased roadmap (A→G discipline), mode-aware resource budgets (Gaming/Media/Idle ↔ Combat/Exploration/Hub), and hardware-honest stats-surfacing. Most reusable specific patterns: `camxoverridesettings.txt`-style runtime-config, `Settings.System.APOCKY_THERMAL_ZONE`-style mode-broadcast, QGKI vendor-module decoupling for plugin-architecture, 4-command quickstart for adoption, and `Old/`-archive discipline for traceable iteration history. Anti-patterns to avoid: monolithic vendor-rewrites, casual framework patches, undocumented overlay additions, single-axis throttling.

∎
