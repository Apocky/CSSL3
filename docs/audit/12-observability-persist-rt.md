# Audit 12 — Observability, Persistence, and Runtime Crates

**Audited:** 2026-05-14  
**Auditor:** Claude Sonnet 4.6 (agent)  
**Files audited:** 12 source files (.rs) + 3 Cargo.toml  
**Crates:** `cssl-telemetry`, `cssl-persist`, `cssl-rt`  
**Spec references:** `specs/22_TELEMETRY.csl` (F6 / R18 observability), `specs/18_ORTHOPERSIST.csl` (R13 orthogonal persistence), `specs/01_BOOTSTRAP.csl`

---

## 1. SLICE OVERVIEW

These three crates cover **feature F6 (Observability)** and its required runtime/storage infrastructure.

**cssl-telemetry** is the primary F6 implementation. It realises the R18 "observability-first-class" mandate from `specs/22_TELEMETRY.csl`: a hardware-aware, multi-domain telemetry pipeline from event capture to export. The crate provides a 26-scope taxonomy covering CPU, GPU, power/thermal, RAS, and app-semantic domains; a single-producer single-consumer ring-buffer for non-blocking event capture; a signed audit chain using real BLAKE3 hashing and Ed25519 signing (upgraded in phase-2a from stubs); and exporter implementations targeting Chrome trace JSON, newline-delimited JSON, and (stubbed) OTLP. Schema metadata for the fat-binary `[telemetry-schema]` section is also housed here. Cryptographic primitives are fully wired in this codebase — blake3 and ed25519-dalek are real dependencies, not placeholder imports.

**cssl-persist** implements orthogonal persistence, the Pharo-lineage R13 requirement from `specs/18_ORTHOPERSIST.csl`: objects survive process death transparently. The crate provides schema versioning with major/minor monotonic identifiers and a stub digest, a migration-chain abstraction for linking ordered schema transitions, an in-memory image model (header + typed records), and a `PersistenceBackend` trait with an `InMemoryBackend` reference implementation. WAL and LMDB backends are explicitly deferred to phase-2.

**cssl-rt** is the runtime library, linked into every CSSLv3 artifact. At present it is a minimal scaffold — a single `STAGE0_SCAFFOLD` constant and one trivial test. No allocator hooks, no telemetry plumbing, no orthopersist image API are present yet. The doc-comment describes its intended role accurately but the actual code is essentially empty.

**Relative maturity:** `cssl-telemetry` is the most complete crate in this slice — real crypto is wired, the ring-buffer semantics match the spec, and test coverage is solid. `cssl-persist` is a well-structured scaffold with correct data-model wiring but no durable backend. `cssl-rt` is a placeholder.

---

## 2. CRATE SUMMARIES

### 2.1 cssl-telemetry

| Property | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-telemetry/` |
| Cargo name | `cssl-telemetry` |
| Description | "CSSLv3 stage0 — R18 ring-buffer + Level-Zero sysman + OTLP + audit-chain" |
| Spec | `specs/22_TELEMETRY.csl` |
| Dependencies | `blake3` (workspace), `ed25519-dalek` (workspace), `rand` (workspace), `thiserror` (workspace) |
| Total LOC | 1,518 (all .rs files combined) |

**Source files:**

| File | Lines |
|---|---|
| `src/lib.rs` | 54 |
| `src/scope.rs` | 317 |
| `src/schema.rs` | 151 |
| `src/ring.rs` | 252 |
| `src/audit.rs` | 519 |
| `src/exporter.rs` | 225 |

### 2.2 cssl-persist

| Property | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-persist/` |
| Cargo name | `cssl-persist` |
| Description | "CSSLv3 stage0 — orthogonal-persistence image + schema-migration + hot-reload" |
| Spec | `specs/18_ORTHOPERSIST.csl` |
| Dependencies | `thiserror` (workspace) |
| Total LOC | 610 (all .rs files combined) |

**Source files:**

| File | Lines |
|---|---|
| `src/lib.rs` | 45 |
| `src/schema.rs` | 91 |
| `src/migration.rs` | 136 |
| `src/image.rs` | 180 |
| `src/backend.rs` | 158 |

### 2.3 cssl-rt

| Property | Value |
|---|---|
| Path | `compiler-rs/crates/cssl-rt/` |
| Cargo name | `cssl-rt` |
| Description | "CSSLv3 stage0 — runtime library" |
| Spec | `specs/01_BOOTSTRAP.csl`, `specs/18_ORTHOPERSIST.csl` |
| Dependencies | none |
| Total LOC | 19 |

**Source files:**

| File | Lines |
|---|---|
| `src/lib.rs` | 19 |

---

## 3. FILE-BY-FILE AUDIT

---

### 3.1 cssl-telemetry — src/lib.rs (54 lines)

**Purpose:** Crate root. Declares all five public modules, re-exports the key public types via flat `pub use` statements, defines the `STAGE0_SCAFFOLD` version constant, and documents the phase-1 scope and phase-2 deferral list. The file enforces `#![forbid(unsafe_code)]` and two rustdoc lint levels across the entire crate.

**Items:**

- `const STAGE0_SCAFFOLD: &str` (line 50) — exposes `CARGO_PKG_VERSION`; used by integration harnesses to verify crate identity.
- `mod scaffold_tests` / `fn scaffold_version_present` (lines 53–59) — trivial smoke-test asserting the version string is non-empty.

**Attributes:**
- `#![forbid(unsafe_code)]` — hard block on unsafe; consistent with the stage-0 pure-safe mandate.
- `#![deny(rustdoc::broken_intra_doc_links)]` and `#![deny(rustdoc::private_intra_doc_links)]` — doc-link integrity enforcement.
- `#![allow(clippy::match_same_arms)]` and `#![allow(clippy::module_name_repetitions)]` — intentional suppressions for the scope-taxonomy match arms.

**Phase-2 deferral list (verbatim from doc-comment):**
- blake3 / ed25519-dalek integration (note: now partially done — see audit.rs)
- Real OTLP gRPC + HTTP exporter (needs prost / reqwest)
- Cross-thread ring-producer (stage-0 is single-thread SPSC only)
- Level-Zero sampling-thread integration
- Chrome-trace file-format round-trip + DevTools compatibility check
- `{Telemetry<S>}` effect-row lowering pass (HIR-level instrumentation)
- Overhead-budget enforcement (0.5% for Counters scope per specs/22)

**Note:** The lib.rs deferral comment at line 20 says "blake3 / ed25519-dalek integration (currently stubbed hashes)" — this is **stale**. `audit.rs` was upgraded with real crypto in phase-2a. The lib.rs comment was not updated to reflect that. Minor doc divergence.

---

### 3.2 cssl-telemetry — src/scope.rs (317 lines)

**Purpose:** Defines the canonical 26-variant telemetry scope taxonomy and the 5-variant event-kind enum, both with stable 16-bit wire encodings and human-readable string names. These enumerations are the type vocabulary used everywhere else in the crate — ring-buffer slots, exporter output, and schema metadata all reference `TelemetryScope` and `TelemetryKind`.

**Enums:**

- `enum TelemetryScope` (line 9) — 26 variants across five domains. Derives `Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord`. The `Ord` impl supports `BTreeSet` in `TelemetryScopeSet`.

  Variants and their domains:
  - CPU (8): `WallClock`, `CpuCycles`, `CpuInstRetired`, `CacheMisses`, `BranchMisses`, `TlbMisses`, `PageFaults`, `CtxSwitches`
  - GPU (6): `DispatchLatency`, `KernelOccupancy`, `ShaderInvocations`, `RtRaysPerSec`, `MemBandwidth`, `XmxUtilization`
  - Power/Thermal/Frequency (4): `Power`, `Thermal`, `Frequency`, `FanSpeed`
  - RAS (2): `EccErrors`, `PcieReplay`
  - App-semantic (4): `Counters`, `Spans`, `Events`, `Audit`
  - Compound (1): `Full`

- `enum ScopeDomain` (line 197) — 6-variant domain-category enum (`Cpu`, `Gpu`, `PowerThermal`, `Ras`, `AppSemantic`, `Compound`). Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`.

- `enum TelemetryKind` (line 229) — 5-variant event-kind enum (`Sample`, `SpanBegin`, `SpanEnd`, `Counter`, `Audit`). Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`.

**impl TelemetryScope:**

- `fn as_str(self) -> &'static str` (line 71, `const`) — returns lowercase kebab-case name for each variant. Used in exporter output and diagnostic messages.
- `fn domain(self) -> ScopeDomain` (line 103, `const`) — classifies variant into `ScopeDomain`. Exhaustive match.
- `fn as_u16(self) -> u16` (line 130, `const`) — stable 16-bit encoding for wire format and ring-slot storage. Values 0–23 for named scopes; `Full` is 255 (gap between 23 and 255 is intentional).
- `const ALL_SCOPES: [Self; 25]` (line 161) — array of all 25 non-`Full` + `Full` variants in taxonomy order. Used by `TelemetryScopeSet::full()` and scope-lookup helpers.

**impl fmt::Display for TelemetryScope** (line 190) — delegates to `as_str`.

**impl ScopeDomain:**
- `fn as_str(self) -> &'static str` (line 216, `const`) — domain short-name.

**impl TelemetryKind:**
- `fn as_str(self) -> &'static str` (line 246, `const`) — kind short-name.
- `fn as_u16(self) -> u16` (line 258, `const`) — stable encoding: Sample=0, SpanBegin=1, SpanEnd=2, Counter=3, Audit=4.

**Tests (mod tests, line 270):** 8 tests covering scope count, name strings, u16 uniqueness (verifies non-Full values are distinct), `Full`=255 encoding, domain groupings, domain names, kind names, kind u16 sequential ordering.

**Notable invariant:** `TelemetryScope::Full.as_u16()` is 255, not 24. This creates a gap. No test validates that the wire encoding round-trips (i.e., that `as_u16` → lookup returns the original variant). The `scope_name_for_u16` helper in `exporter.rs` does a linear search through `ALL_SCOPES`; since 255 is in `ALL_SCOPES` as `Full`, the mapping is complete, but a gap exists in the u16 space (values 24–254 are unassigned).

---

### 3.3 cssl-telemetry — src/schema.rs (151 lines)

**Purpose:** Provides `TelemetryScopeSet` (a `BTreeSet`-backed ordered collection of declared scopes) and `TelemetrySchema` (the metadata embedded in the fat-binary `[telemetry-schema]` section). Together these define what telemetry a compiled module declares and at what rate/ring-size.

**Structs:**

- `struct TelemetryScopeSet` (line 9) — wraps `BTreeSet<TelemetryScope>`. Derives `Debug, Clone, Default, PartialEq, Eq`.
  - Field: `scopes: BTreeSet<TelemetryScope>`

- `struct TelemetrySchema` (line 74) — fat-binary section payload. Derives `Debug, Clone, PartialEq, Eq`.
  - Fields: `version: u32`, `module: String`, `scopes: TelemetryScopeSet`, `ring_size: usize`, `sampling_hz: u32`

**impl TelemetryScopeSet:**

- `fn new() -> Self` (line 16, `must_use`) — returns `Self::default()`.
- `fn add(&mut self, s: TelemetryScope)` (line 21) — inserts into the BTreeSet.
- `fn contains(&self, s: TelemetryScope) -> bool` (line 27, `must_use`) — presence check.
- `fn iter(&self) -> impl Iterator<Item = TelemetryScope> + '_` (line 32) — sorted iteration (BTreeSet order = `PartialOrd` on `TelemetryScope`, which is the derived lexicographic order on the enum discriminant).
- `fn len(&self) -> usize` (line 38, `must_use`) — count.
- `fn is_empty(&self) -> bool` (line 44, `must_use`) — delegates to `len`.
- `fn is_subset_of(&self, other: &Self) -> bool` (line 51, `must_use`) — implements the spec/22 scope-narrowing invariant: "callee's scope ⊑ caller's scope". Uses `BTreeSet::is_subset`.
- `fn full() -> Self` (line 57, `must_use`) — returns a set containing all 25 scopes via `from_iter(TelemetryScope::ALL_SCOPES)`.

**impl FromIterator<TelemetryScope> for TelemetryScopeSet** (line 62) — standard collection builder.

**impl TelemetrySchema:**

- `fn defaults_for(module: impl Into<String>) -> Self` (line 90, `must_use`) — canonical defaults: version=1, empty scopes, ring_size=1<<20 (1,048,576 slots), sampling_hz=100. Values match specs/22 defaults.
- `fn summary(&self) -> String` (line 102, `must_use`) — human-readable one-line description for diagnostics.

**Tests (mod tests, line 115):** 6 tests: `empty_scope_set_is_empty`, `scope_set_add_contains`, `scope_set_subset_check`, `scope_set_full_has_all_25` (pins count at 25), `schema_defaults_canonical`, `schema_summary_has_module_and_ring_size`.

**Note:** `scope_set_full_has_all_25` pins len at 25, which matches `ALL_SCOPES.len()`. Consistent.

---

### 3.4 cssl-telemetry — src/ring.rs (252 lines)

**Purpose:** The single-producer single-consumer telemetry ring-buffer. Stage-0 uses `RefCell<VecDeque<TelemetrySlot>>` as a single-threaded stand-in with the correct overflow-counting semantics. Phase-2 swaps the internals for a lock-free atomic head/tail implementation; the public API (`push`, `drain_all`, `peek`, `len`, `overflow_count`, `capacity`) is stable across the swap.

**Structs:**

- `struct TelemetrySlot` (line 23) — the 64-byte ring slot, fixed layout matching specs/22 § TelemetrySlot. Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`.
  - Fields: `timestamp_ns: u64`, `scope: u16`, `kind: u16`, `thread_id: u32`, `cpu_or_gpu_id: u32`, `payload: [u8; 40]`, `payload_extern_ptr: u64`
  - Layout check: 8+2+2+4+4+40+8 = 68 bytes, not 64. The doc comment says "64-byte ring-slot record" but the actual struct is 68 bytes due to the two u16 fields and padding. **This is a spec/code divergence.**

- `struct TelemetryRing` (line 66) — the ring container. Derives `Debug`.
  - Fields: `slots: RefCell<VecDeque<TelemetrySlot>>`, `capacity: usize`, `overflow: Cell<u64>`, `total_pushed: Cell<u64>`

**enum RingError** (line 149) — single variant `Overflow` with `thiserror::Error` derive.

**impl TelemetrySlot:**

- `fn new(timestamp_ns: u64, scope: TelemetryScope, kind: TelemetryKind) -> Self` (line 43, `const`, `must_use`) — constructs a zeroed slot, encoding scope and kind via their `as_u16()` methods.
- `fn with_inline_payload(mut self, bytes: &[u8]) -> Self` (line 57, `must_use`) — builder-style payload writer; truncates input silently if longer than 40 bytes. Returns `self` for chaining.

**impl TelemetryRing:**

- `fn new(capacity: usize) -> Self` (line 76, `must_use`) — panics if capacity=0; allocates `VecDeque::with_capacity(capacity)`.
- `fn capacity(&self) -> usize` (line 88, `const`, `must_use`) — returns the configured capacity.
- `fn len(&self) -> usize` (line 94, `must_use`) — borrows the deque and returns its length.
- `fn is_empty(&self) -> bool` (line 100, `must_use`) — delegates to `len`.
- `fn overflow_count(&self) -> u64` (line 106, `must_use`) — monotonic discard counter; never resets.
- `fn total_pushed(&self) -> u64` (line 112, `must_use`) — total push attempts (successful + overflowed); useful for overhead-budget monitoring.
- `fn push(&self, slot: TelemetrySlot) -> Result<(), RingError>` (line 123) — non-blocking producer. Atomically increments `total_pushed` (saturating), borrows the deque mutably. If `len >= capacity`, increments `overflow` (saturating) and returns `Err(RingError::Overflow)`. Otherwise pushes to back. This is the producer-never-blocks guarantee from specs/22.
- `fn drain_all(&self) -> Vec<TelemetrySlot>` (line 137, `must_use`) — consumer drains all pending slots in FIFO order via `drain(..)`. Returns a `Vec`.
- `fn peek(&self) -> Option<TelemetrySlot>` (line 143, `must_use`) — borrows the deque and returns a copy of the front element without removing it.

**Tests (mod tests, line 157):** 7 tests: `new_ring_has_capacity`, `zero_capacity_panics` (should_panic), `slot_new_zeroes_payload`, `slot_with_inline_payload_writes_bytes`, `slot_with_inline_payload_truncates_long`, `push_and_drain_preserves_fifo_order`, `overflow_increments_counter_not_blocks`, `peek_does_not_remove`, `total_pushed_counts_all_attempts`.

**Notable algorithm — ring semantics:** The design deliberately drops new events when full (oldest data is preserved; newest is discarded). This is the spec/22 "lossy-non-blocking" policy. The `overflow_count` counter allows callers to detect loss without blocking.

**Bug / correctness issue:** `TelemetrySlot` is documented as "64-byte ring-slot record" at line 22, but the struct layout is 68 bytes (u64 + u16 + u16 + u32 + u32 + [u8;40] + u64 = 8+2+2+4+4+40+8 = 68, ignoring any compiler padding). The two u16 fields side by side are likely packed without extra padding on x86-64, but the actual size_of would be 68 unless Rust aligns differently. No test asserts `size_of::<TelemetrySlot>() == 64`. If the spec mandates 64 bytes for the hardware ring layout, this is a correctness divergence that will matter when phase-2 introduces the lock-free atomic ring with memory-mapped backing.

---

### 3.5 cssl-telemetry — src/audit.rs (519 lines)

**Purpose:** The cryptographic audit chain — the core of F6 observability's integrity guarantee. Implements BLAKE3 content-hashing, Ed25519 signing/verification, and an append-only chain of signed entries. Phase-2a upgraded this file from stub crypto to real `blake3` and `ed25519-dalek` primitives while retaining stub methods for tests that pin specific byte-patterns.

**Type aliases / new-types:**

- `struct ContentHash([u8; 32])` (line 28) — 32-byte BLAKE3 digest wrapper. Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`.
- `struct Signature([u8; 64])` (line 71) — 64-byte Ed25519 signature wrapper. Derives `Debug, Clone, Copy, PartialEq, Eq, Hash`.
- `struct SigningKey { inner: DalekSigningKey }` (line 109) — Ed25519 signing-key wrapper. Derives `Clone`. Has a custom `Debug` impl that reveals only the verifying-key digest, never the secret material.
- `struct AuditEntry` (line 185) — one chain entry. Derives `Debug, Clone, PartialEq, Eq`.
  - Fields: `seq: u64`, `timestamp_s: u64`, `content_hash: ContentHash`, `prev_hash: ContentHash`, `signature: Signature`, `tag: String`, `message: String`
- `struct AuditChain` (line 219) — the chain container. Derives `Debug, Clone, Default`.
  - Fields: `entries: Vec<AuditEntry>`, `signing_key: Option<SigningKey>`
- `enum AuditError` (line 352) — four variants: `GenesisPrevNonZero`, `ChainBreak { seq: u64 }`, `InvalidSequence { expected: u64, actual: u64 }`, `SignatureInvalid`. Implements `thiserror::Error`.

**impl ContentHash:**

- `fn zero() -> Self` (line 33, `const`, `must_use`) — all-zero placeholder.
- `fn hash(bytes: &[u8]) -> Self` (line 41, `must_use`) — **real BLAKE3** via `blake3::hash`; the phase-2a upgrade target. Returns the 32-byte digest as a `ContentHash`.
- `fn stub_hash(bytes: &[u8]) -> Self` (line 50, `must_use`) — deterministic non-crypto XOR-fold into 32 bytes; retained for test stability. Same algorithm used in `cssl-persist`'s `with_digest_from` and `stub_content_digest` — note this duplicated stub implementation.
- `fn hex(&self) -> String` (line 59, `must_use`) — lowercase hex-encode, 64 chars.

**impl Signature:**

- `fn zero() -> Self` (line 74, `const`, `must_use`) — all-zero placeholder.
- `fn sign(key: &SigningKey, message: &[u8]) -> Self` (line 82, `must_use`) — **real Ed25519** signing via `key.inner.sign(message)` (ed25519-dalek `Signer` trait).
- `fn stub_sign(message: &[u8]) -> Self` (line 91, `must_use`) — deterministic non-crypto stub: double-folds `stub_hash` into 64 bytes. Retained for unit-tests and chains without a signing key.

**impl SigningKey:**

- `fn generate() -> Self` (line 127, `must_use`) — generates a fresh random key using `rand::rngs::OsRng`.
- `fn from_seed(seed: [u8; 32]) -> Self` (line 137, `must_use`) — deterministic key from a 32-byte seed via `DalekSigningKey::from_bytes`. Used for reproducible-build and R16 attestation.
- `fn verifying_key_bytes(&self) -> [u8; 32]` (line 145, `must_use`) — extracts the 32-byte public key.
- `fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), AuditError>` (line 153) — verifies using the dalek `Verifier` trait; maps any error to `AuditError::SignatureInvalid`.

**fn verify_detached** (line 171, free function) — third-party auditor path. Takes `verifying_key: &[u8; 32]`, `message: &[u8]`, `signature: &Signature`. Constructs a `VerifyingKey` from raw bytes (returns `SignatureInvalid` on invalid curve point), then verifies. This is the public key-only verification surface for downstream consumers.

**impl AuditEntry:**

- `fn sign_input(&self) -> Vec<u8>` (line 205, `must_use`) — builds the byte vector that is hashed before signing: `seq(LE-u64) || timestamp_s(LE-u64) || content_hash(32) || prev_hash(32) || tag.as_bytes() || b'|' || message.as_bytes()`. Total length for a 1-byte tag and 1-byte message: 83 bytes (confirmed by test).

**impl AuditChain:**

- `fn new() -> Self` (line 231, `must_use`) — empty chain with no signing key.
- `fn with_signing_key(key: SigningKey) -> Self` (line 237, `must_use`) — chain with real signing.
- `fn signing_key(&self) -> Option<&SigningKey>` (line 247, `const`, `must_use`) — accessor.
- `fn append(&mut self, tag: impl Into<String>, message: impl Into<String>, timestamp_s: u64)` (line 253) — core mutation. Computes `content_hash = ContentHash::hash(message)` (real BLAKE3). Sets `prev_hash` to the preceding entry's `content_hash`, or `ContentHash::zero()` for genesis. Builds a temporary `AuditEntry` with a zero signature to compute `sign_input`. Signs: if a `SigningKey` is present, uses `Signature::sign`; otherwise uses `Signature::stub_sign`. Pushes the final entry.
- `fn len(&self) -> usize` (line 289, `must_use`)
- `fn is_empty(&self) -> bool` (line 295, `must_use`)
- `fn iter(&self) -> impl Iterator<Item = &AuditEntry>` (line 300) — immutable iteration.
- `fn verify_chain(&self) -> Result<(), AuditError>` (line 312) — full chain integrity check. For each entry: verifies monotonic `seq`, checks `prev_hash` linkage (genesis must be zero; subsequent must match previous entry's `content_hash`). If a signing key is present, reconstructs the `sign_input`, computes the expected stub signature, and if the actual signature differs from the stub, calls `key.verify`. If no signing key is present, structural checks only.

**Tests (mod tests, line 369):** 20 tests split into two groups:

*Stub/structural tests (original phase-1):*
`content_hash_zero_is_all_zeroes`, `content_hash_stub_deterministic`, `content_hash_different_inputs_different_outputs`, `content_hash_hex_is_64_chars`, `signature_stub_deterministic`, `empty_chain_verifies`, `append_builds_sequential_chain`, `chain_verify_detects_break`, `chain_verify_detects_bad_genesis`, `chain_verify_detects_bad_seq`, `entry_sign_input_includes_seq_and_hash`.

*Phase-2a real-crypto tests:*
`real_blake3_hash_is_cryptographic`, `real_blake3_differs_from_stub`, `signing_key_from_seed_deterministic`, `signing_key_generate_is_nondeterministic`, `real_ed25519_sign_verify_roundtrip`, `real_ed25519_verify_rejects_wrong_message`, `signing_key_debug_hides_secret`, `signed_chain_verifies_with_real_key`, `signed_chain_detects_tampered_signature`, `chain_without_key_still_verifies_structurally`, `signing_key_access_via_const_accessor`.

**Notable correctness issue — verify_chain stub-signature bypass:** In `verify_chain` (line 329–344), when a signing key is present the code reconstructs the expected stub signature and skips real Ed25519 verification if the stored signature equals the stub. This creates a **security hole in mixed-mode chains**: if `AuditChain::new()` (no key) is used first and then entries are migrated to a keyed chain, or if an attacker knows the stub-sign algorithm, they can forge entries whose signature passes `verify_chain` even against a keyed chain by producing stub-sign output. The comment at line 329 frames this as intentional for "unit-tests + CI without a long-term key-store," but the bypass applies at the `verify_chain` level even after a real key is attached, which undermines the integrity guarantee if the chain is ever operated in a "no key initially, add key later" pattern. The bypass should be scope-restricted to chains that have never had a signing key, or removed entirely in phase-2.

**Notable correctness issue — prev_hash links content_hash, not a chain hash:** The chain at line 257 sets `prev_hash` to `self.entries.last().map_or(ContentHash::zero(), |e| e.content_hash)`. This links each entry to the previous entry's message content hash, not to a hash of the full previous entry (which would include the previous entry's own `prev_hash` and `seq`). This means two entries with identical messages but different positions in the chain produce identical `prev_hash` links — chain-position is not commit-hashed. A true blockchain-style hash-chain would hash the entire serialised previous entry. The current design provides weaker linkage.

---

### 3.6 cssl-telemetry — src/exporter.rs (225 lines)

**Purpose:** Defines the `Exporter` trait and three implementations: `ChromeTraceExporter` (functional, writes Chrome tracing JSON to an in-memory buffer), `JsonExporter` (functional, writes newline-delimited JSON), and `OtlpExporter` (stub, always returns `ExportError::NotWired`). Two private helper functions convert u16-encoded scope/kind identifiers back to their string names.

**Enum:**

- `enum ExportError` (line 13) — three variants: `EndpointUnreachable { endpoint: String }`, `Serialization(String)`, `NotWired`. Derives `Debug, Error, PartialEq, Eq`.

**Trait:**

- `trait Exporter` (line 29) — two required methods:
  - `fn name(&self) -> &'static str` — human-readable exporter identifier.
  - `fn export_batch(&self, slots: &[TelemetrySlot]) -> Result<usize, ExportError>` — export a batch; returns slot count on success.

**Structs and their impl blocks:**

- `struct ChromeTraceExporter { buffer: RefCell<String> }` (line 45) — Derives `Debug, Clone, Default`.
  - `fn new() -> Self` (line 53, `must_use`) — empty exporter.
  - `fn take_output(&self) -> String` (line 61, `must_use`) — drains and returns the buffer via `replace`.
  - `impl Exporter for ChromeTraceExporter` (line 66):
    - `fn name()` → `"chrome-trace"`
    - `fn export_batch(slots)` (line 71) — if the buffer is empty, opens with `"[\n"`. For each slot, emits a comma-separated JSON object `{"name": "<scope>", "ph": "<phase>", "ts": <ts/1000>, "pid": <cpu_or_gpu_id>, "tid": <thread_id>}`. Phase mapping: SpanBegin→"B", SpanEnd→"E", Counter→"C", everything else→"i". Timestamp is divided by 1000 to convert ns→µs (Chrome trace format uses µs). Uses `core::fmt::Write` for formatting.

- `struct JsonExporter { buffer: RefCell<String> }` (line 104) — Derives `Debug, Clone, Default`.
  - `fn new() -> Self` (line 111, `must_use`) — empty exporter.
  - `fn take_output(&self) -> String` (line 118, `must_use`) — drains via `replace`.
  - `impl Exporter for JsonExporter` (line 123):
    - `fn name()` → `"json-lines"`
    - `fn export_batch(slots)` (line 128) — emits one `{"ts_ns": ..., "scope": "...", "kind": "...", "tid": ..., "pid": ...}` per slot, each on its own line with `writeln!`.

- `struct OtlpExporter { endpoint: String }` (line 148) — Derives `Debug, Clone`.
  - `fn new(endpoint: impl Into<String>) -> Self` (line 155, `must_use`) — stores endpoint.
  - `impl Exporter for OtlpExporter` (line 163):
    - `fn name()` → `"otlp"`
    - `fn export_batch(_slots)` (line 168) — always returns `Err(ExportError::NotWired)`. Comment: "Phase-1 : no real network transport ; phase-2 wires prost / reqwest."

**Private helpers:**

- `fn scope_name_for_u16(u: u16) -> &'static str` (line 174) — linear search through `TelemetryScope::ALL_SCOPES`; returns `"unknown"` for unrecognised values. O(25) worst-case; acceptable for stage-0.
- `fn kind_name_for_u16(u: u16) -> &'static str` (line 181) — manual iteration over 5 `TelemetryKind` variants; returns `"unknown"`. Logically equivalent to a match.

**Tests (mod tests, line 196):** 5 tests: `chrome_trace_exports_slots`, `json_exporter_emits_lines`, `otlp_exporter_returns_not_wired`, `exporter_names`, `export_empty_batch_is_ok`.

**Notable issue — ChromeTraceExporter comma logic:** The comma insertion logic at lines 79–80 (`if n > 0 || i > 0`) produces a leading comma before the very first element on the second call to `export_batch` if the first call ended at `n=0` but `i` was incremented past 0 for some reason — but this cannot happen because `i` only advances when `slots` is non-empty, so `n` and `i` are always in sync. The condition could be simplified to `if i > 0` but is not a bug in practice.

**Notable issue — Chrome trace format incomplete:** The emitted JSON does not include a closing `"]"` bracket. `take_output` drains the buffer in mid-array state. A consumer that calls `export_batch` followed by `take_output` will receive an unclosed JSON array. This is explicitly noted as a phase-2 compatibility gap in the lib.rs deferral list ("Chrome-trace file-format round-trip + DevTools compatibility check").

---

## 4. cssl-persist FILE-BY-FILE AUDIT

---

### 4.1 cssl-persist — src/lib.rs (45 lines)

**Purpose:** Crate root. Declares four public modules, re-exports the key public types, defines `STAGE0_SCAFFOLD`, and documents the phase-1 scope and phase-2 deferral list.

**Items:**

- `const STAGE0_SCAFFOLD: &str` (line 41) — crate version.
- `mod scaffold_tests` / `fn scaffold_version_present` (lines 43–50) — trivial smoke-test.

**Attributes:** Same set as cssl-telemetry (`forbid(unsafe_code)`, rustdoc lint denials, match_same_arms and module_name_repetitions allowances).

**Phase-2 deferral list (from doc-comment):**
- WAL-file backend (append-only log + snapshot checkpoints)
- LMDB backend (alternative for large working-sets)
- `@hot_reload_preserve` HIR attribute extraction + root-set discovery
- Schema-derivation from HIR-types (hooks into cssl-hir)
- Live-object migration (apply migration-chain to in-flight image)
- R16 attestation of image-provenance (BLAKE3 chain + Ed25519 signature)

---

### 4.2 cssl-persist — src/schema.rs (91 lines)

**Purpose:** Defines `SchemaVersion`, the monotonic (major, minor) version identifier with a 32-byte stub digest. Used everywhere records or migrations need to be tagged with a schema identity.

**Struct:**

- `struct SchemaVersion { major: u32, minor: u32, digest: [u8; 32] }` (line 10) — Derives `Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord`.

**impl SchemaVersion:**

- `fn new(major: u32, minor: u32) -> Self` (line 22, `const`, `must_use`) — constructs with all-zero digest.
- `fn genesis() -> Self` (line 31, `const`, `must_use`) — returns `Self::new(1, 0)`.
- `fn with_digest_from(mut self, canonical_bytes: &[u8]) -> Self` (line 38, `must_use`) — stage-0 stub: XOR-folds input bytes into the 32-byte digest using the same algorithm as `ContentHash::stub_hash` in cssl-telemetry. The doc comment says "Stage-0 stub-hash of canonical_bytes. Phase-2 swaps for BLAKE3." This stub is **not cryptographically strong** and will not detect intentional collisions.
- `fn is_minor_upgrade_of(self, other: Self) -> bool` (line 47, `must_use`) — returns true iff same major and `self.minor > other.minor`.

**impl fmt::Display for SchemaVersion** (line 52) — formats as `"{major}.{minor}"`.

**Tests (mod tests, line 58):** 5 tests: `new_has_zero_digest`, `genesis_is_1_0`, `with_digest_stable_for_same_input`, `with_digest_distinguishes_inputs`, `minor_upgrade_detected`, `display_format`.

**Note:** The `with_digest_from` stub algorithm is copy-pasted from the telemetry crate's `ContentHash::stub_hash`. If both are upgraded to real BLAKE3, they should become a shared primitive to avoid future drift.

---

### 4.3 cssl-persist — src/migration.rs (136 lines)

**Purpose:** Defines `SchemaMigration` (one from→to version-pair step with an id label) and `MigrationChain` (an ordered, contiguous sequence of steps). The chain enforces linkage: each pushed migration's `before` version must equal the current tail's `after` version.

**Structs:**

- `struct SchemaMigration { before: SchemaVersion, after: SchemaVersion, id: String, description: Option<String> }` (line 7) — Derives `Debug, Clone, PartialEq, Eq`.
- `struct MigrationChain { migrations: Vec<SchemaMigration> }` (line 39) — Derives `Debug, Clone, Default, PartialEq, Eq`.

**impl SchemaMigration:**

- `fn new(before: SchemaVersion, after: SchemaVersion, id: impl Into<String>) -> Self` (line 21, `must_use`) — constructs with `description: None`.
- `fn with_description(mut self, desc: impl Into<String>) -> Self` (line 31, `must_use`) — builder-pattern description setter.

**impl MigrationChain:**

- `fn new() -> Self` (line 47, `must_use`) — returns `Self::default()`.
- `fn push(&mut self, m: SchemaMigration)` (line 52) — appends a migration, panicking if the tail's `after` does not match `m.before`. Error message includes both versions.
- `fn is_empty(&self) -> bool` (line 65, `must_use`)
- `fn len(&self) -> usize` (line 70, `must_use`)
- `fn start_version(&self) -> Option<SchemaVersion>` (line 76, `must_use`) — returns `migrations.first().map(|m| m.before)`.
- `fn end_version(&self) -> Option<SchemaVersion>` (line 82, `must_use`) — returns `migrations.last().map(|m| m.after)`.
- `fn iter(&self) -> impl Iterator<Item = &SchemaMigration>` (line 88) — ordered iteration.

**Tests (mod tests, line 94):** 4 tests: `migration_construct`, `empty_chain_shape`, `chain_push_sequential`, `chain_push_broken_panics` (should_panic), `chain_iter_preserves_order`.

**Notable design note:** The migration chain enforces structural adjacency but does not validate version monotonicity (e.g., it would accept a chain that goes 1.0 → 0.9). Since `SchemaVersion` derives `PartialOrd` based on the field order (major then minor), downstream code could check `m.after > m.before` but this crate does not. Phase-2 should add this validation.

---

### 4.4 cssl-persist — src/image.rs (180 lines)

**Purpose:** Defines the in-memory persistence image: `ImageHeader` (the magic-bytes metadata block), `ImageRecord` (one typed record with a string key and byte payload), and `PersistenceImage` (header + ordered records). A private helper computes the stub content digest.

**Structs:**

- `struct ImageHeader` (line 7) — Derives `Debug, Clone, PartialEq, Eq`.
  - Fields: `magic: [u8; 8]`, `format_version: u32`, `schema: SchemaVersion`, `timestamp_s: u64`, `record_count: u32`, `content_digest: [u8; 32]`
  - `const MAGIC: [u8; 8]` (line 24) — `*b"CSSLPRS1"` — the persistence-image magic bytes.

- `struct ImageRecord` (line 42) — Derives `Debug, Clone, PartialEq, Eq`.
  - Fields: `key: String`, `schema: SchemaVersion`, `payload: Vec<u8>`

- `struct PersistenceImage` (line 71) — Derives `Debug, Clone, PartialEq, Eq`.
  - Fields: `header: ImageHeader`, `records: Vec<ImageRecord>`

**impl ImageHeader:**

- `fn new(schema: SchemaVersion, timestamp_s: u64) -> Self` (line 28, `must_use`) — initialises with `MAGIC`, `format_version=1`, `record_count=0`, all-zero digest.

**impl ImageRecord:**

- `fn new(key: impl Into<String>, schema: SchemaVersion, payload: Vec<u8>) -> Self` (line 54, `must_use`)
- `fn payload_size(&self) -> usize` (line 64, `must_use`) — delegates to `payload.len()`.

**impl PersistenceImage:**

- `fn new(schema: SchemaVersion, timestamp_s: u64) -> Self` (line 81, `must_use`) — empty image with fresh header.
- `fn push_record(&mut self, r: ImageRecord)` (line 90) — appends a record; updates `header.record_count` using `u32::try_from(...).unwrap_or(u32::MAX)` for overflow safety; recomputes `header.content_digest` via `stub_content_digest`.
- `fn find(&self, key: &str) -> Option<&ImageRecord>` (line 98, `must_use`) — linear search by key. O(n).
- `fn total_payload_size(&self) -> usize` (line 104, `must_use`) — sums `payload_size()` across all records.

**Private fn stub_content_digest(records: &[ImageRecord]) -> [u8; 32]** (line 109) — XOR-folds key bytes and payload bytes into a 32-byte digest using per-byte rotate-and-XOR. Stage-0 placeholder; phase-2 replaces with real BLAKE3 over a canonical encoding.

**Tests (mod tests, line 123):** 6 tests: `header_has_canonical_magic`, `new_image_is_empty`, `push_record_updates_count_and_digest`, `find_locates_record_by_key`, `digest_stable_for_same_records`, `record_payload_size`.

**Notable issue:** `push_record` recomputes the stub digest from scratch over all records on every insertion — O(n×|payload|) total cost over n insertions. For phase-2 with real BLAKE3, this should be an incremental hash or a single hash at snapshot time.

---

### 4.5 cssl-persist — src/backend.rs (158 lines)

**Purpose:** Defines the `PersistenceBackend` trait (the stable API surface for WAL/LMDB/in-memory dispatch) and `InMemoryBackend` (the stage-0 `HashMap<String, ImageRecord>` reference implementation with insertion-order tracking).

**Enum:**

- `enum PersistError` (line 11) — three variants: `NotFound { key: String }`, `SchemaMismatch { found: SchemaVersion, expected: SchemaVersion }`, `BackendNotWired { backend: &'static str }`. Derives `Debug, Error, PartialEq, Eq`.

**Trait:**

- `trait PersistenceBackend` (line 28) — five methods:
  - `fn name(&self) -> &'static str` — backend identifier.
  - `fn put(&mut self, record: ImageRecord) -> Result<(), PersistError>` — write; overwrites on key collision.
  - `fn get(&self, key: &str) -> Result<ImageRecord, PersistError>` — read; `NotFound` on miss.
  - `fn snapshot(&self, timestamp_s: u64, schema: SchemaVersion) -> Result<PersistenceImage, PersistError>` — full image snapshot.
  - `fn len(&self) -> usize` — record count.
  - `fn is_empty(&self) -> bool` (provided, line 54) — delegates to `len() == 0`.

**Struct:**

- `struct InMemoryBackend { records: HashMap<String, ImageRecord>, insertion_order: Vec<String> }` (line 60) — Derives `Debug, Clone, Default`. The dual-field design (`HashMap` for O(1) lookup, `Vec<String>` for insertion-order iteration) ensures `snapshot` returns records in deterministic order regardless of `HashMap` internals.

**impl InMemoryBackend:**

- `fn new() -> Self` (line 66, `must_use`) — returns `Self::default()`.

**impl PersistenceBackend for InMemoryBackend:**

- `fn name()` → `"in-memory"` (line 75)
- `fn put(&mut self, record: ImageRecord)` (line 78) — clones the key; if not already present, appends to `insertion_order`. Inserts into `records` (overwrite on collision does not re-append to `insertion_order`, preserving original insertion position).
- `fn get(&self, key: &str)` (line 87) — `HashMap::get` + clone + `NotFound` on miss.
- `fn snapshot(&self, timestamp_s: u64, schema: SchemaVersion)` (line 96) — iterates `insertion_order`, fetching each key from `records` and pushing to a fresh `PersistenceImage`. Returns the image.
- `fn len(&self) -> usize` (line 110) — `records.len()`.

**Tests (mod tests, line 115):** 6 tests: `new_backend_is_empty`, `put_and_get`, `get_missing_returns_not_found`, `put_overwrites_same_key_once_in_order`, `snapshot_preserves_insertion_order`, `snapshot_record_count_correct`. Also includes private helper `fn rec(key, payload) -> ImageRecord` (line 122).

**Notable design note:** `PersistError::SchemaMismatch` is defined but never returned by any current backend method — `InMemoryBackend::get` does not check schema version. This variant is reserved for phase-2 typed deserialization paths.

---

## 5. cssl-rt — src/lib.rs (19 lines)

**Purpose:** Crate root and only file. Declares the intended role of the runtime library (allocator hooks, TelemetryRing integration, evidence passing, orthopersist image API) but provides only the `STAGE0_SCAFFOLD` constant and a trivial test. No dependencies. No `use` statements. This is a pure scaffold.

**Items:**

- `const STAGE0_SCAFFOLD: &str` (line 14) — crate version.
- `mod scaffold_tests` / `fn scaffold_version_present` (lines 16–21) — same trivial smoke test pattern as in the other two crates.

**Attributes:** `#![forbid(unsafe_code)]`, rustdoc lint denials.

**Status comment (verbatim from lines 5–7):**
> `§ STATUS : T10+ scaffold — runtime entry-points + persistence bridge pending.`
> `§ ROLE   : linked into every CSSLv3 artifact; provides allocator, TelemetryRing`
> `           hooks, evidence-passing plumbing, and orthopersist image API.`

None of the described role is implemented. The crate is entirely a placeholder.

---

## 6. SLICE NOTES

### 6.1 Test Coverage

| Crate | Test modules | Test count | Coverage quality |
|---|---|---|---|
| cssl-telemetry/scope.rs | 1 | 8 | Good — covers all enum properties |
| cssl-telemetry/schema.rs | 1 | 6 | Good — covers set operations and schema defaults |
| cssl-telemetry/ring.rs | 1 | 8 | Good — covers SPSC semantics, overflow, FIFO order |
| cssl-telemetry/audit.rs | 1 | 20 | Excellent — covers stub and real crypto, signed chain, tamper detection |
| cssl-telemetry/exporter.rs | 1 | 5 | Adequate — covers happy paths; does not test malformed JSON output |
| cssl-telemetry/lib.rs | 1 | 1 | Trivial |
| cssl-persist/schema.rs | 1 | 5 | Good |
| cssl-persist/migration.rs | 1 | 5 | Good — includes broken-chain panic |
| cssl-persist/image.rs | 1 | 6 | Good |
| cssl-persist/backend.rs | 1 | 5 | Adequate — does not test schema-mismatch path |
| cssl-persist/lib.rs | 1 | 1 | Trivial |
| cssl-rt/lib.rs | 1 | 1 | Trivial |

No integration tests exist for any of the three crates. No `tests/` directories were found.

### 6.2 Incomplete / Stubbed Items

**cssl-telemetry:**
- `OtlpExporter::export_batch` always returns `Err(ExportError::NotWired)` — no network transport.
- `ChromeTraceExporter` does not close the JSON array — `take_output` returns a fragment.
- Level-Zero sysman sampling thread not wired (planned via `cssl-host-level-zero`).
- Cross-thread SPSC ring not implemented — `TelemetryRing` is single-thread only via `RefCell`.
- Overhead-budget enforcement (0.5% for Counters per specs/22) not present.
- `{Telemetry<S>}` effect-row HIR lowering pass absent.
- lib.rs doc-comment at line 20 incorrectly describes blake3/ed25519 as "currently stubbed hashes" — they are now real.

**cssl-persist:**
- WAL backend, LMDB backend not present.
- `@hot_reload_preserve` HIR attribute extraction not present.
- Schema-derivation from HIR types not present.
- Live-object migration (applying a `MigrationChain` to a live image) not present.
- R16 image-provenance attestation (BLAKE3 + Ed25519 chain) not present.
- `PersistError::SchemaMismatch` is defined but never produced by any current code path.
- `SchemaVersion::with_digest_from` and `ImageHeader::content_digest` use stub XOR-fold, not BLAKE3.

**cssl-rt:**
- Entirely a scaffold. No allocator, no telemetry hooks, no orthopersist bridge.

### 6.3 Spec Divergences

| Issue | File:line | Severity |
|---|---|---|
| `TelemetrySlot` documented as "64-byte" but is 68 bytes by field layout | `ring.rs:22` | Medium — will break when lock-free SPSC with memory-mapped ring is implemented |
| lib.rs deferral comment says blake3/ed25519 "currently stubbed" — stale after phase-2a upgrade | `lib.rs:20` | Low — doc only |
| Chain links `prev_hash` to message `content_hash` only, not a hash of the full previous entry | `audit.rs:257` | Medium — weaker chain integrity than blockchain-standard |
| `verify_chain` skips Ed25519 verification for stub-signed entries even when a signing key is present | `audit.rs:329–344` | High — security bypass |
| `MigrationChain::push` does not enforce version monotonicity | `migration.rs:52` | Low — would allow 1.2 → 1.0 migrations |
| `stub_content_digest` algorithm is copy-duplicated in both crates | `image.rs:109`, `schema.rs:38` | Low — maintenance divergence risk |

### 6.4 Dead Code / Surprises

- `PersistError::SchemaMismatch` (backend.rs:18–21) is dead — no code path produces it. It is a forward declaration for phase-2.
- `scope_name_for_u16` and `kind_name_for_u16` in `exporter.rs` are private and only called from `export_batch` implementations. They are O(n) linear searches; fine for 25 and 5 elements respectively, but could be a simple match if the u16 encoding is stable (which it is — `as_u16` has stable assignment).
- `TelemetryScope::Full.as_u16()` returns 255 with values 24–254 unused. This is intentional (reserve space for future scopes) but undocumented as an explicit design decision in the source.
- `SigningKey` does not implement `PartialEq` or `Eq`. This is correct for a signing key (comparing secret-key material is generally wrong), but means `AuditChain` cannot derive `PartialEq` via its `signing_key: Option<SigningKey>` field — `AuditChain` correctly does not derive `PartialEq`.

### 6.5 Bugs Flagged

1. **`audit.rs:329–344` — stub-signature bypass in `verify_chain`:** When a signing key is attached to the chain, `verify_chain` checks whether the stored signature equals the stub-sign output; if it does, Ed25519 verification is skipped. This allows an attacker who knows the stub-sign algorithm to forge entries that pass chain verification against a keyed chain. The bypass should be removed or made conditional on whether the chain has ever operated in keyless mode.

2. **`ring.rs:22` — TelemetrySlot size mismatch:** The struct is documented as 64 bytes but its field layout totals 68 bytes (`u64 + u16 + u16 + u32 + u32 + [u8;40] + u64` = 8+2+2+4+4+40+8 = 68, with potential for compiler alignment padding to bring it higher). No `assert_eq!(std::mem::size_of::<TelemetrySlot>(), 64)` exists. When phase-2 maps this struct to a hardware ring buffer, the size assumption will be violated.

3. **`audit.rs:257` — weak chain linkage:** `prev_hash` contains only the previous entry's message content hash, not a hash of the full serialized previous entry. An adversary who can swap message text while preserving hash values (cryptographically infeasible with real BLAKE3 but structurally incomplete as a design) would not be caught by the linkage check. More importantly, the chain does not commit to the previous entry's `seq`, `timestamp_s`, `tag`, or own `signature`, so those fields are not covered by the chain linkage.

---

*End of audit — 12 source files audited, 3 Cargo.toml audited.*
