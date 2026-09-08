import type { GetStaticProps, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import {
  CONTRIBUTOR_NODE_MANIFEST,
  parseContributorNodeManifest,
  type ContributorNodeManifest,
} from '@/lib/apocrypha/contributor-node';

interface Props {
  readonly manifest: ContributorNodeManifest;
}

function formatState(value: string): string {
  return value.split('_').join(' ');
}

export function canOfferContributorArtifact(
  manifest: ContributorNodeManifest,
  platform: ContributorNodeManifest['platforms'][number],
): boolean {
  return (manifest.release_state === 'PARTIAL' || manifest.release_state === 'READY')
    && manifest.release_gate === 'OPEN'
    && manifest.contract.production_eligible
    && manifest.contract.status === 'native_release'
    && manifest.contract.transport === 'signed_native_transport'
    && platform.state === 'READY'
    && platform.artifact !== null
    && manifest.artifact_policy.detached_signature === 'required'
    && manifest.artifact_policy.pinned_signer === 'required'
    && manifest.artifact_policy.unsigned_candidate_download === 'blocked';
}

const ApocryphaContributorNodeDownload: NextPage<Props> = ({ manifest }) => {
  const desktop = manifest.platforms.filter((platform) => platform.category === 'desktop');
  const mobile = manifest.platforms.filter((platform) => platform.category === 'mobile');
  const publicRelease = manifest.release_state === 'PARTIAL' || manifest.release_state === 'READY';
  const gateRows = [
    ['SHA-256 digest', manifest.artifact_policy.sha256],
    ['Detached signature', manifest.artifact_policy.detached_signature],
    ['Pinned release signer', manifest.artifact_policy.pinned_signer],
    ['Reproducible build', manifest.artifact_policy.reproducible_build],
    ['Malware scan', manifest.artifact_policy.malware_scan],
    ['Install smoke', manifest.artifact_policy.install_smoke],
    ['Rollback proof', manifest.artifact_policy.rollback],
    ['Unsigned candidate download', manifest.artifact_policy.unsigned_candidate_download],
  ] as const;
  return (
    <>
      <Head>
        <title>Apocrypha contributor node · downloads · Apocky</title>
        <meta
          name="description"
          content="Public release status for the opt-in, resource-capped Apocrypha Mycelial Contributor Node."
        />
        <meta property="og:title" content="Apocrypha contributor node" />
        <meta
          property="og:description"
          content="A future sovereign, opt-in contributor node for public Apocrypha workloads."
        />
        <link rel="canonical" href="https://www.apocky.com/download/apocrypha-node" />
        <link rel="alternate" type="application/json" href="/api/apocrypha/contributor/manifest" />
      </Head>
      <main className="contributor-page" id="main-content">
        <div className="contributor-wrap">
          <nav className="return-nav" aria-label="Download navigation">
            <Link href="/">← Home</Link>
            <Link href="/apocrypha">Open Apocrypha →</Link>
          </nav>

          <header className="hero">
            <p className="eyebrow">Apocrypha · Mycelium</p>
            <div className="title-row">
              <div>
                <h1>Contribute compute, on your terms.</h1>
                <p className="lead">One release surface for the future desktop and mobile contributor node.</p>
              </div>
              <span className="state-badge" role="status" aria-live="polite" data-release-state={manifest.release_state}>
                {formatState(manifest.release_state)}
              </span>
            </div>
            <div className="notice" role="note">
              <strong>{publicRelease ? 'Verified packages are listed below.' : 'Downloads are not enabled yet.'}</strong>
              <p>
                {publicRelease
                  ? 'Only platform entries with a verified artifact link can be installed. This page never starts a worker or grants processing authority.'
                  : 'The current Apocrypha node is a local candidate contract, not a live worker. No executable, listener, background job, or processing-power contribution starts from this page.'}
              </p>
            </div>
          </header>

          <section className="release-card" aria-labelledby="release-status-heading">
            <div className="section-heading">
              <div>
                <p className="eyebrow">Release gate</p>
                <h2 id="release-status-heading">{manifest.version}</h2>
              </div>
              <span className="closed" data-release-gate={manifest.release_gate}>{manifest.release_gate}</span>
            </div>
            <p className="copy">{manifest.summary}</p>
            <p className="copy small">
              A package appears only after every required gate below is independently verified. A listed
              requirement is a release condition, not proof that the current candidate has passed it.
            </p>
            <a
              className="manifest-link"
              href="/api/apocrypha/contributor/manifest"
              rel="alternate"
              type="application/json"
            >
              View machine-readable manifest →
            </a>
          </section>

          <section aria-labelledby="platforms-heading">
            <div className="section-heading compact">
              <div>
                <p className="eyebrow">Platform matrix</p>
                <h2 id="platforms-heading">Desktop + mobile</h2>
              </div>
            </div>
            <div className="platform-grid">
              {[...desktop, ...mobile].map((platform) => (
                <article className="platform-card" key={platform.target}>
                  <div className="platform-topline">
                    <span className="platform-kind">{platform.category}</span>
                    <span className="platform-state" data-platform-state={platform.state}>{formatState(platform.state)}</span>
                  </div>
                  <h3>{platform.label}</h3>
                  <p>{platform.availability_note}</p>
                  {canOfferContributorArtifact(manifest, platform) && platform.artifact ? (
                    <a className="download-button" href={platform.artifact.href} download>
                      Download verified package <span aria-hidden="true">↓</span>
                    </a>
                  ) : (
                    <span className="download-disabled" aria-disabled="true">
                      No verified package available
                    </span>
                  )}
                  <p className="platform-scope">
                    {platform.category === 'mobile'
                      ? 'Foreground opt-in only · charging required · metered network blocked by default.'
                      : 'Per-user opt-in · starts paused · no elevation or automatic start.'}
                  </p>
                </article>
              ))}
            </div>
          </section>

          <section className="verification-card" aria-labelledby="verification-heading">
            <div className="section-heading compact">
              <div>
                <p className="eyebrow">Candidate gates</p>
                <h2 id="verification-heading">What must pass before download</h2>
              </div>
            </div>
            <div className="gate-grid">
              {gateRows.map(([label, state]) => (
                <div className="gate-row" key={label}>
                  <span>{label}</span>
                  <code data-gate-state={state}>{formatState(state)}</code>
                </div>
              ))}
            </div>
            <p className="copy small">
              The current manifest is {formatState(manifest.release_state)} / {manifest.release_gate}.
              The unsigned-candidate rule is {formatState(manifest.artifact_policy.unsigned_candidate_download)};
              no candidate package is linked while the gate is closed.
            </p>
            <details className="verify-help">
              <summary>How to verify a future package</summary>
              <ol>
                <li>Open the machine-readable manifest and confirm the platform state is <code>READY</code> and the release gate is <code>OPEN</code>.</li>
                <li>Compare the downloaded file with the manifest’s full <code>sha256</code> value: <code>Get-FileHash .\PACKAGE.zip -Algorithm SHA256</code>.</li>
                <li>Verify the detached signature with the release verifier and pinned public key named by <code>signing_key_id</code>. If either is missing, do not install.</li>
                <li>Keep the local node paused until you have reviewed the package’s scope, resource caps, privacy boundary, and pause/revoke/uninstall controls.</li>
              </ol>
            </details>
          </section>

          <section className="details-grid" aria-label="Contribution safeguards">
            <article className="detail-card">
              <p className="eyebrow">When released</p>
              <h2>Safe-by-default contribution</h2>
              <ol>
                <li>Install a signed package and verify its SHA-256 plus detached signature.</li>
                <li>Choose explicit opt-in, resource caps, schedule, and network policy.</li>
                <li>Run only approved public-capsule workloads; the node starts paused.</li>
                <li>Pause immediately, revoke and sever, or uninstall while retaining state.</li>
              </ol>
            </article>
            <article className="detail-card">
              <p className="eyebrow">Data boundary</p>
              <h2>What never leaves your device</h2>
              <ul>
                <li>Raw conversation, raw memory, and Obsidian Vault payloads.</li>
                <li>Credentials, private paths, screenshots, and local logs.</li>
                <li>Anything not explicitly distilled into a public capsule.</li>
                <li>Telemetry by default; any future diagnostics require separate consent.</li>
              </ul>
            </article>
          </section>

          <section className="policy-card" aria-labelledby="policy-heading">
            <p className="eyebrow">Proposed default caps</p>
            <h2 id="policy-heading">Your machine remains yours.</h2>
            <dl className="policy-list">
              <div><dt>CPU</dt><dd>≤ {manifest.resource_policy.cpu_percent_max}%</dd></div>
              <div><dt>Memory</dt><dd>≤ {manifest.resource_policy.memory_mb_max} MB</dd></div>
              <div><dt>Disk</dt><dd>≤ {manifest.resource_policy.disk_mb_max} MB</dd></div>
              <div><dt>Network</dt><dd>≤ {manifest.resource_policy.egress_mbps_max} Mbps · metered blocked</dd></div>
            </dl>
            <p className="copy small">
              These are contract defaults, not a claim that a worker is currently running. The future runtime
              must enforce them and expose a local receipt before release promotion.
            </p>
          </section>

          <footer className="footer-links">
            <Link href="/docs/mycelium">Read the Mycelium contract</Link>
            <Link href="/download/apocrypha">Apocrypha app downloads</Link>
            <Link href="/legal/privacy">Privacy</Link>
          </footer>
        </div>
      </main>
      <style jsx>{`
        .contributor-page { min-height: 100vh; background: #0c1020; color: #f2eee5; padding: 24px 20px 54px; }
        .contributor-wrap { max-width: 1080px; margin: 0 auto; }
        .return-nav { display: flex; justify-content: space-between; gap: 16px; flex-wrap: wrap; margin-bottom: 46px; }
        .return-nav :global(a), .footer-links :global(a), .manifest-link { color: #91d5dc; text-underline-offset: 4px; min-height: 44px; display: inline-flex; align-items: center; }
        .hero { max-width: 820px; margin-bottom: 24px; }
        .eyebrow { margin: 0 0 8px; color: #c6b5f0; font: 600 12px/1.4 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; letter-spacing: .12em; text-transform: uppercase; }
        .title-row, .section-heading { display: flex; align-items: start; justify-content: space-between; gap: 20px; }
        h1 { max-width: 17ch; margin: 0; font-size: clamp(2.3rem, 6vw, 4.6rem); line-height: 1.03; letter-spacing: -.055em; }
        h2 { margin: 0; font-size: clamp(1.35rem, 3vw, 2rem); line-height: 1.15; letter-spacing: -.03em; }
        h3 { margin: 12px 0 8px; font-size: 1.35rem; }
        .lead { color: #bac3d2; font-size: 1.08rem; line-height: 1.65; margin: 18px 0 0; }
        .state-badge, .closed, .platform-state, .platform-kind { border: 1px solid #586378; border-radius: 999px; color: #f6d487; font: 600 11px/1.2 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; letter-spacing: .08em; padding: 8px 11px; white-space: nowrap; }
        .closed { color: #f6d487; }
        .notice, .release-card, .platform-card, .detail-card, .policy-card { background: #161c2a; border: 1px solid #364052; border-radius: 16px; }
        .notice { max-width: 760px; margin-top: 30px; padding: 16px 18px; border-left: 3px solid #f6d487; color: #bac3d2; line-height: 1.6; }
        .notice strong { color: #f6d487; }
        .notice p { margin: 6px 0 0; }
        .release-card { margin: 26px 0 44px; padding: 24px; }
        .copy { max-width: 70ch; color: #bac3d2; line-height: 1.65; margin: 14px 0 0; }
         .copy.small { font-size: .9rem; color: #8f9bad; }
         .manifest-link { margin-top: 16px; }
         .compact { margin-bottom: 14px; }
         .verification-card { margin-top: 44px; padding: 24px; background: #161c2a; border: 1px solid #364052; border-radius: 16px; }
         .gate-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 0 24px; margin-top: 18px; }
         .gate-row { display: flex; align-items: center; justify-content: space-between; gap: 14px; min-height: 44px; border-top: 1px solid #364052; color: #bac3d2; font-size: .86rem; }
         .gate-row code { color: #f6d487; font: 600 .72rem ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; text-transform: uppercase; text-align: right; }
         .gate-row code[data-gate-state="blocked"] { color: #f6d487; }
         .verify-help { margin-top: 18px; border-top: 1px solid #364052; color: #bac3d2; }
         .verify-help summary { cursor: pointer; min-height: 48px; padding: 12px 0; color: #f2eee5; }
         .verify-help ol { margin: 8px 0 0; padding-left: 22px; line-height: 1.7; }
         .verify-help li + li { margin-top: 8px; }
         .verify-help code { color: #f6d487; overflow-wrap: anywhere; }
         .platform-grid { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 12px; }
        .platform-card { min-height: 260px; padding: 18px; display: flex; flex-direction: column; }
        .platform-topline { display: flex; justify-content: space-between; gap: 8px; align-items: center; }
        .platform-kind { border: 0; padding: 0; color: #91d5dc; text-transform: uppercase; font-size: 10px; }
        .platform-state { padding: 5px 7px; font-size: 9px; color: #8f9bad; }
        .platform-card p { color: #bac3d2; font-size: .87rem; line-height: 1.55; margin: 0; }
        .download-disabled, .download-button { margin-top: auto; min-height: 45px; padding: 12px 13px; border-radius: 10px; display: flex; align-items: center; justify-content: space-between; gap: 10px; font-size: .82rem; }
        .download-disabled { border: 1px dashed #586378; color: #8f9bad; }
        .download-button { background: #c6b5f0; color: #171321; font-weight: 700; text-decoration: none; }
        .platform-scope { margin-top: 10px !important; color: #8f9bad !important; font-size: .75rem !important; }
        .details-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; margin-top: 44px; }
        .detail-card { padding: 24px; }
        .detail-card ol, .detail-card ul { margin: 16px 0 0; padding-left: 20px; color: #bac3d2; line-height: 1.7; }
        .detail-card li + li { margin-top: 8px; }
        .policy-card { margin-top: 14px; padding: 24px; }
        .policy-list { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; margin: 20px 0 0; }
        .policy-list > div { border-top: 1px solid #364052; padding-top: 10px; }
        .policy-list dt { color: #8f9bad; font-size: .8rem; }
        .policy-list dd { margin: 4px 0 0; color: #f2eee5; font-size: .9rem; line-height: 1.45; }
        .footer-links { display: flex; flex-wrap: wrap; gap: 6px 20px; border-top: 1px solid #364052; margin-top: 40px; padding-top: 8px; }
        a:focus-visible { outline: 2px solid #91d5dc; outline-offset: 4px; }
        @media (max-width: 940px) { .platform-grid { grid-template-columns: repeat(3, minmax(0, 1fr)); } }
         @media (max-width: 680px) { .contributor-page { padding: 16px 16px 40px; } .return-nav { margin-bottom: 28px; } .title-row, .section-heading { display: block; } .state-badge, .closed { display: inline-flex; margin-top: 16px; } .platform-grid, .details-grid { grid-template-columns: 1fr; } .gate-grid { grid-template-columns: 1fr; } .policy-list { grid-template-columns: repeat(2, 1fr); } h1 { font-size: 2.65rem; } }
      `}</style>
    </>
  );
};
export const getStaticProps: GetStaticProps<Props> = async () => {
  const manifest = parseContributorNodeManifest(CONTRIBUTOR_NODE_MANIFEST);
  if (!manifest) throw new Error('APOCRYPHA_CONTRIBUTOR_MANIFEST_INVALID');
  return { props: { manifest } };
};

export default ApocryphaContributorNodeDownload;
