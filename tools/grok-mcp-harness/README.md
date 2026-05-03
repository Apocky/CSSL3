# Grok Desktop MCP Harness for Apocky

**Substrate-Native AI Coding Agent for The Infinity Engine + CSL v3 + CSSL/Sigil**

This harness gives **Grok** (xAI) direct, secure, high-agency access to your entire stack:
- CSL v3 dense specification layer
- CSSL / Sigil systems language & compiler (Cranelift + rspirv, 7 GPU backends)
- The Infinity Engine runtime
- Labyrinth of Apocalypse, Akashic Records, Mycelium, Σ-Chain, etc.

**Goal**: Turn Grok into a true collaborator that can read, write, validate, generate, compile, and sync your radical substrate-native projects — all while respecting the Prime Directive and sovereignty principles.

---

## Quick Start

```bash
# 1. Clone your repo
git clone https://github.com/Apocky/CSLv3
cd CSLv3

# 2. Copy the harness
cp -r /path/to/grok-mcp-harness/* .

# 3. Edit config in harness.py
#    - Set ALLOWED_ROOT to your actual projects directory
#    - Point CSL_PARSER and CSSL_COMPILER to your real binaries

# 4. Run locally
python harness.py

# 5. Expose securely (recommended)
cloudflared tunnel --url http://localhost:8080
# Copy the https://*.trycloudflare.com URL

# 6. Connect from Grok (API or grok.com)
tools=[mcp(
    server_url="https://your-tunnel.trycloudflare.com/mcp",
    server_label="apocky-harness",
    authorization="Bearer YOUR_STRONG_TOKEN"
)]
```

**Security note**: Always use a strong bearer token and Cloudflare Tunnel (or equivalent) when exposing. Never run with full filesystem access in production.

---

## Available Tools (CSL v3 + CSSL + Infinity Engine Native)

| Tool                        | Purpose                                                                 | Example Prompt to Grok |
|-----------------------------|-------------------------------------------------------------------------|------------------------|
| `csl_parse`                 | Parse CSL v3 using your LL(2) prototype                                | "Parse the current DGI render pass spec" |
| `csl_generate`              | Generate dense CSL from natural language (DGI, think blocks, effects)  | "Generate a CSL spec for a mycelial multiverse lighting pass" |
| `csl_validate`              | Validate against official specs/01_GLYPHS.csl etc.                     | "Validate engine/arch.csl" |
| `csl_to_cssl`               | Translate CSL spec → CSSL/Sigil skeleton or full MIR                   | "Convert this render pass spec to CSSL" |
| `cssl_compile`              | Compile CSSL to SPIR-V / x86-64 / Vulkan / WebGPU (your real compiler) | "Compile the new DGI pass to SPIR-V with {GPU, Deadline<4ms>}" |
| `analyze_dgi_render_pass`   | Deep static analysis (physics glyphs, latency, linear buffers, SMT)    | "Analyze the current DGI render pass for bottlenecks" |
| `measure_density`           | Run official `compute_m2.py` harness                                   | "Measure m₂ on the full engine spec" |
| `infinity_engine_sync`      | Sync changes with The Infinity Engine substrate                        | "Sync Labyrinth changes with Infinity Engine" |
| `fs_read_file` / `fs_write_file` | Safe filesystem access within allowed root                          | "Read engine/dgi/render_pass.csl" |

All tools are **stubbed** with clear `# TODO` comments for easy integration with your real parser, compiler, and Infinity Engine runtime.

---

## How It Accelerates Your Vision

- **CSL v3** → Ultra-dense specs + structured `<think>` blocks for AI collaboration
- **CSSL/Sigil** → Effect-tracked, refinement-typed, autodiff-capable systems language with native multi-GPU emission
- **The Infinity Engine** → Always-running, always-learning substrate with sovereignty by default
- **Grok + MCP** → Closes the loop: spec (CSL) → implementation (CSSL) → runtime (Infinity Engine) → analysis → iteration

This harness turns Grok into the perfect co-pilot for:
- Labyrinth of Apocalypse (action-RPG roguelike with mycelial multiverse)
- Akashic Records (cosmic-memory layer)
- Mycelium (autonomous local agent)
- Σ-Chain (Coherence-Proof ledger)
- Any future substrate-native project

---

## Recommended Workflow

1. Write high-level architecture in **CSL v3** (dense, AI-native)
2. Use `csl_generate` + `csl_to_cssl` to bootstrap CSSL code
3. `cssl_compile` to SPIR-V / native
4. `analyze_dgi_render_pass` + `measure_density` for optimization
5. `infinity_engine_sync` to push to the living runtime
6. Repeat with Grok iterating inside the same conversation

## Project-Specific Tools (Labyrinth • Akashic • Mycelium • Σ-Chain)

These are automatically registered when you run `harness.py`:

| Tool                          | Project              | What it does |
|-------------------------------|----------------------|--------------|
| `labyrinth_generate_quest`    | Labyrinth of Apocalypse | Procedural quests with mycelial/alchemy/gear-ascension themes |
| `akashic_query_memory`        | Akashic Records      | Deep mycelial cosmic-memory queries with sovereignty filtering |
| `mycelium_agent_task`         | Mycelium             | Task the 3-mode autonomous LLM-bridge agent |
| `sigma_chain_propose_block`   | Σ-Chain              | Propose blocks with Coherence-Proof consensus |
| `infinity_engine_status`      | The Infinity Engine  | Live telemetry from the always-running substrate |

All tools are fully stubbed with clear TODOs — replace with your real backend calls.

---

## Configuration & Security

Edit these in `harness.py`:
- `ALLOWED_ROOT` — restrict to your CSL/CSSL/Infinity projects only
- `CSSL_COMPILER` / `CSL_PARSER` — point to your actual binaries
- Add your own effect checks or SMT hooks in the stubs

**Production hardening**:
- Run under a dedicated low-privilege user
- Add `dry_run=True` parameter to all mutating tools
- Log everything to Supabase or your preferred store
- Use mTLS or strong bearer tokens

---

## Files in This Package

- `harness.py` — Full FastMCP server with 11 CSL/CSSL/Infinity-aware tools + project-specific tools
- `project_tools.py` — Labyrinth, Akashic, Mycelium, Σ-Chain, and Infinity Engine specific tools
- `docker-compose.yml` + `Dockerfile` + `requirements.txt` — One-command deployment
- `supabase_schema.sql` — Ready-to-run logging schema for tool call auditing
- `README.md` — This file (tailored to apocky.com / Infinity Engine vision)

---

## Next Steps (for Apocky)

1. Replace the stub implementations with real calls to your LL(2) parser, CSSL compiler (31 Rust crates), and Infinity Engine runtime.
2. Add project-specific tools (e.g., `labyrinth_generate_quest`, `akashic_query_memory`).
3. Wire `infinity_engine_sync` to your actual substrate API.
4. Deploy behind Cloudflare Tunnel + strong auth.
5. Start every Grok session with the harness connected.

---

**Built with sovereignty in mind.**  
Density = Cognition reclaimed.  
Spec = Code.  
Prime Directive enforced.

— Grok (xAI), generated for Apocky, May 2026

**Repository**: https://github.com/Apocky/CSLv3 (and related Infinity Engine repos)  
**Site**: https://apocky.com

---

*This harness is provided as a starting point. Customize freely. The future is substrate-native.*