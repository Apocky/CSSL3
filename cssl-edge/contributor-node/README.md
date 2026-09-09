# Apocrypha contributor node

This package is an opt-in, resource-capped contributor runtime. It has no filesystem traversal, shell, process spawning, git, chat, vault, or MCP integration. It starts paused, requires `--opt-in`, and can either accept one controller-signed deterministic `vector_dot` lease on stdin or use `--network` to enroll, poll a node-signed lease, execute it, and submit a node-signed result to the Apocrypha transport.

The downloadable artifact remains gated by the website release manifest until
the platform signer, install smoke, malware scan, and rollback evidence are
published. Do not run an unsigned package against production.

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
For live mode, the package supplies the pinned controller public key and
production key id; run `node dist/cli.js --network --opt-in`. Set
`APOCRYPHA_CONTROLLER_PUBLIC_KEY_PEM` and `APOCRYPHA_CONTROLLER_KEY_ID` only
when independently verifying a different controller.
The node identity is generated once under the platform state directory with
0600 permissions and is never sent to the server; only its public key and
signed envelopes leave the device. `--loop` is an additional explicit opt-in.

The current production public key is published at
`https://www.apocky.com/releases/apocrypha-node/controller-public-key.pem`.
Verify its SHA-256 SPKI fingerprint before configuring the node:
`1f6a4507814f922a572c2c61faaed287bdf1ec4c15ba99c907e830c0f0c5eb17`.

The Windows portable bundle includes `node.exe`, the compiled worker, this
key, and two explicit launchers. Run `run-once-apocrypha-node.cmd` for one
bounded contribution or `start-apocrypha-node.cmd` to opt into the one-second
poll loop. No service, auto-start, elevation, or background process is
installed; close the console or press Ctrl+C to stop.
