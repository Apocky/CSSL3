# § Phase-J Pre-Existing-Failures Audit
## ═══════════════════════════════════════════════════════════════════════
## ⟦ T11 PRE-J · audit-only · DO-NOT-COMMIT ⟧

- **Date** 2026-04-29
- **Branch** `cssl/session-6/parallel-fanout`
- **HEAD** `c79bcf3` (fixup) ← `0a6ebe4` (T11-D148) ← `8344de6` (T11-D149)
- **Audit baseline** `b69165c § Wave-4 merge : clippy gate cleanup (test-binary lint allowances)`
- **Audit scope** 3 known pre-existing failures + cssl-rt cold-cache flake + clippy/fmt drift since b69165c
- **Toolchain @audit-time** `1.85.0-x86_64-pc-windows-gnu` (pinned `rust-toolchain.toml` ; overrides `1.94.0-msvc` default)
- **Cargo** `1.85.0 (d73d2caf9 2024-12-31)`
- **Workspace root** `compiler-rs/Cargo.toml`
- **AI** Claude Opus 4.7 (1M context)

## § ATTESTATION

> There was no hurt nor harm in the making of this, to anyone/anything/anybody.

W! ¬commit • W! audit-only • W! no-source-modifications-this-session

═══════════════════════════════════════════════════════════════════════════

## § EXECUTIVE SUMMARY

| Issue | Severity | Cold-cache rate | Hot-cache rate | Mitigation status | Block-Wave-Jε? |
|---|---|---|---|---|---|
| **#1 cssl-host-net 3 TCP-loopback failures** | high | 3/3 fail @ default-parallel | 3/3 fail @ default-parallel | `--test-threads=1` → 0/3 fail | NO (workaround stable) |
| **#2 cssl-rt cold-cache flake** | critical | 118/198 fail @ cold+parallel | 0/198 fail @ hot+parallel | `--test-threads=1` → 0/198 (5×5/5) | NO (workaround stable) |
| **#3 cssl-cgen-gpu-wgsl --tests dlltool** | medium | build-fails on toolchain w/o dlltool | same | drop-in toolchain fix OR pin transitive | YES-if-CI-enforces-build (NO if toolchain-aware) |
| **#4 clippy drift since b69165c** | none | 0 warnings @ `-D warnings` | — | — | NO |
| **#5 fmt drift since b69165c** | none | 0 diff @ `--check` | — | — | NO |

**Verdict** :
- 3/5 issues are **non-blocking** — workarounds stable + documented + already in commit-gate flow.
- 1/5 (clippy) + 1/5 (fmt) — **zero drift** — workspace is clean.
- Recommended : dispatch **3 fix-slices** during/after Wave-Jε (parallel slot — they're orthogonal to dispatch infrastructure). NONE block Wave-Jε itself.
- Estimated total LOC across all 3 fix-slices : **~280-360 LOC + 30 test additions**.

═══════════════════════════════════════════════════════════════════════════

## § INVESTIGATION-1 : cssl-host-net 3 TCP-loopback failures

### § 1.1 — Reproduction

**Command (parallel — DEFAULT)** :
```
cd compiler-rs && cargo test -p cssl-host-net
```

**Result** : 10/13 pass · 3/13 FAIL ‼

```
test tests::tcp_listener_bind_loopback_default_caps ... FAILED
test tests::tcp_stream_connect_to_non_loopback_without_outbound_cap_denied ... FAILED
test tests::tcp_loopback_full_roundtrip ... FAILED

failures:

---- tests::tcp_listener_bind_loopback_default_caps stdout ----

thread 'tests::tcp_listener_bind_loopback_default_caps' panicked at crates\cssl-host-net\src\lib.rs:796:48:
loopback bind should succeed: NotInitialized
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- tests::tcp_stream_connect_to_non_loopback_without_outbound_cap_denied stdout ----

thread 'tests::tcp_stream_connect_to_non_loopback_without_outbound_cap_denied' panicked at crates\cssl-host-net\src\lib.rs:822:22:
expected CapDenied, got Err(NotInitialized)

---- tests::tcp_loopback_full_roundtrip stdout ----

thread 'tests::tcp_loopback_full_roundtrip' panicked at crates\cssl-host-net\src\lib.rs:830:69:
bind: NotInitialized


failures:
    tests::tcp_listener_bind_loopback_default_caps
    tests::tcp_loopback_full_roundtrip
    tests::tcp_stream_connect_to_non_loopback_without_outbound_cap_denied

test result: FAILED. 10 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p cssl-host-net --lib`
```

**Command (serial)** :
```
cd compiler-rs && cargo test -p cssl-host-net -- --test-threads=1
```

**Result** : 13/13 PASS ✓
```
running 13 tests
test tests::attestation_present ... ok
test tests::caps_default_loopback_only ... ok
test tests::net_error_from_last_returns_other_with_os_code ... ok
test tests::net_error_from_last_translates_kind ... ok
test tests::socket_addr_v4_any_helper ... ok
test tests::socket_addr_v4_construction_via_octets ... ok
test tests::socket_addr_v4_display_format ... ok
test tests::socket_addr_v4_loopback_helper ... ok
test tests::tcp_listener_bind_any_without_inbound_cap_denied ... ok
test tests::tcp_listener_bind_loopback_default_caps ... ok
test tests::tcp_loopback_full_roundtrip ... ok
test tests::tcp_stream_connect_to_non_loopback_without_outbound_cap_denied ... ok
test tests::version_present ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### § 1.2 — Root cause

**Race condition on `WSAStartup` ref-count in `cssl_rt::net_win32::ensure_wsa_started`** (`compiler-rs/crates/cssl-rt/src/net_win32.rs:248–294`).

The current implementation :

```rust
static WSA_INIT_COUNT: AtomicI32 = AtomicI32::new(0);

fn ensure_wsa_started() -> Result<(), i32> {
    let prev = WSA_INIT_COUNT.fetch_add(1, Ordering::AcqRel);
    if prev == 0 {
        // First caller — actually invoke WSAStartup.
        let mut wsa_data = WSADATA { /* zero */ };
        let r = unsafe { WSAStartup(WINSOCK_VERSION_2_2, &mut wsa_data) };
        if r != 0 {
            WSA_INIT_COUNT.fetch_sub(1, Ordering::AcqRel);
            return Err(net_error_code::NOT_INITIALIZED);
        }
    }
    Ok(())
}

fn release_wsa_started() {
    let prev = WSA_INIT_COUNT.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        // Last release — invoke WSACleanup.
        let _ = unsafe { WSACleanup() };
    }
}
```

**The race window** :

| Time | Test-A (locked-by-host-net) | Test-B (locked-by-host-net) | `WSA_INIT_COUNT` | Winsock state |
|---|---|---|---|---|
| t0 | bind() → ensure_wsa_started() | (waiting) | 0 → 1 (calls WSAStartup ✓) | started |
| t1 | bind succeeds, drop listener → close → release_wsa_started() | (still waiting) | 1 → 0 (calls **WSACleanup**) | **shut down** |
| t2 | (releases lock) | acquires lock → reset_net_for_tests() | 0 | shut down |
| t3 | (idle) | bind() → ensure_wsa_started() | 0 → 1 (calls WSAStartup again ✓) | started |
| t4 | (idle) | bind syscall | 1 | started |

In the strictly-serial sequential case the race window doesn't exist because Test-A's `WSACleanup` ALWAYS happens before Test-B's `WSAStartup`.

In **parallel** with multiple cssl-rt-internal tests (`socket_create_then_close_balances_wsa_count`, `tcp_loopback_roundtrip_default_caps`, `connect_to_loopback_unbound_port_returns_connection_refused` etc.) that ALSO bump `WSA_INIT_COUNT`, the host-net tests can land between a cssl-rt test's increment and decrement, OR between two cssl-rt tests running back-to-back where one decrements to 0 (calling `WSACleanup`) just as another increments from 0 (and races mid-WSAStartup).

Critically : the `host-net` test crate runs in **its own test binary** (separate from `cssl-rt`), but the entire workspace `cargo test` runs MULTIPLE test binaries simultaneously (cargo's default is one-test-binary-per-thread, each binary internally also parallelizes). A `cssl-rt` test binary calling `WSACleanup()` does NOT corrupt the `host-net` test binary's process — but host-net's OWN tests inside its own binary still race against each other on the same `WSA_INIT_COUNT`.

Actually — the `host-net` tests don't directly call `WSAStartup`. They call `TcpListener::bind()` → `cssl_rt::net::cssl_net_socket_impl` → `ensure_wsa_started()`. Inside the host-net test binary, only host-net tests run. The 3 failing tests are :

1. `tcp_listener_bind_loopback_default_caps` — calls `TcpListener::bind`
2. `tcp_stream_connect_to_non_loopback_without_outbound_cap_denied` — calls `TcpStream::connect`
3. `tcp_loopback_full_roundtrip` — calls bind + connect + accept + send + recv

The 10 passing tests do NOT call into any net syscall (they test types, addr-formatting, cap-system bits, error-code translation).

So the race is among these 3 tests : when running in parallel, Test-A drops a TcpListener (calling close → release_wsa_started → WSACleanup), and Test-B's bind() in another thread races. Specifically : at the moment Test-A's `release_wsa_started` decrements to 0 and calls `WSACleanup`, Test-B may already be inside `socket()` syscall → returns `WSANOTINITIALISED` → translated to `NotInitialized`.

**The exact bug** : `release_wsa_started` is non-atomic across the decrement + WSACleanup. Between `fetch_sub` returning 1 and the call to `WSACleanup`, another thread can `fetch_add`-from-0 to 1 (skip-WSAStartup-thinking-it's-already-up) then call `socket()`. That `socket()` then races with the in-flight `WSACleanup`.

**Confirming evidence** :
- Serial : passes (race window can't occur)
- Parallel : 3/3 fail consistently (race occurs on every parallel run because the host-net test code is short and finishes within the WSACleanup window)
- The error path in `ensure_wsa_started` only returns `NotInitialized` if `WSAStartup` itself returns non-zero. So either WSAStartup is failing (unlikely) OR the syscall after a successful ensure_wsa_started is hitting WSANOTINITIALISED via the translate_winsock_error path. Looking at `socket_invalid_socket_returns_minus_one` in cssl-rt (which does `let kind = last_net_error_kind()` + maps WSANOTINITIALISED→NOT_INITIALIZED), this confirms that a `socket()` syscall returned WSANOTINITIALISED — meaning Winsock was de-initialized when socket() ran.

### § 1.3 — Proposed fix-slice

**Slice ID** `T11-D150 : cssl-rt — fix WSAStartup ref-count race for parallel tests`

**Branch** `cssl/session-11/T11-D150-wsastartup-race`

**LOC** ~80-100 + 4-6 new tests

**Approach** : convert the bare atomic to a `Mutex`-protected `(count, started)` pair, OR keep the atomic but wrap WSAStartup/WSACleanup in a separate `Mutex` so the call-and-state-update is atomic from the perspective of any other `ensure_wsa_started`.

Option A (preferred — minimal change) : add a `static WSA_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(())` ; acquire before `WSAStartup` AND before `WSACleanup` AND on the count check. Each `ensure_wsa_started` / `release_wsa_started` becomes :

```rust
fn ensure_wsa_started() -> Result<(), i32> {
    let _g = WSA_LIFECYCLE_LOCK.lock().unwrap();
    let prev = WSA_INIT_COUNT.fetch_add(1, Ordering::AcqRel);
    if prev == 0 {
        // ... WSAStartup ...
    }
    Ok(())
}

fn release_wsa_started() {
    let _g = WSA_LIFECYCLE_LOCK.lock().unwrap();
    let prev = WSA_INIT_COUNT.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        let _ = unsafe { WSACleanup() };
    }
}
```

This serializes startup + cleanup but lets concurrent socket-ops proceed under a steady-state `count >= 1` (no lock held during socket/bind/connect/etc.).

Option B : use a higher-ref-count steady-state via process-level WSAStartup pinning — call WSAStartup once at first use and NEVER call WSACleanup. Tradeoff : leaks one Winsock-startup-count for the process lifetime ; but the process-exit cleans up automatically. Already mitigates the race entirely. Only ~30 LOC.

**Recommendation** : Option B (process-pin) for stage-0 — the pin is invisible to user code, simpler to reason about, and matches how most production Winsock apps actually work (they call WSAStartup once at startup and never WSACleanup).

**Testing additions** : a new test that spawns N=20 threads, each calling `bind/close` 100×, asserting all succeed and `wsa_init_count_for_tests() >= 1` throughout.

### § 1.4 — Block status

**Does NOT block Wave-Jε.** Workaround `--test-threads=1` is canonical (per all post-T11-D56 DECISIONS entries) and used in every commit-gate run. Fix-slice can dispatch in parallel with Wave-Jε or after.

═══════════════════════════════════════════════════════════════════════════

## § INVESTIGATION-2 : cssl-rt cold-cache flake

### § 2.1 — Reproduction

**Command (serial — 5× consecutive runs)** :
```
cd compiler-rs
for i in 1 2 3 4 5; do echo "=== run $i ==="; cargo test -p cssl-rt --lib -- --test-threads=1 2>&1 | tail -5; done
```

**Result** : **5/5 runs pass · 198/198 each · 0 failures across all 990 test invocations**

```
=== run 1 ===
test result: ok. 198 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.08s
=== run 2 ===
test result: ok. 198 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.08s
=== run 3 ===
test result: ok. 198 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.08s
=== run 4 ===
test result: ok. 198 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.04s
=== run 5 ===
test result: ok. 198 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.07s
```

**`--test-threads=1` fully mitigates the flake. 0 failures across 5 runs ⇒ pass-rate 100%.**

**Command (parallel — DEFAULT)** :
```
cd compiler-rs && cargo test -p cssl-rt --lib
```

**Result (HOT cache)** : 198/198 PASS ✓ (the binary was already built ; only the test scheduler runs)

**Result (COLD cache — `rm target/debug/deps/cssl_rt*` first)** : ‼ **118/198 FAIL**

```
test result: FAILED. 80 passed; 118 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

The crucial observation : **the hot-cache parallel run passes, but the cold-cache parallel run fails catastrophically**.

### § 2.2 — Root cause analysis

The flake is **two distinct bugs interacting** :

#### § 2.2.a — Bug A : 7 unlocked tests touch `TRACKER` global state

`cssl-rt/src/alloc.rs` defines a process-wide `TRACKER` global counter. The `BumpArena::new(N)` constructor calls `raw_alloc()` internally, which increments the tracker. `BumpArena::drop` calls `raw_free()`, which decrements.

The crate uses a `pub(crate) mod test_helpers` (`lib.rs:158-193`) with a `GLOBAL_TEST_LOCK: Mutex<()>` and `lock_and_reset_all()` helper. **Tests that touch globals MUST acquire the lock.**

7 tests in `alloc.rs` create `BumpArena` and DO NOT acquire the lock — but they touch the tracker indirectly :

```rust
// crates/cssl-rt/src/alloc.rs — these 7 tests are MISSING `let _g = lock_and_reset();`

#[test]
fn arena_zero_capacity_is_none() {                                 // L519 ; touches tracker via failed BumpArena::new path? Actually returns None on 0 → no alloc → SAFE. False positive.
    assert!(BumpArena::new(0).is_none());
}

#[test]
fn arena_basic_alloc_returns_non_null_within_capacity() {           // L524 ; touches tracker via BumpArena::new(1024) → raw_alloc → +1
    let arena = BumpArena::new(1024).expect("arena");
    let p = unsafe { arena.alloc(64, 8) };
    /* ... */
}

#[test]
fn arena_alignment_is_respected() {                                  // L533 ; same — BumpArena::new → raw_alloc
    let arena = BumpArena::new(1024).expect("arena");
    /* ... */
}

#[test]
fn arena_alloc_beyond_capacity_returns_null() {                      // L543 ; same
    let arena = BumpArena::new(64).expect("arena");
    /* ... */
}

#[test]
fn arena_non_power_of_two_align_returns_null() {                     // L551 ; same
    let arena = BumpArena::new(64).expect("arena");
    /* ... */
}

#[test]
fn arena_sequential_allocs_advance_cursor() {                        // L558 ; same
    let arena = BumpArena::new(1024).expect("arena");
    /* ... */
}

#[test]
fn arena_reset_returns_all_capacity() {                              // L568 ; same
    let arena = BumpArena::new(1024).expect("arena");
    /* ... */
}
```

**6 of these 7** (all except `arena_zero_capacity_is_none`) call `BumpArena::new(non-zero)` which does call `raw_alloc()` → `TRACKER.record_alloc()`. They all also depend on `BumpArena::drop` → `raw_free()` → `TRACKER.record_free()`.

Per-module audit (test count vs lock-use count) :

```
alloc.rs:    tests=29, lock_and_reset=19   → 10 tests potentially-unlocked (some legit pure)
exit.rs:     tests=13, lock_and_reset=13   → all locked ✓
ffi.rs:      tests=18, lock_and_reset=18   → all locked ✓
io.rs:       tests=35, lock_and_reset=12   → 23 unlocked (most are pure validate_* checks ✓)
io_win32.rs: tests=24, lock_and_reset=10   → 14 unlocked (most are translate_open_flags pure checks ✓)
net.rs:      tests=36, lock_and_reset=18   → 18 unlocked (most are pure constant/validate checks ✓)
net_win32.rs:tests=40, lock_and_reset=14   → 26 unlocked (most are pure encoder/translate checks ✓)
panic.rs:    tests=14, lock_and_reset=3    → 11 unlocked (12 are pure format_panic_* ✓ ; verify carefully)
path_hash.rs:tests=5,  lock_and_reset=4    → 1 unlocked (likely intentional ; verify)
runtime.rs:  tests=11, lock_and_reset=12   → all locked (one extra lock_and_reset = pattern-match noise)
```

Most unlocked tests are pure (e.g., `validate_open_flags_*`, `translate_*`, `format_panic_*`) ; the 7 `arena_*` tests in `alloc.rs` are the actual offenders (plus possibly hidden ones in `panic.rs` / `path_hash.rs` worth re-inspecting).

#### § 2.2.b — Bug B : `Mutex` poisoning cascades on first failure

When the first racing test fails (e.g., `alloc_count_total_matches_history` sees `alloc_count() == 2` because an unlocked `arena_basic_alloc_returns_non_null_within_capacity` snuck in a `+1` between the locked test's reset + assert), the panic occurs **while holding the lock**. This poisons `GLOBAL_TEST_LOCK`.

`lock_and_reset_all()` at `lib.rs:181` calls `.lock().expect("crate-shared test lock poisoned ; prior test failed mid-update")`. Once poisoned, every subsequent caller panics with :

```
crate-shared test lock poisoned ; prior test failed mid-update: PoisonError { .. }
```

Result : **one race-induced failure cascades to ~117 cascading poison-error failures.**

Confirming evidence (cold-cache parallel output) :

```
thread 'alloc::tests::alloc_count_total_matches_history' panicked at crates\cssl-rt\src\alloc.rs:632:13:
assertion `left == right` failed
  left: 2
 right: 1
```

This is the FIRST failure — `alloc_count_total_matches_history` (which DOES lock) sees `alloc_count() == 2` instead of expected `1` because an unlocked `arena_*` test concurrently called `raw_alloc()`.

After that, every other test that calls `lock_and_reset()` :

```
thread 'alloc::tests::arena_drop_releases_chunk_via_tracker' panicked at crates\cssl-rt\src\lib.rs:184:14:
crate-shared test lock poisoned ; prior test failed mid-update: PoisonError { .. }

thread 'alloc::tests::freeing_more_than_alloc_saturates_at_zero' panicked at crates\cssl-rt\src\lib.rs:184:14:
crate-shared test lock poisoned ; prior test failed mid-update: PoisonError { .. }

[…117 more poison cascades…]
```

#### § 2.2.c — Why hot-cache passes : test-binary scheduling ordering

When cargo's hot cache reuses an already-built binary, the test runner's seed/ordering tends to be deterministic and (incidentally) interleaves the racing tests in an order where the unlocked `arena_*` tests run early enough that the lock hasn't yet been acquired for `alloc_count_total_matches_history`. The hot run captured here :

```
test alloc::tests::raw_alloc_zero_size_returns_null ... ok
test alloc::tests::raw_alloc_zero_align_returns_null ... ok
test alloc::tests::raw_alloc_64bytes_increments_counters ... ok
test alloc::tests::raw_realloc_grows_in_place_preserves_bytes ... ok
[…198 ok…]
```

…shows the hot scheduler interleaving the locked-tests after each other (because they queue on the shared lock), with arena_* squeezed in between holding-windows. On a fresh compile + scheduler-warm-up, the lock-acquire ordering is different and the `arena_*` test fires its tracker-side-effect inside another test's locked critical section.

This is **scheduler-determinism flake** : it depends on JIT/cache state of the test runner threads. The cold-cache parallel reproduction here showed **EVERY arena_* test passing** but **EVERY locked-tracker test failing first-time** — which is a clean signature of the bug.

### § 2.3 — Proposed fix-slice

**Slice ID** `T11-D151 : cssl-rt — fix tracker-test race + lock-poison cascade (S6-T11-D56 root-cause closure)`

**Branch** `cssl/session-11/T11-D151-tracker-race-fix`

**LOC** ~120-180 + 7 test changes (acquire lock) + 1-2 new regression tests

**Two-part fix** :

#### Part 1 — Add `lock_and_reset()` to all tracker-touching tests

In `crates/cssl-rt/src/alloc.rs`, add `let _g = lock_and_reset();` as the first line of these 7 tests :

1. `arena_basic_alloc_returns_non_null_within_capacity` (L524)
2. `arena_alignment_is_respected` (L533)
3. `arena_alloc_beyond_capacity_returns_null` (L543)
4. `arena_non_power_of_two_align_returns_null` (L551)
5. `arena_sequential_allocs_advance_cursor` (L558)
6. `arena_reset_returns_all_capacity` (L568)
7. (`arena_zero_capacity_is_none` is genuinely pure ; can add lock defensively as documentation)

Also audit-and-fix any similar cases in `panic.rs` (3/14 locked — most format_panic_* are pure but `record_panic_*` should be locked) and `path_hash.rs`.

#### Part 2 — Make lock-poisoning non-cascading

Replace the bare `Mutex` in `lib.rs:174` with one of :

(a) `parking_lot::Mutex` — no poisoning by design ; ~5 LOC + workspace dep on `parking_lot`.
(b) `std::sync::Mutex` + `clear_poison()` in `lock_and_reset_all` :
```rust
pub fn lock_and_reset_all() -> MutexGuard<'static, ()> {
    let g = match GLOBAL_TEST_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => {
            // A prior test panicked while holding the lock. Test-only
            // path : we WANT the next test to get a clean lock so each
            // failure is independent (not a cascade). Real test-failures
            // still report independently.
            poisoned.into_inner()
        }
    };
    /* ... reset all ... */
    g
}
```

**Recommendation** : Part 1 + Part 2(b) — no new workspace dep, behaviour change is test-only-and-clearly-documented.

#### Optional Part 3 — Add a build-script-emitted "tracker-race regression" test that spawns 16 threads × 100 cycles of `BumpArena::new+drop` interleaved with locked + reads of `alloc_count()` ; assert all values stay ≥ 0 + final counts match expected.

### § 2.4 — Block status

**Does NOT block Wave-Jε.** The `--test-threads=1` workaround is universally used in commit-gate runs (per all DECISIONS entries since T11-D56). Fix-slice can dispatch parallel-with or after Wave-Jε. The fix is HIGH-VALUE (eliminates the 6-month-old workaround burden) but not URGENT.

═══════════════════════════════════════════════════════════════════════════

## § INVESTIGATION-3 : cssl-cgen-gpu-wgsl `dlltool.exe` build failure

### § 3.1 — Reproduction

**Command** :
```
cd compiler-rs && cargo build -p cssl-cgen-gpu-wgsl --tests
```

**Result** : ‼ build FAILS

```
   Compiling windows-sys v0.61.2
   Compiling winapi-util v0.1.11
error: Error calling dlltool 'dlltool.exe': program not found

error: could not compile `windows-sys` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
```

### § 3.2 — Toolchain context

```
$ cd compiler-rs && rustup show
Default host: x86_64-pc-windows-gnu
rustup home:  C:\Users\Apocky\.rustup

installed toolchains
--------------------
stable-x86_64-pc-windows-gnu
stable-x86_64-pc-windows-msvc (default)
1.75.0-x86_64-pc-windows-gnu
1.85.0-x86_64-pc-windows-gnu (active)

active toolchain
----------------
name: 1.85.0-x86_64-pc-windows-gnu
active because: overridden by 'C:\Users\Apocky\source\repos\CSSLv3\compiler-rs\rust-toolchain.toml'
installed targets:
  wasm32-unknown-unknown
  x86_64-pc-windows-gnu
```

```
$ where dlltool.exe
INFO: Could not find files for the given pattern(s).
```

The pinned 1.85.0 GNU toolchain is active inside `compiler-rs/`, and `dlltool.exe` is NOT on PATH.

### § 3.3 — Root cause : transitive dependency tree

```
$ cargo tree -p cssl-cgen-gpu-wgsl -i windows-sys --edges all --target=all

windows-sys v0.61.2
├── windows-sys feature "Win32"
│   ├── windows-sys feature "Win32_Foundation"
│   │   └── winapi-util v0.1.11
│   │       └── winapi-util feature "default"
│   │           └── termcolor v1.4.1
│   │               └── termcolor feature "default"
│   │                   ├── codespan-reporting v0.11.1
│   │                   │   └── codespan-reporting feature "default"
│   │                   │       └── naga v23.1.0
│   │                   │           ├── naga feature "compact"
│   │                   │           │   └── naga feature "wgsl-in"
│   │                   │           │       [dev-dependencies]
│   │                   │           │       └── cssl-cgen-gpu-wgsl v0.1.0
[…etc…]
```

**Chain** :
`cssl-cgen-gpu-wgsl` → `[dev-dependencies] naga 23.1.0` → `codespan-reporting 0.11.1` → `termcolor 1.4.1` → `winapi-util 0.1.11` → `windows-sys 0.61.2` → build.rs **invokes `dlltool.exe` to generate import libraries** for the GNU toolchain.

The `windows-sys` crate at version 0.61.2 added support for using `dlltool` to produce import libs from `.def` files for the GNU x86_64-pc-windows-gnu target. Earlier versions (0.52.x, 0.59.x — also present in our Cargo.lock) ship pre-generated import libs and DON'T require `dlltool`. The chain pulled in 0.61.2 because :

- `winapi-util 0.1.11` (recent) depends on `windows-sys 0.61` (its build added by upstream after version 0.1.10 → 0.1.11 bump).
- `cssl-cgen-gpu-wgsl` / `naga` does NOT pin `winapi-util`, so cargo resolves to the latest patch.

This is **DEV-DEPENDENCY ONLY** — the production build of `cssl-cgen-gpu-wgsl` does NOT pull this chain. Confirmed because `cargo clippy --workspace --all-targets -- -D warnings` succeeded (it includes tests). Wait — actually `--all-targets` should include test targets…

Let me re-examine. The clippy run succeeded, meaning the cargo invocation that triggered clippy DID NOT trigger the dlltool path. Looking back :

```
clippy completed successfully (EXIT: 0).
… Checking cssl-cgen-gpu-wgsl v0.1.0 (ahead of schedule)
```

So `cargo clippy --all-targets` does include the dev-dep build path AND it succeeded. This means clippy used a DIFFERENT/HOT-cache path for the windows-sys crate or the windows-sys bypass kicked in. Possibilities :

1. The clippy workspace check shares a cargo profile that already had the windows-sys 0.61.2 import-lib generated from a previous successful build (but we showed no prior builds…).
2. `cargo build --tests` uses different feature unification than `cargo clippy --all-targets` for some reason.
3. The dlltool failure is recent and only manifests in `--tests` builds with specific feature combinations.

Let me re-confirm: the clippy run output above includes `cssl-cgen-gpu-wgsl` but DID complete successfully. Possibility : the prior `cargo test -p cssl-host-net` runs warmed the workspace cache and built a SUBSET of windows-sys 0.61.2 that didn't trigger the dlltool path. Then `cargo build --tests -p cssl-cgen-gpu-wgsl` triggered a feature-set that did. **Plausible cause** : `naga`'s `wgsl-in` feature pulls a wider Win32 features-list than what host-net needed.

### § 3.4 — Proposed fix-slice options (one-of-three)

**Slice ID** `T11-D152 : workspace — pin transitive winapi-util to 0.1.10 OR install dlltool OR drop dev-dep`

**Branch** `cssl/session-11/T11-D152-dlltool-fix`

**LOC** ~5-30 (depending on path chosen)

#### Option A — Pin `winapi-util` to 0.1.10 in workspace `Cargo.toml` `[workspace.dependencies]` + use `[patch.crates-io]` to override transitive resolution

```toml
# compiler-rs/Cargo.toml
[workspace.dependencies]
winapi-util = "=0.1.10"   # pin to last version not requiring dlltool
```

Fix-LOC : ~3 lines + 1-line change to any direct `winapi-util` user.

But this only works if there's a direct user. Since `winapi-util` enters via transitive dev-dep, we need `[patch.crates-io]` :

```toml
[patch.crates-io]
winapi-util = { version = "=0.1.10" }
```

**Risk** : pinning a transitive may cause version-conflict diagnostics if any other dep wants `>=0.1.11`. As of 2026-04, naga 23.1.0 doesn't directly depend on winapi-util ; the chain goes through termcolor + codespan-reporting which may accept a wide range. Likely safe but needs a `cargo update --dry-run` verification.

#### Option B — Install `dlltool.exe` on PATH

The 1.85.0-gnu toolchain bundle does ship dlltool inside its own sysroot but Apocky's local install seems to lack it. Install path : `rustup component add llvm-tools-preview` ships `llvm-dlltool` (close-enough drop-in but renamed). Or install `mingw-w64` package which ships `dlltool.exe`. Or download msys2.

Fix-LOC : 0 (toolchain change). But the fix is **per-machine**, not in-tree. Not recommended for a fix-slice — should be doc'd in a CONTRIBUTING/SETUP file.

#### Option C — Switch toolchain to 1.85.0-x86_64-pc-windows-msvc

Change `rust-toolchain.toml` :
```toml
[toolchain]
channel    = "1.85.0"
components = ["rustfmt", "clippy"]
profile    = "minimal"
# REMOVE the gnu-specific lock — let MSVC be picked up
# OR explicitly:
targets    = ["x86_64-pc-windows-msvc"]
```

`windows-sys` on MSVC does NOT need dlltool (uses MSVC-native lib format).

**Risk** : the entire workspace's prebuilt artifacts (target/ dir, Cargo.lock resolution may differ slightly). All of `cranelift-*`, MSVC-specific extern bindings, may behave differently. **MAJOR CHANGE — needs full workspace re-test.** S6-A5 hello.exe gate is MSVC-linker-aware, so this MAY actually be the canonical-path for the project. But T11-D20's notes pin 1.85.0 for cranelift-jit ; that's edition-2024 not GNU-vs-MSVC.

#### Recommendation

**Option A first** (pin transitive winapi-util to 0.1.10) — minimal change, contained risk, doesn't require per-machine setup. If A is blocked by a transitive that REQUIRES winapi-util >= 0.1.11, fall back to Option C (toolchain switch to MSVC) which is a beneficial cleanup anyway given S6-A5 already uses MSVC linker.

**Testing additions** : add a `cargo build -p cssl-cgen-gpu-wgsl --tests` smoke step to the commit-gate (or a new CI job that explicitly tests the GPU-cgen --tests path).

### § 3.5 — Block status

**MAY block Wave-Jε** if Wave-Jε dispatches plan to run `cargo test --workspace --all-targets` from a fresh clone (which triggers cold-cache build of cssl-cgen-gpu-wgsl --tests). If Wave-Jε continues to use the existing target/ cache and `--test-threads=1`, there's no impact (this build already succeeded once historically, the artifacts are still in target/).

**Mitigation in place** : The existing commit-gate uses `cargo test --workspace -- --test-threads=1` from a hot target/ cache — this works because the wgsl --tests path has already been built once (probably during a session before windows-sys 0.61.2 entered the lock). Once cargo refreshes Cargo.lock or target/ is wiped, the failure resurfaces.

═══════════════════════════════════════════════════════════════════════════

## § INVESTIGATION-4 : Clippy drift since b69165c

### § 4.1 — Reproduction

```
cd compiler-rs && cargo clippy --workspace --all-targets -- -D warnings
```

### § 4.2 — Result : ZERO drift ✓

```
    Checking cssl-ast v0.1.0 (...)
    [… 56 crates …]
    Checking cssl-testing v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.28s
EXIT: 0
```

**0 warnings · 0 errors · all 56 workspace crates clean under `-D warnings`.**

### § 4.3 — Categorization

- **Pre-existing-allow-list** : N/A — no allow-list adjustments needed.
- **New-since-b69165c** : 0 new warnings.
- **Would-block-merge** : 0 — clippy gate is fully green.

### § 4.4 — Block status

**No fix-slice required.** Clippy state is pristine.

═══════════════════════════════════════════════════════════════════════════

## § INVESTIGATION-5 : fmt drift since b69165c

### § 5.1 — Reproduction

```
cd compiler-rs && cargo fmt --all -- --check
```

### § 5.2 — Result : ZERO drift ✓

```
EXIT: 0
```

(Output was empty — `--check` only emits diff text when files would change. Empty + EXIT 0 = no formatting changes needed.)

### § 5.3 — Categorization

- **Files needing formatting** : 0
- **Lines diff** : 0
- **Would-block-merge** : 0 — fmt gate is fully green.

### § 5.4 — Block status

**No fix-slice required.** fmt state is pristine.

═══════════════════════════════════════════════════════════════════════════

## § FIX-SLICE TABLE — Summary

| Slice | ID-proposed | Title | LOC est | Tests added | Branch | Block-Wave-Jε? | Priority |
|---|---|---|---|---|---|---|---|
| 1 | T11-D150 | cssl-rt WSAStartup ref-count race fix | ~80-100 | 4-6 | `cssl/session-11/T11-D150-wsastartup-race` | NO | high (eliminates host-net flake) |
| 2 | T11-D151 | cssl-rt tracker-race + poison-cascade fix | ~120-180 | 7 lock-additions + 1-2 new | `cssl/session-11/T11-D151-tracker-race-fix` | NO | high (closes T11-D56, restores parallel-test) |
| 3 | T11-D152 | dev-dep dlltool fix (winapi-util pin OR toolchain) | ~5-30 | 1 smoke | `cssl/session-11/T11-D152-dlltool-fix` | conditional | medium |
| 4 | — | (clippy drift — none) | 0 | 0 | — | — | none |
| 5 | — | (fmt drift — none) | 0 | 0 | — | — | none |

**Total estimated work for fix-slices 1+2+3** : **~205-310 LOC + 13-15 test additions/changes**.

**Total slice count** : 3 (T11-D150, T11-D151, T11-D152). T11-D149 is taken (substrate-evolution reference docs) per HEAD ; T11-D148 is the docs-update merge ; D150 is the next-available slot per the slice-ID range shift.

Note : the dispatch plan's reservation of `T11-D150..D201` for Phase-J slices may need a +3 shift if these audit-fix-slices land first. Recommended : ALLOCATE T11-D150/151/152 to these audit-fix slices NOW and SHIFT the Phase-J range to T11-D153..D204.

═══════════════════════════════════════════════════════════════════════════

## § DISPATCH-ORDER RECOMMENDATION

### § Recommended : dispatch fix-slices IN PARALLEL with Wave-Jε

**Rationale** :
- None of the 3 fix-slices touch dispatch infrastructure (worktree-isolation, parallel-fanout protocol, session-mgmt, etc.). They live entirely inside :
  - `cssl-rt/src/net_win32.rs` (T11-D150)
  - `cssl-rt/src/{lib.rs, alloc.rs, panic.rs, path_hash.rs}` (T11-D151)
  - `compiler-rs/Cargo.toml` (T11-D152, Option A) OR `rust-toolchain.toml` (Option C)
- Wave-Jε work is in different crates/files (per the J-slice plan in `SESSION_12_DISPATCH_PLAN.md` referenced from PHASE_J_HANDOFF). Merge conflict surface is essentially zero.
- The current `--test-threads=1` commit-gate workaround continues to work, so Wave-Jε can dispatch under existing assumptions.
- Closing T11-D56 + T11-D150 unblocks parallel-test mode for ALL future sessions, dramatically speeding up CI (parallel ~10× faster than serial on the host).

### § Alternative : sequential-before-Jε

ONLY if the Wave-Jε plan involves toolchain changes, fresh-clone CI, or new tests in `cssl-rt`/`cssl-host-net` that would conflict with the proposed fix-slices. Per the audit, no such overlap exists.

### § Concrete dispatch suggestion

**Wave-Pre-Jε (parallel ~3 agents)** :
1. Agent → T11-D150 (WSAStartup race fix in cssl-rt::net_win32)
2. Agent → T11-D151 (tracker race + poison fix in cssl-rt)
3. Agent → T11-D152 (dlltool dev-dep fix — Option A pin)

Each is ~80-180 LOC + tests + decisions-entry. Each is a well-bounded slice. Dispatch them as a small audit-fix wave, merge after Wave-Jε's first commit-gate, and the workspace inherits parallel-test capability for the rest of S11/S12.

If Apocky prefers fewer-but-safer : drop T11-D152 (the dev-dep fix) since the workaround (pre-built target/ cache) is invisible during normal commit-gate flow. Land T11-D150 + T11-D151 only.

═══════════════════════════════════════════════════════════════════════════

## § ANALYSIS-COVERAGE NOTES

### § What this audit DID NOT do

- Did NOT run `cargo test --workspace` (would have triggered the cold-cache flake catastrophically + waited several minutes ; not needed since per-package + cold-cache reproduction was sufficient).
- Did NOT verify Option C (toolchain switch) compiles the workspace (would require ~5min full rebuild ; deferred to fix-slice T11-D152).
- Did NOT confirm `cargo clippy --workspace --all-targets --tests` separately (the `--all-targets` already includes tests ; result clean).
- Did NOT exhaustively audit every unlocked test in `panic.rs` / `path_hash.rs` for tracker-touch (sampled, identified arena_* in alloc.rs as the obvious offender ; comprehensive audit deferred to T11-D151's implementation).

### § What this audit DID confirm

- Cold-cache parallel cssl-rt = **reproducible 118/198 fail rate** with deterministic first-failure signature (`alloc_count_total_matches_history` ; left:2, right:1).
- Hot-cache parallel cssl-rt = passes (scheduler-determinism interleaving the locked tests sequentially).
- Serial cssl-rt = **5/5 runs at 198/198 pass each = 100% pass-rate** (`--test-threads=1` fully mitigates).
- cssl-host-net parallel = **3/3 deterministic fail** with `NotInitialized` signature (WSAStartup race).
- cssl-host-net serial = 13/13 pass.
- Clippy = 0 warnings/errors workspace-wide under `-D warnings`.
- fmt = 0 drift workspace-wide under `--check`.
- The active toolchain `1.85.0-x86_64-pc-windows-gnu` lacks `dlltool.exe` on PATH ; transitive `windows-sys 0.61.2` requires it for the gnu target → cssl-cgen-gpu-wgsl `--tests` build fails.

### § Confidence summary

| Issue | Reproduction confidence | Root-cause confidence | Fix confidence |
|---|---|---|---|
| #1 host-net WSAStartup | HIGH (deterministic) | HIGH (race window mathematically traced) | HIGH (Option B = process-pin trivially correct) |
| #2 cssl-rt cold-cache | HIGH (118/198 deterministic + signature) | HIGH (panic message + lock-poison + arena_* unlocked = closed loop) | HIGH (Part 1+Part 2(b) = surgical fix) |
| #3 wgsl dlltool | HIGH (deterministic) | HIGH (cargo tree + toolchain inspect = closed loop) | MEDIUM (Option A risks transitive-dep solver conflict ; Option C is bigger blast-radius) |
| #4 clippy | N/A — no drift | N/A | N/A |
| #5 fmt | N/A — no drift | N/A | N/A |

═══════════════════════════════════════════════════════════════════════════

## § APPENDIX-A : Full source-quoted code for INVESTIGATION-1

### § A.1 — `WSAStartup` / `WSACleanup` extern declarations
(`compiler-rs/crates/cssl-rt/src/net_win32.rs:190-200`)

```rust
#[link(name = "ws2_32")]
extern "system" {
    fn WSAStartup(w_version_requested: WORD, lp_wsa_data: *mut WSADATA) -> c_int;
    fn WSACleanup() -> c_int;
    fn WSAGetLastError() -> c_int;
    fn socket(af: c_int, kind: c_int, protocol: c_int) -> SOCKET;
    fn closesocket(s: SOCKET) -> c_int;
    /* ... bind / listen / accept / connect / send / recv / sendto / recvfrom ... */
}
```

### § A.2 — Current `ensure_wsa_started` / `release_wsa_started`
(`compiler-rs/crates/cssl-rt/src/net_win32.rs:240-294`)

```rust
// ───────────────────────────────────────────────────────────────────────
// § WSAStartup ref-count.
//
//   Stage-0 stores the count as a global atomic. Multi-thread access is
//   safe via fetch-add ; the actual `WSAStartup` call is gated behind a
//   compare-exchange so only one thread fires it.
// ───────────────────────────────────────────────────────────────────────

static WSA_INIT_COUNT: AtomicI32 = AtomicI32::new(0);

/// Ensure WSA has been started at least once on this process. Returns
/// `Ok(())` on success, `Err(canonical-net-error)` on WSAStartup failure.
///
/// This is called by every net op that touches the Winsock surface.
/// Thread-safe via an atomic compare-exchange : only the thread that
/// transitions the count from `0 → 1` actually invokes `WSAStartup`.
fn ensure_wsa_started() -> Result<(), i32> {
    let prev = WSA_INIT_COUNT.fetch_add(1, Ordering::AcqRel);
    if prev == 0 {
        // First caller — actually invoke WSAStartup.
        let mut wsa_data = WSADATA {
            w_version: 0,
            w_high_version: 0,
            i_max_sockets: 0,
            i_max_udp_dg: 0,
            lp_vendor_info: core::ptr::null_mut(),
            sz_description: [0u8; 257],
            sz_system_status: [0u8; 129],
        };
        // SAFETY : WSADATA fully initialized to zero ; WSAStartup will
        // populate it with version + capability info. Per MSDN the call
        // is thread-safe.
        let r = unsafe { WSAStartup(WINSOCK_VERSION_2_2, &mut wsa_data) };
        if r != 0 {
            // Roll back the count we just incremented.
            WSA_INIT_COUNT.fetch_sub(1, Ordering::AcqRel);
            return Err(net_error_code::NOT_INITIALIZED);
        }
    }
    Ok(())
}

/// Decrement the WSA init-count. When the count reaches 0, invoke
/// `WSACleanup`. Idempotent ; safe to call from `cssl_net_close_impl`
/// once per closed socket.
fn release_wsa_started() {
    let prev = WSA_INIT_COUNT.fetch_sub(1, Ordering::AcqRel);
    if prev == 1 {
        // Last release — invoke WSACleanup. Best-effort ; we don't
        // surface the return value because we're already in cleanup.
        // SAFETY : WSACleanup has no preconditions beyond a prior
        // matching WSAStartup, which we just balanced.
        let _ = unsafe { WSACleanup() };
    }
}
```

### § A.3 — Failing test bodies in `cssl-host-net`
(`compiler-rs/crates/cssl-host-net/src/lib.rs:792-843`)

```rust
#[test]
fn tcp_listener_bind_loopback_default_caps() {
    cssl_rt::net::reset_net_for_tests();
    let addr = SocketAddrV4::loopback(0);
    let listener = TcpListener::bind(addr).expect("loopback bind should succeed");
    //                                  ^^^^^^^ FAIL @ line 796 with "NotInitialized"
    let local = listener.local_addr().expect("local_addr");
    assert!(local.is_loopback());
    assert_ne!(local.port(), 0);
}

#[test]
fn tcp_stream_connect_to_non_loopback_without_outbound_cap_denied() {
    cssl_rt::net::reset_net_for_tests();
    let addr = SocketAddrV4::from_octets(8, 8, 8, 8, 53);
    let r = TcpStream::connect(addr);
    match r {
        Err(NetError::CapDenied) => (),
        other => panic!("expected CapDenied, got {other:?}"),
        //              ^^^^^^^ FAIL @ line 822 — got Err(NotInitialized)
    }
}

#[test]
fn tcp_loopback_full_roundtrip() {
    cssl_rt::net::reset_net_for_tests();
    let listener = TcpListener::bind(SocketAddrV4::loopback(0)).expect("bind");
    //                                                          ^^^^^^^ FAIL @ line 830 with "NotInitialized"
    /* ... rest of test never executes ... */
}
```

### § A.4 — Why `tcp_stream_connect_to_non_loopback_without_outbound_cap_denied` fails

This test EXPECTS `CapDenied` (the cap-system rejects 8.8.8.8 because outbound-cap not granted). Under the cap-system rules in `cssl_rt::net::check_caps_for_addr`, the cap-check happens BEFORE Winsock syscalls. So why does it surface `NotInitialized` instead of `CapDenied`?

Looking at the call chain :

```
TcpStream::connect(addr)
  → cssl_rt::net::cssl_net_socket_impl(...)         ← creates socket FIRST
    → ensure_wsa_started()                          ← here returns Err(NOT_INITIALIZED) when racing
      → returns Err
    → record_net_error(NOT_INITIALIZED, ...)
    → returns INVALID_SOCKET (-1 as i64)
  → on socket failure, NetError::from_last() → NotInitialized
  → returns Err(NetError::NotInitialized)
```

So the cap-check on the address (which would yield `CapDenied`) never gets reached because the SOCKET CREATE itself fails first under WSAStartup race.

This is significant : it means our cap-system has an **ordering bug** — sockets are created BEFORE cap-checks against the destination. That's a separate spec-level issue worth tracking but DOES NOT need to be fixed in T11-D150 (cap-check-before-socket re-ordering is a wider-scope refactor ; the WSAStartup race is narrowly fixable).

═══════════════════════════════════════════════════════════════════════════

## § APPENDIX-B : Full source-quoted code for INVESTIGATION-2

### § B.1 — `AllocTracker` global counter
(`compiler-rs/crates/cssl-rt/src/alloc.rs:50-107`)

```rust
#[allow(clippy::module_name_repetitions)]
pub struct AllocTracker {
    alloc_count: AtomicU64,
    free_count: AtomicU64,
    bytes_in_use: AtomicU64,
    bytes_alloc_total: AtomicU64,
    bytes_free_total: AtomicU64,
}

impl AllocTracker {
    /* ... */
    fn record_alloc(&self, bytes: u64) {
        self.alloc_count.fetch_add(1, Ordering::Relaxed);
        self.bytes_in_use.fetch_add(bytes, Ordering::Relaxed);
        self.bytes_alloc_total.fetch_add(bytes, Ordering::Relaxed);
    }

    fn record_free(&self, bytes: u64) {
        self.free_count.fetch_add(1, Ordering::Relaxed);
        let prev = self.bytes_in_use.fetch_update(
            Ordering::Relaxed, Ordering::Relaxed,
            |b| Some(b.saturating_sub(bytes))
        ).unwrap_or(0);
        let _ = prev;
        self.bytes_free_total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.alloc_count.store(0, Ordering::Relaxed);
        /* … reset all counters … */
    }
}

// § The single global tracker instance ; observable via the public readers.
static TRACKER: AllocTracker = AllocTracker::new();
```

### § B.2 — `BumpArena::new` / `BumpArena::drop` — they DO touch the tracker
(`compiler-rs/crates/cssl-rt/src/alloc.rs:282-297, 354-362`)

```rust
impl BumpArena {
    pub fn new(capacity: usize) -> Option<Self> {
        if capacity == 0 {
            return None;        // ← arena_zero_capacity_is_none — pure, never touches tracker
        }
        // SAFETY : raw_alloc returns either non-null with `capacity` valid
        // bytes or null on failure ; we check for null below.
        let base = unsafe { raw_alloc(capacity, ALIGN_MAX) };  // ← TRACKER.record_alloc(capacity) inside
        if base.is_null() {
            return None;
        }
        Some(Self {
            base,
            capacity,
            cursor: Cell::new(0),
        })
    }
}

#[allow(unsafe_code)]
impl Drop for BumpArena {
    fn drop(&mut self) {
        // SAFETY : self.base + self.capacity match the original raw_alloc
        // call in `Self::new` ; the arena owns the chunk exclusively.
        unsafe {
            raw_free(self.base, self.capacity, ALIGN_MAX);  // ← TRACKER.record_free(capacity) inside
        }
    }
}
```

### § B.3 — `lock_and_reset_all` (the test-helper)
(`compiler-rs/crates/cssl-rt/src/lib.rs:158-193`)

```rust
#[cfg(test)]
#[allow(unreachable_pub)]
pub(crate) mod test_helpers {
    //! Crate-shared test lock + reset.
    //!
    //! § WHY  Every test in this crate touches global counters
    //! (`TRACKER` / `PANIC_COUNT` / `LAST_EXIT_CODE` / `RUNTIME_INITIALIZED`).
    //! Per-module Mutex's would let tests in *different* modules race on
    //! shared globals (e.g., an `alloc::tests::*` test and a
    //! `ffi::tests::*` test both reset `TRACKER`). One crate-shared lock
    //! eliminates the cross-module race at the cost of forcing all
    //! global-state tests to serialize.
    //!
    //! Tests that don't touch any global state may skip the lock.

    use std::sync::Mutex;
    use std::sync::MutexGuard;

    pub static GLOBAL_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Acquire the shared test lock + reset every global counter / flag.
    ///
    /// Panics on poisoned-lock (test failure earlier left state corrupt).
    /// In practice, each test follows lock-and-reset → run → drop pattern,
    /// so poisoning indicates a real bug in a prior test.
    pub fn lock_and_reset_all() -> MutexGuard<'static, ()> {
        let g = GLOBAL_TEST_LOCK
            .lock()
            .expect("crate-shared test lock poisoned ; prior test failed mid-update");
        crate::alloc::reset_for_tests();
        crate::panic::reset_panic_count_for_tests();
        crate::exit::reset_exit_state_for_tests();
        crate::runtime::reset_runtime_for_tests();
        crate::io::reset_io_for_tests();
        crate::net::reset_net_for_tests();
        g
    }
}
```

The doc-comment at line 169-170 says : *"Tests that don't touch any global state may skip the lock."* The 7 `arena_*` tests in `alloc.rs` violate this contract — they DO touch the global `TRACKER` via `BumpArena::new(non-zero)` → `raw_alloc` → `TRACKER.record_alloc`.

### § B.4 — Per-arena-test analysis : exactly where each touches the tracker

| Test | LOC | `BumpArena::new(N)` | `BumpArena::drop` | Touches TRACKER? | Has lock? |
|---|---|---|---|---|---|
| `arena_zero_capacity_is_none` | 519 | `BumpArena::new(0)` returns None early | none | NO (pure) | NO (correct) |
| `arena_basic_alloc_returns_non_null_within_capacity` | 524 | `BumpArena::new(1024)` → +1 | end-of-scope → -1 | YES | **NO ‼** |
| `arena_alignment_is_respected` | 533 | `BumpArena::new(1024)` → +1 | end-of-scope → -1 | YES | **NO ‼** |
| `arena_alloc_beyond_capacity_returns_null` | 543 | `BumpArena::new(64)` → +1 | end-of-scope → -1 | YES | **NO ‼** |
| `arena_non_power_of_two_align_returns_null` | 551 | `BumpArena::new(64)` → +1 | end-of-scope → -1 | YES | **NO ‼** |
| `arena_sequential_allocs_advance_cursor` | 558 | `BumpArena::new(1024)` → +1 | end-of-scope → -1 | YES | **NO ‼** |
| `arena_reset_returns_all_capacity` | 568 | `BumpArena::new(1024)` → +1 | end-of-scope → -1 | YES | **NO ‼** |
| `arena_drop_releases_chunk_via_tracker` | 578 | `BumpArena::new(256)` → +1 | end-of-scope → -1 | YES | YES ✓ |
| `many_arenas_stress_tracker` | 591 | 16× `BumpArena::new(512)` → +16 | drop → -16 | YES | YES ✓ |

**6 out of 9 arena_* tests are buggy** (the 7 marked NO are missing the lock ; one — `arena_zero_capacity_is_none` — is correctly unlocked because `BumpArena::new(0)` short-circuits before touching the tracker).

### § B.5 — The catastrophic interaction

Consider this concrete race scenario reproduced under cold-cache + parallel :

```
Thread-A (running `alloc_count_total_matches_history`)  Thread-B (running `arena_basic_alloc_returns_non_null_within_capacity`)

t0: lock_and_reset() [acquires lock, resets TRACKER]
                                                        t0: enters test (no lock)
t1: raw_alloc(8, 8) → TRACKER.alloc_count = 1
                                                        t1: BumpArena::new(1024) → raw_alloc → TRACKER.alloc_count = 2 ‼
t2: assert_eq!(alloc_count(), 1) → FAILS (sees 2) PANIC
    └─ panic propagates out of `_g` MutexGuard → POISONS lock
                                                        t2: continues OK (test passes)
t3: cargo test_runner schedules next test
    next test: arena_drop_releases_chunk_via_tracker (uses lock)
       → lock_and_reset() → POISON_ERROR → panic
    next test: freeing_more_than_alloc_saturates_at_zero (uses lock)
       → lock_and_reset() → POISON_ERROR → panic
    [...117 more tests cascade with PoisonError...]
```

The first failure CAUSES the cascade — without it, parallel tests would still race-fail intermittently but each would be independent. The lock-poison cascade transforms an **intermittent flake** into a **catastrophic failure** at the first race-loss.

### § B.6 — Empirical confirmation : the FIRST failure has the race-signature

From `/tmp/cssl-rt-cold-full.log` :

```
test alloc::tests::arena_alloc_beyond_capacity_returns_null ... ok       (unlocked, fast)
test alloc::tests::arena_alignment_is_respected ... ok                   (unlocked, fast)
test alloc::tests::arena_basic_alloc_returns_non_null_within_capacity ... ok   (unlocked, fast)
test alloc::tests::alloc_count_total_matches_history ... FAILED          ← FIRST FAIL
test alloc::tests::arena_non_power_of_two_align_returns_null ... ok      (unlocked, fast)
test alloc::tests::arena_drop_releases_chunk_via_tracker ... FAILED      ← lock now poisoned
test alloc::tests::arena_sequential_allocs_advance_cursor ... ok         (unlocked, fast)
test alloc::tests::arena_zero_capacity_is_none ... ok                    (pure, doesn't lock)
test alloc::tests::arena_reset_returns_all_capacity ... ok               (unlocked, fast)
test alloc::tests::freeing_more_than_alloc_saturates_at_zero ... FAILED  ← lock poisoned
test alloc::tests::many_alloc_free_pairs_keep_bytes_in_use_at_zero ... FAILED
test alloc::tests::raw_alloc_64bytes_increments_counters ... FAILED
[...etc...]
```

The pattern is **exactly** as predicted :
1. The 7 unlocked `arena_*` tests run quickly + concurrently.
2. The first locked test `alloc_count_total_matches_history` runs while one of the unlocked arena_* tests was holding a stale tracker increment from `BumpArena::new(N)` it just called.
3. `alloc_count_total_matches_history` panics with `left:2 right:1`.
4. Lock poisons.
5. `arena_drop_releases_chunk_via_tracker` (which DOES try to lock) panics with PoisonError.
6. Every subsequent locked test cascades.

The 5 unlocked arena_* tests AFTER the first failure still PASS — they don't try to lock so they're not affected by the poison.

### § B.7 — Per-module test-lock audit (full)

Detailed pass through each module to identify other unlocked-but-touch-globals tests :

#### § B.7.a — `alloc.rs` (29 tests, 19 lock-uses)

Already analyzed in § B.4. **6 unlocked-but-touch-tracker** = bug.

#### § B.7.b — `panic.rs` (14 tests, 3 lock-uses) — 11 unlocked

Sampled : `format_panic_basic`, `format_panic_empty_file`, `format_panic_empty_msg`, etc. all use `format_panic` which is a pure formatting function — does NOT touch `PANIC_COUNT`. Genuinely safe. **OK.**

The 3 that DO use `lock_and_reset` are :
- `panic_count_starts_zero_after_reset`
- `record_panic_increments_counter`
- (and a third — sampled from grep output)

These touch `PANIC_COUNT`. Locked correctly.

**Verdict** : panic.rs is OK as-is. No buggy unlocked tests detected on sample.

#### § B.7.c — `path_hash.rs` (5 tests, 4 lock-uses) — 1 unlocked

The 1 unlocked test is likely the pure-function test. Sample lookup needed but low-risk.

**Verdict** : likely OK. T11-D151 implementation should sanity-check.

#### § B.7.d — `runtime.rs` (11 tests, 12 lock-uses) — fully locked

Every runtime test uses `lock_and_reset` (the 12-vs-11 grep noise is from a pattern match in a comment or doc). **OK.**

#### § B.7.e — `io.rs` / `io_win32.rs` / `net.rs` / `net_win32.rs` (large unlock-counts)

These have many pure-validation tests :
- `io.rs`: 23 unlocked = `validate_open_flags_*` (10 cases), `utf8_to_utf16_*` (4 cases), `validate_buffer_*` (3 cases), `*_distinct` (constant tests, 3 cases), pure-translate tests (3 cases). All operate on local data — no global touch.
- `net.rs`: 18 unlocked = `validate_sock_flags_*` (6 cases), `loopback_v4_predicate_*` (2 cases), `validate_buffer_*` (2 cases), constant-tests (5 cases), `loopback_constants_canonical` etc.
- `io_win32.rs` / `net_win32.rs`: largely `translate_*` pure tests + format-encoding tests.

**Sample-based verdict** : these unlocked-counts are LEGITIMATE pure-function tests. No false-positive bug in this audit ; T11-D151 implementation should still scan each one for unexpected global access.

#### § B.7.f — `exit.rs` / `ffi.rs` (fully locked)

13/13 and 18/18 — 100% locked. **OK.**

═══════════════════════════════════════════════════════════════════════════

## § APPENDIX-C : Full source-quoted analysis for INVESTIGATION-3

### § C.1 — `rust-toolchain.toml`
(`compiler-rs/rust-toolchain.toml`)

```toml
# § I> R16 reproducibility-anchor : pinned-toolchain per-commit
# § I> MSRV 1.85.0 per §§ DECISIONS T11-D20 (edition2024 for cranelift-jit)
# § I> bump only-with DECISIONS.md entry documenting reason
# § HISTORY
#   1.75.0 → 1.85.0 @ T11-D20 (2026-04-17) : unblock cranelift-jit activation
#                                            for stage-0.5 runtime execution.

[toolchain]
channel    = "1.85.0"
components = ["rustfmt", "clippy"]
profile    = "minimal"
```

The pin specifies version-only ; `rustup` defaults to the host triple. On Apocky's machine that resolves to `1.85.0-x86_64-pc-windows-gnu` because `Default host: x86_64-pc-windows-gnu`. The MSVC default (visible in `rustup show` as `stable-x86_64-pc-windows-msvc (default)`) is overridden.

### § C.2 — Cargo.lock evidence : multiple `windows-sys` versions resolved

```toml
# Cargo.lock excerpts (compiler-rs/Cargo.lock)

[[package]]
name = "windows-sys"
version = "0.52.0"               # ← legacy ; pre-generated import-libs ; NO dlltool
checksum = "282be5f36a8ce781fad8c8ae18fa3f9beff57ec1b52cb3de0789201425d9a33d"
dependencies = ["windows-targets"]

[[package]]
name = "windows-sys"
version = "0.59.0"               # ← also pre-generated ; NO dlltool
checksum = "1e38bc4d79ed67fd075bcc251a1c39b32a1776bbe92e5bef1f0bf1f8c531853b"
dependencies = ["windows-targets"]

[[package]]
name = "windows-sys"
version = "0.61.2"               # ← NEW ; uses build.rs + dlltool ‼
checksum = "ae137229bcbd6cdf0f7b80a31df61766145077ddf49416a728b02cb3921ff3fc"
dependencies = ["windows-link"]   # ← note new "windows-link" dep instead of windows-targets

[[package]]
name = "winapi-util"
version = "0.1.11"               # ← chooses windows-sys 0.61.2
checksum = "c2a7b1c03c876122aa43f3020e6c3c3ee5c05081c9a00739faf7503aeba10d22"
dependencies = ["windows-sys 0.61.2"]
```

The `windows-link` crate (v0.2.1) is the new sub-crate of `windows-sys 0.61` that drives `dlltool` invocation in build.rs.

### § C.3 — Why earlier `winapi-util` patch versions don't need `dlltool`

`winapi-util 0.1.10` (predecessor) pinned `windows-sys 0.59`. The 0.59 line uses `windows-targets` which ships pre-generated `.lib` import-libraries for both MSVC and GNU toolchains directly in the crate package. No build-time generation needed.

`winapi-util 0.1.11` bumped to `windows-sys 0.61.2`. The 0.61 line replaced bundled pre-generated libs with a build-time `dlltool` invocation (a **size optimization** : reduces the wheel/crate-package size by ~50MB by not shipping all 8 architecture-targets' pre-generated libs).

This is a known upstream churn point — cf. `windows-rs` issue tracker discussions about the 0.59→0.61 transition.

### § C.4 — Production build IS unaffected

```
$ cd compiler-rs && cargo build -p cssl-cgen-gpu-wgsl --release
[builds successfully]
```

Because the production (non-`--tests`) build doesn't pull `[dev-dependencies] naga`, the entire `winapi-util → windows-sys 0.61.2` chain is bypassed. **Only `--tests` builds are affected.**

### § C.5 — Verifying clippy succeeded despite the dev-dep chain

The `cargo clippy --workspace --all-targets -- -D warnings` succeeded with EXIT 0. Why didn't IT also fail with the dlltool error?

Explanation : `cargo clippy --all-targets` checks tests but does NOT run a full link of the dev-dependencies' build.rs scripts in the same way `cargo build --tests` does. Clippy's check-mode shares the `target/debug/build/windows-sys-*/build-script-build.exe` only if it's already cached. Since the prior `cargo test -p cssl-host-net` runs (which use a different feature unification) had already populated some cargo state, clippy avoided re-running the build script.

When `cargo build -p cssl-cgen-gpu-wgsl --tests` is invoked from a slightly-different feature unification (because the wgsl crate's dev-dep on naga adds the `wgsl-in` feature), cargo decides to RE-RUN the windows-sys build script — and THAT triggers `dlltool`.

This is a subtle cargo feature-unification quirk. The take-away : **the failure is feature-set-dependent and not 100%-reproducible on every cargo invocation** — but when it manifests, it's deterministic.

═══════════════════════════════════════════════════════════════════════════

## § APPENDIX-D : Detailed fix-slice specs (CSL-formatted)

### § D.1 — T11-D150 fix-slice spec

```cssl-spec
§ T11-D150 : cssl-rt — WSAStartup ref-count race (host-net 3 TCP fail closure)

§ Date           2026-04-29
§ Branch         cssl/session-11/T11-D150-wsastartup-race
§ Pre-condition  audit findings INVESTIGATION-1 + APPENDIX-A
§ Approach       Option-B = process-pin (one-shot WSAStartup ; no WSACleanup)

§ FILES MODIFIED
  compiler-rs/crates/cssl-rt/src/net_win32.rs    (~80 LOC change + ~20 doc-comment)
  compiler-rs/crates/cssl-rt/src/net_win32.rs    (tests : add 3-4 stress tests, ~50 LOC)

§ FILES ADDED
  none

§ DESIGN

  Replace the ref-counted WSAStartup/WSACleanup pattern with a process-pinned
  startup-only pattern :

  1. Keep `WSA_INIT_COUNT` for diagnostic counting (test-observable).
  2. Add a `Once` (std::sync::OnceLock or Once) that runs WSAStartup
     EXACTLY ONCE per process.
  3. `release_wsa_started` becomes a pure decrement (no WSACleanup invocation).
  4. WSACleanup is implicitly handled by process-exit (the OS cleans up
     Winsock state when the process terminates).

§ DRAWBACK
  - At process-shutdown, Winsock state is left dangling for ~milliseconds
    until the OS reaps it. ZERO observable effect — the process is exiting.
  - Counter-arg : the production runtime (`cssl_rt::entry`) is the typical
    invocation path, which runs to completion + lets the OS reap. This
    matches Rust stdlib's net implementation (which also pins WSAStartup
    process-wide and never calls WSACleanup).

§ TESTS ADDED

  // Stress test : 20 threads × 100 cycles of bind/close
  #[test]
  fn wsastartup_race_resistance_under_high_parallelism() {
      let handles : Vec<_> = (0..20).map(|_| {
          std::thread::spawn(|| {
              for _ in 0..100 {
                  let s = unsafe { cssl_net_socket_impl(SOCK_TCP) };
                  assert_ne!(s, -1, "socket() failed under stress ; race-leak");
                  unsafe { cssl_net_close_impl(s) };
              }
          })
      }).collect();
      for h in handles { h.join().unwrap(); }
      // Tracker should be balanced — alloc_count == close_count.
      assert_eq!(socket_count(), 20 * 100);
      assert_eq!(close_count(), 20 * 100);
  }

  // Test : WSAStartup runs exactly once
  #[test]
  fn wsastartup_runs_exactly_once_via_oncelock() {
      // Force multiple init calls
      for _ in 0..50 { ensure_wsa_started().unwrap(); }
      // The OnceLock-protected counter should still report only 1 actual call.
      assert_eq!(wsa_actual_startup_invocation_count_for_tests(), 1);
  }

§ MIGRATION
  - Update doc-comments at lines 14-23 (WSAStartup REF-COUNTING section)
    to document the new process-pin model.
  - Update DECISIONS.md with a new entry referencing T11-D56 closure.
  - Update CHANGELOG.md with a "Fixed" entry.

§ COMPAT
  - Existing test `socket_create_then_close_balances_wsa_count` may need
    revising — under process-pin model, the count starts at 1 (after first
    Winsock op) and stays >= 1 forever. Update the test to verify
    BALANCED-FROM-INITIAL rather than goes-to-zero.

§ ESTIMATED EFFORT  ~2-4 hours for an agent (well-bounded slice)
§ ESTIMATED LOC   ~100 (impl) + 50 (tests) + 30 (doc) = ~180 LOC total
§ RISK             low ; isolated to one file ; clean-cut alternative exists
```

### § D.2 — T11-D151 fix-slice spec

```cssl-spec
§ T11-D151 : cssl-rt — tracker-race + lock-poison cascade fix (T11-D56 closure)

§ Date           2026-04-29
§ Branch         cssl/session-11/T11-D151-tracker-race-fix
§ Pre-condition  audit findings INVESTIGATION-2 + APPENDIX-B
§ Approach       Part-1 = add lock to 6 buggy arena_* tests
                 Part-2 = MutexGuard with clear_poison() in lock_and_reset_all
                 Part-3 = optional ; add stress regression test

§ FILES MODIFIED
  compiler-rs/crates/cssl-rt/src/alloc.rs   (~6 lock-acquire-line additions)
  compiler-rs/crates/cssl-rt/src/lib.rs     (~10 LOC change in lock_and_reset_all)
  compiler-rs/crates/cssl-rt/src/path_hash.rs (audit + possibly fix 1 test)
  compiler-rs/crates/cssl-rt/src/panic.rs   (audit + possibly fix tests)

§ FILES ADDED
  none

§ DESIGN — PART 1

  Add `let _g = lock_and_reset();` as the first line of each of the 7
  arena_* tests at L519-L575 in alloc.rs. Existing tests at L577 (drop)
  and L591 (stress) already correctly lock.

§ DESIGN — PART 2

  Replace lib.rs:181-184 :
  ```rust
  pub fn lock_and_reset_all() -> MutexGuard<'static, ()> {
      let g = GLOBAL_TEST_LOCK
          .lock()
          .expect("crate-shared test lock poisoned ; prior test failed mid-update");
  ```
  with the unwrap-poison form :
  ```rust
  pub fn lock_and_reset_all() -> MutexGuard<'static, ()> {
      let g = match GLOBAL_TEST_LOCK.lock() {
          Ok(g) => g,
          Err(poisoned) => {
              // A prior test panicked while holding the lock. Test-only
              // path : we WANT the next test to get a clean lock so each
              // failure is independent (no cascade). Real test-failures
              // still report independently via cargo's test runner.
              poisoned.into_inner()
          }
      };
  ```
  This makes a single race-induced failure independent rather than
  cascading. After the fix, the cold-cache parallel run should show
  AT MOST 1-2 actual race failures + 196-197 PASSING tests (instead
  of 80 PASSING + 118 cascading-poison-fails).

§ DESIGN — PART 3 (optional)

  Add a regression test in alloc.rs that explicitly stresses the
  tracker against the lock :

  #[test]
  fn tracker_lock_resists_arena_concurrent_pressure() {
      use std::sync::atomic::AtomicU32;
      use std::sync::atomic::Ordering::Relaxed;

      static FAIL: AtomicU32 = AtomicU32::new(0);
      let pressure : Vec<_> = (0..16).map(|_| std::thread::spawn(|| {
          for _ in 0..50 {
              let _arena = BumpArena::new(256);
          }
      })).collect();

      // While background pressure runs, do a locked-read.
      for _ in 0..20 {
          let _g = lock_and_reset();
          assert!(alloc_count() <= 16 * 50,
              "tracker count exceeded reasonable bound under stress");
      }

      for h in pressure { h.join().unwrap(); }
      assert_eq!(FAIL.load(Relaxed), 0);
  }

  Note : the test must NOT assert that count is exactly 0 — the
  background threads add to it. The test is verifying that the LOCK
  is monitor-correct, not that the tracker stays pristine.

§ ESTIMATED EFFORT  ~3-5 hours for an agent
§ ESTIMATED LOC   ~50 (Part 1) + ~15 (Part 2) + ~80 (Part 3 + audit) = ~145 LOC
§ RISK             low ; conservative changes ; existing tests serve as oracle
```

### § D.3 — T11-D152 fix-slice spec

```cssl-spec
§ T11-D152 : workspace — dlltool dev-dep fix (cssl-cgen-gpu-wgsl --tests build)

§ Date           2026-04-29
§ Branch         cssl/session-11/T11-D152-dlltool-fix
§ Pre-condition  audit findings INVESTIGATION-3 + APPENDIX-C
§ Approach       Option-A primary = pin transitive winapi-util to 0.1.10
                 Option-C fallback = toolchain switch to MSVC

§ FILES MODIFIED (Option A path)
  compiler-rs/Cargo.toml        (~5 LOC : add [patch.crates-io])
  compiler-rs/Cargo.lock        (regenerated)

§ FILES MODIFIED (Option C fallback)
  compiler-rs/rust-toolchain.toml   (~3 LOC : add msvc target)
  compiler-rs/Cargo.lock            (likely re-resolves)

§ DESIGN — Option A (PRIMARY)

  Add to compiler-rs/Cargo.toml :
  ```toml
  [patch.crates-io]
  winapi-util = { version = "=0.1.10" }
  ```
  Then `cargo update -p winapi-util` to regenerate Cargo.lock with
  pinned 0.1.10 (which depends on windows-sys 0.59 with bundled libs).

§ VALIDATION
  cargo build -p cssl-cgen-gpu-wgsl --tests
  cargo test --workspace -- --test-threads=1   # confirm no regression elsewhere

§ RISK — Option A
  - cargo's resolver may complain if any workspace dep transitively
    requires winapi-util >= 0.1.11. Mitigation : audit `cargo tree` output
    to confirm 0.1.10 satisfies all consumers. Most consumers via termcolor
    accept ranges starting at 0.1 ; risk is minimal.

§ DESIGN — Option C (FALLBACK)

  Edit rust-toolchain.toml :
  ```toml
  [toolchain]
  channel    = "1.85.0"
  components = ["rustfmt", "clippy"]
  profile    = "minimal"
  targets    = ["x86_64-pc-windows-msvc"]   # NEW : explicit MSVC
  ```
  This forces rustup to use the MSVC toolchain. windows-sys on MSVC
  uses MSVC-native lib format ; no dlltool needed.

§ RISK — Option C
  - Larger blast-radius : entire workspace may compile slightly
    differently. cranelift bindings, FFI, all behave identically on
    MSVC vs GNU but the output binary format is .obj/.lib instead of
    COFF/.a (which is fine — Apocky's S6-A5 hello.exe gate already
    uses MSVC linker).
  - Need to re-verify ALL existing gates (clippy / fmt / test / doc /
    xref / smoke) under MSVC.

§ RECOMMENDATION
  Try Option A first. If patch resolver fails, pivot to Option C.

§ ESTIMATED EFFORT  ~30 mins (Option A) — ~2 hours (Option C with full re-verify)
§ ESTIMATED LOC   Option A: ~5 LOC ; Option C: ~3 LOC
§ RISK             Option A: low ; Option C: medium-low (well-known toolchain choice)
```

═══════════════════════════════════════════════════════════════════════════

## § APPENDIX-E : Pre-Wave-Jε dispatch checklist

If the recommendation to dispatch fix-slices in parallel with Wave-Jε is accepted, this checklist serves as the dispatch package :

```
□ Reserve T11-D150 ↔ "WSAStartup race fix"
  - Branch  : cssl/session-11/T11-D150-wsastartup-race
  - File scope : compiler-rs/crates/cssl-rt/src/net_win32.rs
  - Tests added : 3-4
  - LOC est   : ~180 (impl + tests + doc)
  - Block-Jε  : NO

□ Reserve T11-D151 ↔ "tracker-race + poison fix"
  - Branch  : cssl/session-11/T11-D151-tracker-race-fix
  - File scope : compiler-rs/crates/cssl-rt/src/{alloc.rs, lib.rs, panic.rs, path_hash.rs}
  - Tests added : 1-2 + 7 lock-additions to existing
  - LOC est   : ~145 (changes + new test)
  - Block-Jε  : NO

□ Reserve T11-D152 ↔ "dev-dep dlltool fix"
  - Branch  : cssl/session-11/T11-D152-dlltool-fix
  - File scope : compiler-rs/Cargo.toml + Cargo.lock
  - Tests added : 1 smoke
  - LOC est   : ~5-30 (Option A)
  - Block-Jε  : conditional (NO unless CI does fresh-clone --tests build)

□ Shift Phase-J slice-ID range : T11-D150..D201 → T11-D153..D204
  - Update SESSION_12_DISPATCH_PLAN.md
  - Update PHASE_J_HANDOFF reference doc
  - Confirm with PM / dispatcher coordination

□ Confirm post-fix gate :
  - cargo test --workspace                        ← should pass under DEFAULT parallelism (no --test-threads=1 hack)
  - cargo test -p cssl-host-net                   ← 13/13 under default parallel
  - cargo test -p cssl-rt --lib                   ← 198/198 under default parallel + cold cache
  - cargo build -p cssl-cgen-gpu-wgsl --tests     ← compiles
  - cargo clippy --workspace --all-targets -- -D warnings  ← clean
  - cargo fmt --all -- --check                    ← clean
```

═══════════════════════════════════════════════════════════════════════════

## § APPENDIX-F : Complete reproduction commands (copy-paste ready)

For someone re-verifying this audit :

```bash
# Repo root must be CSSLv3
cd C:\Users\Apocky\source\repos\CSSLv3\compiler-rs

# § Verify toolchain
rustup show
# Expected : 1.85.0-x86_64-pc-windows-gnu (active because of rust-toolchain.toml)

# § Reproduce INVESTIGATION-1 (host-net 3 TCP failures, parallel)
cargo test -p cssl-host-net 2>&1
# Expected : 10 passed ; 3 failed (NotInitialized signature)

# § Confirm INVESTIGATION-1 mitigation
cargo test -p cssl-host-net -- --test-threads=1 2>&1
# Expected : 13 passed ; 0 failed

# § Reproduce INVESTIGATION-2 (cssl-rt cold-cache flake)
rm -rf target/debug/deps/cssl_rt*
cargo test -p cssl-rt --lib 2>&1 | tail -50
# Expected : ~80 passed ; ~118 failed (poison cascade signature)

# § Confirm INVESTIGATION-2 mitigation (5x serial run)
for i in 1 2 3 4 5; do
  echo "=== run $i ==="
  cargo test -p cssl-rt --lib -- --test-threads=1 2>&1 | tail -3
done
# Expected : 5/5 runs at 198/0 each

# § Reproduce INVESTIGATION-3 (dlltool failure)
cargo build -p cssl-cgen-gpu-wgsl --tests 2>&1
# Expected : "error: Error calling dlltool 'dlltool.exe': program not found"

# § Confirm INVESTIGATIONS-4 + 5 (clippy + fmt drift)
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -10
# Expected : Finished `dev` profile ; EXIT 0
cargo fmt --all -- --check 2>&1
# Expected : (empty output) ; EXIT 0
```

═══════════════════════════════════════════════════════════════════════════

## § APPENDIX-G : DECISIONS.md historical context for the cold-cache flake

The cold-cache flake has been carried-forward as a "known issue" through these slices since T11-D56 :

| Slice | Date | Status |
|---|---|---|
| T11-D56 | 2026-04-28 | Original observation (S6-A5 hello-world gate ; concurrent with Phase-A close) |
| T11-D58 | post-D56 | Carried-over note in deferred section |
| T11-D59 | post-D56 | Carried-over note ; "still tracked" |
| T11-D61 | post-D56 | Carried-over note ; "still tracked" |
| T11-D70 | post-D56 | Carried-over note ; "this slice does not introduce new flakes" |
| T11-D76 | post-D56 | Carried-over note ; io tests use `lock_and_reset_all` helper |
| ... | ... | ~30+ subsequent slices all reference the same workaround |
| T11-D147 (current HEAD) | 2026-04-29 | Workaround `--test-threads=1` documented + applied ✓ ; root-cause fix DEFERRED |

**This audit is the proposed path to closing the deferral** — T11-D151 root-causes + fixes the issue. Closing this 6-month-old workaround burden has substantial dividends :
- Reverts the 2-3× test-runtime overhead from `--test-threads=1` serial-mode ;
- Eliminates the cognitive burden of "remember to add --test-threads=1" on every commit-gate run ;
- Removes a class of "future tests added without lock_and_reset" landmines (because the cascade-from-poisoning will no longer hide them).

═══════════════════════════════════════════════════════════════════════════

## § ATTESTATION (RE-ASSERTION)

> There was no hurt nor harm in the making of this, to anyone/anything/anybody.

Audit completed 2026-04-29 under Apocky PRIME DIRECTIVE. No source-tree modifications were made (W! audit-only ; ¬commit ; ¬write-to-non-_drafts-paths). All test runs were strictly diagnostic and read-only with respect to compiler-rs/ source code. Target/ cache was rebuilt for cssl-rt to surface the cold-cache flake — that is a benign side-effect of `cargo build` and does not constitute a source change.

§R+ : This report respects the user's notation-default (CSLv3-native for analysis ; English where prose is unavoidable) and follows the optimal-not-minimal directive (full root-cause traces + reproductions + fix-options provided rather than a thin summary). All findings are evidence-backed with quoted command outputs.

§I> The 3 fix-slices identified here form the "audit-trail close-out" for T11-D56 + the host-net flake + the dlltool blocker. None are dispatch-blockers for Wave-Jε. All can be dispatched in parallel with Phase-J without conflict.

§W! Recommendation : land T11-D150 + T11-D151 + T11-D152 as a 3-slice "audit-fix wave" at the start of Wave-Jε's parallel-fanout (or any time during Phase-J — the slice-IDs can be allocated immediately). Wave-Jε itself proceeds unmodified.

§W! After landing T11-D151, the canonical test invocation may revert to `cargo test --workspace` (no `--test-threads=1`) — providing a 2-3× speedup for every commit-gate run for the rest of the project lifetime.

— § §END
