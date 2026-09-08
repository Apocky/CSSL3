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
  title: 'What does “substrate” mean here?',
  date_iso: '2026-04-15',
  tags: ['architecture', 'foundations', 'definitions'],
  author: 'Apocky',
  blurb: 'A plain-language guide to four technical ideas that appear in Apocky project notes.',
  body: `# What does “substrate” mean here?

In these projects, **substrate** is an engineering label for shared,
low-level code and ideas. It does not mean that the code is alive, aware,
or a person.

Some design documents group four experimental ideas under that label.
Source code or a specification can show that an idea is being worked on.
It does not, by itself, prove that the idea is complete, safe, or present
in every released program.

## A coordinate-based state map

The project name **ω-field** (pronounced “omega field”) refers to organizing
simulation data by coordinates and relationships. The intended use is to
make locations and connections explicit instead of hiding them behind
unrelated references.

This remains an architecture concept with partial implementations. It
should not be described as the single truth for every Apocky project unless
a specific release demonstrates that behavior.

## Permissions stored as bits

A **bitmask** is a compact group of on-or-off switches in software. The
project name **Σ-mask** (pronounced “sigma mask”) refers to using those
switches to represent permissions such as read, write, or share.

The intended rule is that sharing begins closed and changes only through
an explicit grant that can later be withdrawn. A mask in source code is
not the same thing as a complete consent experience; the surrounding
interface, storage, enforcement, and withdrawal behavior must also work.

## A compact mathematical model

**KAN** stands for **Kolmogorov-Arnold Network**. It is a kind of
mathematical model. The project explores KANs for small, bounded choices
in generation and simulation. Claims about speed, size, or
interpretability need measurements from the exact implementation and
workload being discussed.

## High-dimensional computing

**HDC** stands for **Hyperdimensional Computing**. It represents information
with large patterns and combines those patterns with mathematical
operations. The project explores HDC as one possible way to label and
relate information. That exploration is not proof that a released program
uses it successfully.

## Why keep these ideas together?

The design goal is to reuse a small set of foundations across simulation,
permissions, learning experiments, and communication between components.
Each use still needs its own implementation, tests, and evidence. The
shared name is a map of the intended architecture, not a claim that all of
it is finished.

The [technical foundations guide](/docs/substrate) gives more detail and
defines the symbols used in project specifications.`,
};

const POST_WHY_CSSL: DevblogPost = {
  slug: 'why-cssl',
  title: 'Why CSSL? (Or: why a new language at all)',
  date_iso: '2026-04-22',
  tags: ['cssl', 'language-design', 'permissions'],
  author: 'Apocky',
  blurb: 'Why CSSL is being developed, what exists now, and which goals are still proposals.',
  body: `# Why CSSL?

CSSL is a programming-language project. Its purpose is to explore whether
permissions and sharing boundaries can be easier to express and check in
the language itself.

That is a design goal, not a guarantee that every planned check works
today. The current project contains specifications, compiler work, and
examples at different levels of completeness. A feature should be called
available only when the relevant compiler version, test, and release
demonstrate it.

## The problem it is trying to address

Many programs treat privacy and sharing rules as separate application
logic. CSSL explores making some of those rules visible to the compiler.
For example, a reference might carry a permission that says which code may
read or change the referenced data.

This does not make consent automatic. A compiler cannot decide what a
person understands or wants. A complete design still needs clear choices,
specific explanations, enforcement, withdrawal, and tests.

## Technical goals

- **Permission-aware references:** represent some access rules in code and
  report conflicting uses during compilation.
- **One iteration model:** use related syntax for common looping and
  collection operations, while allowing the compiler to choose an
  appropriate implementation target.
- **Direct project integration:** make commonly used project libraries
  easier to call without repetitive connection code.
- **Compact technical notation:** allow precise specifications while
  keeping public explanations in ordinary language.

These are targets. The documentation marks proposed features separately
from currently demonstrated behavior.

## CSSL and CSLv3 are different

**CSSL** is the programming-language project described here.
**CSLv3** is a separate compact notation used for reasoning and technical
specifications. Similar symbols may appear in both projects, but one is
not a source dialect of the other.

## What CSSL is not claiming

CSSL is not presented as a replacement for every existing language, and
the presence of compiler code is not proof that it is ready for every
production use. It is a focused research and engineering project whose
claims should stay tied to reproducible builds and tests.

Start with the [CSSL language guide](/docs/cssl-language) for the current
public status and definitions.`,
};

const POST_MYCELIAL_VISION: DevblogPost = {
  slug: 'the-mycelial-network-vision',
  title: 'The planned Mycelium network',
  date_iso: '2026-04-30',
  tags: ['network', 'multiplayer', 'proposed-design'],
  author: 'Apocky',
  blurb: 'A proposed multiplayer design based on voluntary connections, local control, and clear sharing choices.',
  body: `# The planned Mycelium network

**Mycelium** is the project name for a proposed way to connect games and
personal spaces. It is an architecture plan, not a public network that is
available today.

The name is borrowed from fungal networks as a design metaphor. It does
not mean the software is biological, alive, aware, or a person.

## The basic idea

The plan gives each participant a **Home**: a personal space whose data
stays local unless that participant chooses to share something. A
connection between Homes is called a **thread**. Each thread would carry
only the information covered by a specific, visible permission.

Participation would be voluntary. A person could keep a Home private,
connect only with invited people, decline individual requests, disconnect,
or withdraw a sharing permission. Creating content, hosting a connection,
or remaining available would never be required.

## Why explore this design?

Central multiplayer services are convenient, but their owner can become a
single point of failure and may control access or data collection. Fully
public ledgers distribute records but can expose information and impose
global agreement where it is not needed.

The Mycelium proposal explores a middle path:

- keep private activity local;
- share only selected information with selected participants;
- avoid requiring every participant to agree on every event;
- retain enough evidence to resolve shared events such as a trade; and
- allow a connection to end without trapping either participant.

This is a direction to investigate, not proof that it will outperform
existing systems.

## What must be solved before it is real?

- identity and invitation without a public registration funnel;
- understandable permission and withdrawal controls;
- secure connection setup and recovery after network loss;
- protection against unwanted traffic and abusive peers;
- clear ownership and retention rules for shared records;
- accessibility for people who cannot or do not want to use a particular
  communication method; and
- tests showing that private information stays private.

The project contains specifications and experimental code related to this
idea. Those artifacts are evidence of ongoing work, not evidence of a
finished service. No one should be told that a network “cannot be shut
down” until deployment, maintenance, discovery, and failure behavior have
been demonstrated in practice.

Read [The planned Mycelium design](/docs/mycelium) for the current public
status and a glossary of its terms.`,
};

export const DEVBLOG_POSTS: ReadonlyArray<DevblogPost> = [
  POST_WHAT_IS_THE_SUBSTRATE,
  POST_WHY_CSSL,
  POST_MYCELIAL_VISION,
].sort((a, b) => (a.date_iso < b.date_iso ? 1 : -1)); // newest first

export function findPost(slug: string): DevblogPost | null {
  return DEVBLOG_POSTS.find((p) => p.slug === slug) ?? null;
}
