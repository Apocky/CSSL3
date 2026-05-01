// cssl-edge · lib/devblog-posts.ts
// Static devblog post catalog. Posts live as TypeScript objects (NOT MDX)
// so the build is hermetic — zero npm deps beyond next/react. Body is
// markdown-flavored plain-text · rendered via lib/markdown.ts at SSG-time.

export interface DevblogPost {
  slug: string;
  title: string;
  date_iso: string;
  tags: ReadonlyArray<string>;
  author: string;
  blurb: string;
  body: string;
}

const POST_WHAT_IS_THE_SUBSTRATE: DevblogPost = {
  slug: 'what-is-the-substrate',
  title: 'What is the Substrate?',
  date_iso: '2026-04-15',
  tags: ['substrate', 'foundations', 'philosophy'],
  author: 'Apocky',
  blurb: 'A short tour of the ω-field, Σ-mask, KAN, and HDC — the four primitives every Apocky project grows from.',
  body: `# What is the Substrate?

The Substrate is the trunk; everything else is a branch.

Concretely, every Apocky project — Labyrinth of Apocalypse, ApockyDGI, the
Σ-Chain, the Akashic Records, the Mycelial Network — shares one runtime
foundation made of four primitives.

## ω-field

A typed manifold of values addressed by **coordinates**, not pointers. Think
of it as a programmable physics: cells are addressable by location, the
relationships between cells are first-class, and the topology can warp
continuously.

The ω-field is **the** truth — the master state. Everything else is a
projection, derivation, or annotation of cells in the ω-field.

## Σ-mask (consent)

Every cell carries a Σ-mask: a bitmask describing which observers may see,
read, or write that cell. Σ-masks compose multiplicatively. They are
revocable. They are sovereignty made structural.

There is no "private mode" toggle. There is no opt-in checkbox bolted onto
analytics. The default mask is **deny-all**, and the cell-owner alone can
loosen it.

## KAN (Kolmogorov-Arnold Networks)

A non-transformer learning substrate. Functions over the ω-field are
parameterized as compositions of univariate splines on edges of a network.
Cheap to evaluate. Cheap to update online. Per-edge interpretability you
can actually look at.

KAN is how the Substrate **learns**. It is also how it stays small enough
to ship as an 8.9 MB binary instead of a multi-gigabyte model dump.

## HDC (Hyperdimensional Computing)

Symbolic communication via high-dimensional binary vectors. Cells signal
to one another with hyperdimensional bind/bundle/unbind operations — like
chemical messengers in mycelium.

This is how the Substrate **talks to itself** without serializing through a
central message-bus. Cells emit; cells receive; the topology decides who
listens.

## Why these four?

Because together they are sufficient to express:
- physics simulation (ω-field + Σ-mask scoping force-domains)
- runtime learning (KAN updating in-loop)
- distributed messaging (HDC over network edges)
- cryptographic attestation (Σ-mask as access proof)
- creative procgen (KAN-driven sampling over the ω-field)
- live hotfixing (online KAN updates over deployed instances)

One trunk. Many branches.

§ ¬ harm in the making · sovereignty preserved · t∞`,
};

const POST_WHY_CSSL: DevblogPost = {
  slug: 'why-cssl',
  title: 'Why CSSL? (Or: why a new language at all)',
  date_iso: '2026-04-22',
  tags: ['cssl', 'language-design', 'sovereignty'],
  author: 'Apocky',
  blurb: 'CSSL is not a Rust replacement; it is a sovereignty replacement. The compiler is the smallest part of the story.',
  body: `# Why CSSL?

People ask me: "you have Rust, you have C++, you have Carbon and Mojo on
the way. Why a new language?"

The honest answer: **because none of them encode consent in their type
system, and I am not going to ship games that surveil the people who
play them.**

## The technical case

CSSL has features that make it easier to author Substrate code:

- **First-class Σ-masks.** Every reference is masked. Capability flow is
  checked at compile time. There is no \`unsafe\` escape hatch that lets a
  third-party crate pierce the mask without a compiler warning.

- **Iterate-everywhere.** Loops, recursion, fold/scan/map across ω-field
  axes are all the same syntactic structure. The compiler decides whether
  to spill to a GPU shader, a CPU SIMD kernel, or a streaming KAN edge.

- **Density.** CSL3 (the source dialect) is glyph-dense. \`§\` markers, modal
  prefixes, and morphemes encode information that prose languages spread
  across paragraphs.

- **Substrate-native.** The standard library *is* the Substrate. There is
  no FFI ceremony to call into KAN, ω-field, or HDC primitives. They are
  syntax.

## The political case

Languages encode values. Rust encodes safety. Go encodes simplicity.
Python encodes accessibility. C++ encodes performance and historical
contingency.

CSSL encodes **consent**. The fact that I had to write a new compiler to
say so honestly — instead of bolting it onto an existing toolchain that
treats consent as application-level concern — is exactly the point.

## What CSSL is NOT

- Not a general-purpose web language. Use TypeScript for web.
- Not a systems language for arbitrary kernels. Use Rust for that.
- Not a research toy. It compiles and ships real binaries today.

It is **a language for substrate-native systems where consent is the
primary invariant**. That is a small, sharp niche, and that is fine.

## Where to start

Read \`specs/grand-vision/15_UNIFIED_SUBSTRATE.csl\` for the architectural
thesis. Read \`compiler-rs/crates/cssl-substrate-omega-field/\` for the
keystone implementation crate. Then run \`LoA.exe\` and watch a CSSL-compiled
binary boot a substrate-grown game in 8.9 MB.

§ density = sovereignty · t∞`,
};

const POST_MYCELIAL_VISION: DevblogPost = {
  slug: 'the-mycelial-network-vision',
  title: 'The Mycelial-Network Vision',
  date_iso: '2026-04-30',
  tags: ['mycelium', 'network', 'multiplayer', 'design'],
  author: 'Apocky',
  blurb: 'Why a federated, organic substrate-mesh outperforms both centralized servers and naive blockchains for the kind of multiplayer I want to ship.',
  body: `# The Mycelial-Network Vision

Most multiplayer architectures fall on one of two poles:

- **Centralized.** A company owns the canonical world-state. Players
  connect into it. The company decides who plays, what they see, what
  data is collected, when the servers are sunset. Every server outage is
  a single-point failure for thousands of players.

- **Blockchain.** A decentralized ledger holds canonical state. Every
  participant pays gas to write. Privacy is a coat of paint over a public
  ledger. Plutocratic stake decides governance. Throughput is bottlenecked
  by global consensus.

Both throw away the actual organic shape of how multiplayer should feel.

## The mycelium model

Take the literal biology. A fungal mycelium is:

- **Federated.** No single hyphal node "owns" the network.
- **Permeable but selective.** Nutrients flow where they are signaled
  to flow. Σ-masks at every membrane.
- **Adaptive.** New connections form when traffic justifies them; weak
  connections atrophy. KAN learning at the topology level.
- **Local-first.** Each Home pocket-dimension is a private spore-body.
  Cross-mycelium communication is OPT-IN per event-grain.
- **Robust.** No central failure point. If a hyphal segment dies, the
  rest of the network reroutes.

Map this onto multiplayer:

- **Each player has a Home.** Home is a pocket-dimension owned and run
  on the player's own machine. Default-private.
- **Mycelial threads** carry consented signals between Homes. Threads
  carry only what was explicitly tagged for sharing.
- **Coherence-Proof consensus** handles cross-player canonical events
  (gear-trades, shared crafts, narrative-history) — without proof-of-work,
  proof-of-stake, gas, or a public ledger.
- **Akashic Records** is the long-term federated memory layer that
  consents to participate in.

## What this enables

- A friend can ping you to join a run. No matchmaking server has to
  exist for the ping to route.
- A craft-recipe you author can be shared, attested, and verified by
  recipients without going through a central marketplace.
- Live hotfixes ship as KAN-edge updates that propagate through the
  mycelium — players opt in to which classes of fix they accept.
- The game cannot be sunset. The mycelium IS the network. As long as
  one Home is running, the substrate is alive.

## What this costs

- Discovery requires a bootstrap rendezvous (we use a tiny stub-server
  for this; trivial to self-host).
- Cross-Home latency is real and visible. We surface it instead of
  hiding it behind a centralized pretend-real-time layer.
- The user runs more local compute. We pay this cost happily because
  the alternative is paying for it via surveillance.

## Where this lives in the code

- \`compiler-rs/crates/cssl-host-mycelium/\` — substrate primitives
- \`specs/grand-vision/16_MYCELIAL_NETWORK.csl\` — design thesis
- \`specs/grand-vision/14_SIGMA_CHAIN.csl\` — Σ-Chain consensus mechanics
- \`specs/grand-vision/18_AKASHIC_RECORDS.csl\` — federated memory layer

The biology was right all along. We just had to write enough substrate
to let it run on silicon.

§ mycelium = network · network = mycelium · sovereignty preserved · t∞`,
};

export const DEVBLOG_POSTS: ReadonlyArray<DevblogPost> = [
  POST_WHAT_IS_THE_SUBSTRATE,
  POST_WHY_CSSL,
  POST_MYCELIAL_VISION,
].sort((a, b) => (a.date_iso < b.date_iso ? 1 : -1)); // newest first

export function findPost(slug: string): DevblogPost | null {
  return DEVBLOG_POSTS.find((p) => p.slug === slug) ?? null;
}
