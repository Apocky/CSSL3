# Security Policy

We take security in the Sigil (CSSLv3) compiler and surrounding open-source
crates seriously. Thank you for helping keep the project safe.

## Reporting a vulnerability

**Please do not open a public GitHub issue for security reports.**

Email: **apocky13@gmail.com**

Subject line prefix: `[CSSL3 SECURITY]`

Please include:

- A clear description of the issue and its potential impact
- Steps to reproduce (proof-of-concept code, sample input, environment)
- Affected version, commit hash, or release tag if known
- Whether the issue is already public or coordinated elsewhere
- How you would like to be credited (or whether you prefer to remain anonymous)

If you would like to encrypt your report, request a PGP key in your initial
email and we will reply with a current public key.

## Response timeline

- **Acknowledgement:** within **5 business days** of your initial email.
- **Triage and severity assessment:** within **10 business days**.
- **Fix and disclosure:** typically **30–90 days** depending on severity,
  complexity, and whether a coordinated release is needed. We will keep you
  updated as we work through the fix and will notify you before public
  disclosure.

We follow a coordinated-disclosure model: we ask reporters to give us a
reasonable window to ship a fix before publishing details. If you have a
hard timeline (e.g., upcoming conference talk), please tell us up front and
we will do our best to align.

## Scope

**In scope (this repository):**

- The `compiler-rs/` Cargo workspace and all open-source crates
- The `csslc` compiler binary and runtime library
- Build and packaging scripts in this repository
- Specifications and documentation, when they describe a security-relevant
  invariant the compiler is expected to enforce

**Out of scope (this repository):**

- Proprietary engine binaries, trained model weights, or private-tier
  services distributed outside this repository — those have their own
  reporting channels described in their own documentation.
- Vulnerabilities in upstream Rust crates we depend on. Please report those
  to the upstream maintainers directly (we are happy to help coordinate if
  the fix needs to flow through this repository).
- Issues that require physical access to the user's machine or that depend
  on the user explicitly running known-malicious code.

## Safe-harbor

We will not pursue legal action against researchers acting in good faith
under this policy: testing only their own systems or systems they have
authorization to test, avoiding privacy violations and service disruption,
and giving us a reasonable window before public disclosure.

## Recognition

Once a fix is released, we are happy to credit reporters in the release
notes (or omit credit entirely if you prefer). Coordinated CVE issuance is
available for qualifying issues.

## Contact

apocky13@gmail.com
