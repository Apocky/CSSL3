# Apocrypha contributor node candidate

This package is a local-only candidate runtime. It has no network transport,
filesystem traversal, shell, process spawning, git, chat, vault, or MCP
integration. It starts paused, requires `--opt-in`, accepts one controller-
signed deterministic `vector_dot` lease, and emits one node-signed result.

The candidate is intentionally not a public executable release. A controller
transport, publisher signature, platform installers, authenticated contribution
oracle, and rollback proof are still missing. Do not run an unsigned package
against production.

## Build and local test

From `cssl-edge/`:

```text
npm --prefix contributor-node run build
npm --prefix contributor-node test
```

The resulting `dist/` directory is reproducible from the pinned source and
compiler version in the parent lockfile. It must be rebuilt, scanned, signed,
and independently smoke-tested before any release pointer is created.

## Candidate evidence bundle

`npm run release:candidate` reuses `pack:candidate`, then emits a deterministic
ZIP plus SPDX SBOM, in-toto/SLSA-shaped provenance, and a fail-closed promotion
manifest under `candidate-dist/`. Set `SOURCE_DATE_EPOCH` and `SOURCE_COMMIT`
when producing a reproducible evidence bundle. The ZIP uses fixed timestamps,
stable path order, and stored entries; it is not a signed public download.

Detached signatures are verified against the exact ZIP bytes only:

```text
node scripts/verify-detached-signature.mjs --artifact FILE.zip --signature FILE.sig --public-key publisher.pub --expected-sha256 HEX
```

The verifier has no signing, private-key, network, or release-promotion path.
The generated promotion manifest remains `NOT_DEPLOYABLE` until every native,
transport, signing, sandbox, scan, rollback, and platform gate is independently
closed with evidence.

## Local protocol

The process reads one JSON lease from stdin and writes one JSON result to
stdout. It requires an Ed25519 controller public key in
`APOCRYPHA_CONTROLLER_PUBLIC_KEY_PEM`, and only accepts `--opt-in` for work.
No private key is read from an environment variable; the node signing key is
generated in memory and never leaves the process.
