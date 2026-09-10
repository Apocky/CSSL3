import type { GetStaticProps, NextPage } from 'next';
import React from 'react';
import Head from 'next/head';
import Link from 'next/link';
import { loadNativeMobileRelease } from '@/lib/mobile/release-server';
import type { NativeMobileRelease } from '@/lib/mobile/release';

interface Props { release: NativeMobileRelease }

const ApocryphaDownload: NextPage<Props> = ({ release }) => {
  const apk = release.android.state === 'ready' ? release.android.artifact : null;
  const iphone = release.ios.state === 'ready' ? release.ios.distribution : null;
  const checks = release.android.verification;
  const pendingChecks = [
    checks.account_sign_in_and_chat !== 'passed' ? 'account sign-in and chat' : null,
    checks.physical_device !== 'passed' ? 'testing on a physical phone' : null,
  ].filter(Boolean);
  return (
    <>
      <Head>
        <title>Apocrypha downloads &amp; phone availability · Apocky</title>
        <meta name="description" content="Check the Android preview and iPhone availability, download an available release, or continue to Apocrypha in your browser." />
        <meta property="og:title" content="Apocrypha. On your phone." />
        <meta property="og:description" content="Android preview downloads, iPhone availability, and a browser option." />
        <link rel="canonical" href="https://www.apocky.com/download/apocrypha" />
      </Head>
      <main id="main-content" className="mobile-download">
        <div className="mobile-wrap">
          <nav className="page-return" aria-label="Download navigation"><Link href="/">← Home</Link><Link href="/apocrypha">Chat in your browser →</Link></nav>
          <header className="mobile-hero">
            <p className="eyebrow">Apocrypha · Downloads</p>
            <h1>Apocrypha <em>on your phone.</em></h1>
            <p className="lead">Choose your phone to see the available download. You can also use Apocrypha in your browser.</p>
            <div className="access-note"><span aria-hidden="true">◇</span><p><strong>Sign in with your Apocky account.</strong> The app uses your account for private chat and conversation history. New here? <Link href="/register?next=%2Fapocrypha">Create an account.</Link></p></div>
          </header>

          <section className="platforms" aria-label="Choose your phone">
            <article className="platform android" aria-labelledby="android-title">
              <div className="platform-heading"><span className="platform-icon" aria-hidden="true">↧</span><span className="badge">{apk ? 'Preview available' : 'Not available yet'}</span></div>
              <h2 id="android-title">Android</h2>
              <p>{apk ? 'Install the Android preview directly from apocky.com.' : 'The Android download will appear here when the preview is available.'}</p>
              <div className="platform-action">
                {apk ? <a className="download-button" href={apk.href} download>Download Android preview <span aria-hidden="true">↓</span></a>
                  : <p className="unavailable">No Android download is available yet.</p>}
                {apk ? <p className="fine">Version {release.version} · {apk.bytes < 1024 * 1024 ? `${Math.ceil(apk.bytes / 1024)} KB` : `${(apk.bytes / (1024 * 1024)).toFixed(1)} MB`} · APK</p>
                  : <p className="fine">The download will appear here when the package and its checksum are available.</p>}
              </div>
              {apk && pendingChecks.length ? <p className="preview-note"><strong>Checks still pending:</strong> {pendingChecks.join(' and ')}. This is a preview release.</p> : null}
              {apk ? <details className="install-help"><summary>How to install on Android</summary><ol><li>Download the APK file to your Android phone.</li><li>Open the file. If Android asks, allow this installation from your browser or file manager.</li><li>Open Apocrypha and sign in with your Apocky account.</li></ol><p>You can return to this page for the available version.</p></details> : null}
              {apk ? <details className="integrity"><summary>Check this download</summary><p>Compare the downloaded file with its SHA-256 checksum.</p><a href={`${apk.href}.sha256`}>Open checksum file</a><dl><dt>File SHA-256</dt><dd><code>{apk.sha256}</code></dd><dt>Signing certificate SHA-256</dt><dd><code>{apk.signing_certificate_sha256}</code></dd></dl></details> : null}
            </article>

            <article className="platform iphone" aria-labelledby="iphone-title">
              <div className="platform-heading"><span className="platform-icon" aria-hidden="true">↗</span><span className="badge">{iphone ? 'Available' : 'Not available yet'}</span></div>
              <h2 id="iphone-title">iPhone</h2>
              <p>{iphone ? 'Open the available iPhone release through Apple’s installation service.' : 'There is no App Store or TestFlight release to install yet.'}</p>
              <div className="platform-action">
                {iphone ? <a className="download-button apple" href={iphone.url} rel="noopener noreferrer">{iphone.channel === 'testflight' ? 'Open in TestFlight' : 'View on the App Store'} <span aria-hidden="true">↗</span></a>
                  : <p className="unavailable">The iPhone download is not available yet.</p>}
                <p className="fine">{iphone ? 'Apple will guide you through installation.' : 'An App Store or TestFlight link will appear here after signing and distribution are ready.'}</p>
              </div>
            </article>
          </section>

          <details className="verification">
            <summary>Preview testing details</summary>
            <div className="verification-body">
            <p>A downloadable preview may still have unfinished checks. These are the recorded Android verification results for this version.</p>
            <dl className="checklist">
              <div><dt>Package signature</dt><dd>{checks.package_signature === 'verified' ? 'Verified' : 'Pending'}</dd></div>
              <div><dt>Emulator launch</dt><dd>{checks.emulator_launch === 'passed' ? 'Passed' : 'Pending'}</dd></div>
              <div><dt>Account sign-in and chat</dt><dd>{checks.account_sign_in_and_chat === 'passed' ? 'Passed' : 'Pending'}</dd></div>
              <div><dt>Physical phone check</dt><dd>{checks.physical_device === 'passed' ? 'Passed' : 'Pending'}</dd></div>
            </dl>
            </div>
          </details>

          <section className="next-steps" aria-label="Before you install">
            <div><h3>Prefer to use your browser?</h3><p><Link href="/apocrypha">Open Apocrypha online.</Link> Sign in with your Apocky account to use private chat and your conversation history.</p></div>
            <div><h3>Check back here for releases.</h3><p>Each install link appears only when that platform’s release is available. Preview testing details stay visible above.</p></div>
          </section>
          <footer className="mobile-footer"><Link href="/apocrypha">Chat in your browser →</Link><Link href="/legal/privacy">Privacy</Link><Link href="/download">Labyrinth of Apocalypse downloads</Link><a href="/releases/apocrypha-mobile/manifest.json">Release details</a></footer>
        </div>
      </main>
      <style jsx>{`
        .mobile-download { color: var(--apx-ink); background: radial-gradient(ellipse at 50% 0, rgba(109,93,252,.13), transparent 55%); padding: 28px 20px 40px; }
        .mobile-wrap { max-width: 1060px; margin: 0 auto; }
        .mobile-hero { max-width: 720px; margin: 0 auto 20px; text-align: center; }
        .mobile-identity { width: 44px; height: 44px; display: grid; place-items: center; margin: 0 auto 12px; border: 1px solid var(--apx-line); border-radius: 14px; background: rgba(185,152,255,.07); }
        .mobile-identity :global(.apx-brand-mark) { width: 30px; height: 30px; }
        .eyebrow { margin: 0; color: var(--apx-violet); font: 600 11px/1.5 var(--apx-mono); letter-spacing: .16em; }
        h1 { margin: 8px 0 12px; font-size: clamp(2rem, 4vw, 3rem); line-height: 1.05; letter-spacing: -.045em; }
        h1 em { color: #c7b3ff; font-family: var(--apx-display); font-weight: 400; }
        .lead { max-width: 560px; margin: 0 auto; font-size: 1rem; line-height: 1.6; color: var(--apx-copy); }
        .access-note { display: flex; gap: 12px; align-items: flex-start; text-align: left; margin: 20px auto 0; max-width: 600px; padding: 12px 14px; border: 1px solid var(--apx-line); border-radius: 12px; color: var(--apx-muted); }
        .access-note > span { color: var(--apx-violet); font-size: 20px; line-height: 1.2; }
        .access-note p { margin: 0; font-size: .85rem; line-height: 1.6; } .access-note strong { color: var(--apx-ink); }
        .access-note :global(a) { color: var(--apx-mint); text-underline-offset: 3px; padding: 14px 0; }
        .platforms { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
        .platform { background: linear-gradient(155deg, rgba(20,24,50,.83), rgba(7,9,21,.96)); border: 1px solid var(--apx-line); padding: 20px; border-radius: 16px; display: flex; flex-direction: column; }
        .platform-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 8px; }
        .platform-icon { font-size: 22px; line-height: 1; color: var(--apx-mint); }
        .iphone .platform-icon { color: var(--apx-violet); }
        .badge { font: 500 .68rem/1.3 var(--apx-mono); color: var(--apx-copy); border: 1px solid var(--apx-line); border-radius: 20px; padding: 5px 10px; }
        h2 { font-size: 1.4rem; letter-spacing: -.03em; line-height: 1.2; margin: 0 0 8px; }
        .platform > p { margin: 0; font-size: .9rem; line-height: 1.55; color: var(--apx-muted); max-width: 34ch; }
        .platform-action { margin-top: 16px; }
        .download-button { display: flex; align-items: center; justify-content: space-between; gap: 16px; min-height: 48px; padding: 12px 16px; border-radius: 11px; color: #06111a; background: var(--apx-mint); font-weight: 650; text-decoration: none; }
        .download-button.apple { background: #cfbcff; color: #120a20; }
        .unavailable { margin: 0; min-height: 48px; padding: 12px 14px; border-radius: 11px; border: 1px dashed rgba(169,181,255,.3); color: var(--apx-copy); font-size: .875rem; line-height: 1.5; }
        .fine { color: var(--apx-muted); font-size: .8rem; line-height: 1.55; margin: 10px 0 0; }
        .integrity { border-top: 1px solid var(--apx-line); margin-top: 14px; padding-top: 2px; font-size: .8rem; color: var(--apx-muted); }
        .integrity summary { cursor: pointer; min-height: 44px; padding: 11px 0; color: var(--apx-copy); }
        .integrity a { color: var(--apx-mint); display: inline-block; padding: 12px 0; }
        .integrity p { margin: 0; line-height: 1.6; } .integrity dt { margin: 10px 0 4px; } .integrity dd { margin: 0; } .integrity code { overflow-wrap: anywhere; font: 11px/1.7 var(--apx-mono); }
        .verification { margin-top: 14px; border-bottom: 1px solid var(--apx-line); }
        .verification > summary { min-height: 48px; padding: 12px 0; font-size: .9rem; color: var(--apx-copy); cursor: pointer; }
        .verification-body { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; align-items: start; padding-bottom: 16px; }
        .verification p { color: var(--apx-muted); font-size: .875rem; line-height: 1.6; margin: 0; }
        .checklist { margin: 0; } .checklist > div { display: flex; align-items: center; justify-content: space-between; gap: 14px; border-bottom: 1px solid var(--apx-line); min-height: 44px; }
        .checklist > div:last-child { border-bottom: 0; }
        .checklist dt { color: var(--apx-copy); font-size: .85rem; } .checklist dd { margin: 0; font: 11px var(--apx-mono); color: var(--apx-violet); }
        .next-steps { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; padding: 18px 0 14px; } .next-steps h3 { font-size: .95rem; margin: 0 0 4px; } .next-steps p { font-size: .85rem; line-height: 1.6; color: var(--apx-muted); margin: 0; }
        .mobile-footer { display: flex; align-items: center; flex-wrap: wrap; gap: 0 12px; border-top: 1px solid var(--apx-line); padding-top: 6px; font-size: .8rem; }
        .mobile-footer :global(a) { color: var(--apx-copy); min-height: 44px; display: inline-flex; align-items: center; padding: 0 6px; margin: 0 -6px; text-underline-offset: 4px; }
        a:focus-visible, summary:focus-visible { outline: 2px solid var(--apx-mint); outline-offset: 4px; }
        @media (max-width: 680px) { .mobile-download { padding: 22px 16px 32px; } .mobile-hero { margin-bottom: 16px; } .lead { font-size: .95rem; } .platforms, .verification-body, .next-steps { grid-template-columns: 1fr; } .platform { padding: 18px; } .verification-body { gap: 12px; } .next-steps { gap: 14px; padding: 16px 0 12px; } .access-note { padding: 12px; } .mobile-footer { gap: 0 8px; } }
        .mobile-download { background: #0e121c; color: #f2eee5; padding: 20px 24px 48px; }
        .page-return { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 4px 20px; margin-bottom: 24px; }
        .page-return :global(a) { display: inline-flex; align-items: center; min-height: 44px; color: #91d5dc; font-size: 15px; text-underline-offset: 4px; }
        .mobile-hero { text-align: left; margin: 0 0 28px; max-width: 760px; }
        .eyebrow { color: #c6b5f0; font: 600 13px/1.5 var(--apx-sans, system-ui); letter-spacing: .04em; }
        h1 { margin-top: 12px; max-width: 18ch; font-size: clamp(36px, 5vw, 56px); line-height: 1.12; }
        h1 em { color: #c6b5f0; }
        .lead { max-width: 58ch; margin: 0; color: #bac3d2; font-size: 18px; line-height: 1.7; }
        .access-note { margin: 20px 0 0; max-width: 680px; background: #161c2a; border-color: #364052; color: #bac3d2; }
        .access-note p { font-size: 16px; }
        .platforms { gap: 20px; align-items: start; }
        .platform { padding: 26px; background: #161c2a; border-color: #364052; }
        .platform > p { font-size: 16px; color: #bac3d2; max-width: 38ch; }
        .platform-heading { margin-bottom: 18px; }
        .badge { color: #c6b5f0; font: 500 13px/1.4 var(--apx-sans, system-ui); border-color: #364052; }
        h2 { font-size: 28px; }
        .download-button { background: #c6b5f0; color: #171321; font-size: 16px; }
        .download-button:hover { background: #dacdf6; }
        .unavailable { color: #bac3d2; border-color: #586378; font-size: 16px; }
        .fine { color: #bac3d2; font-size: 14px; }
        .preview-note { border-left: 3px solid #c6b5f0; padding: 0 0 0 12px; margin-top: 20px !important; font-size: 15px !important; }
        .install-help { margin-top: 18px; color: #bac3d2; border-top: 1px solid #364052; }
        .install-help summary { cursor: pointer; min-height: 48px; padding: 12px 0; color: #f2eee5; font-size: 16px; }
        .install-help ol { padding-left: 22px; margin: 8px 0; font-size: 16px; line-height: 1.7; }
        .install-help li + li { margin-top: 10px; }
        .install-help p { margin: 12px 0 0; font-size: 15px; line-height: 1.7; }
        .integrity { font-size: 14px; border-color: #364052; }
        .integrity summary, .verification > summary { font-size: 16px; }
        .integrity code { font-size: 12px; }
        .verification p, .checklist dt { font-size: 16px; }
        .checklist dd { font-size: 13px; }
        .next-steps { padding-block: 24px; }
        .next-steps h3 { font-size: 18px; }
        .next-steps p { font-size: 16px; }
        .next-steps :global(a) { color: #91d5dc; text-underline-offset: 4px; }
        .mobile-footer { font-size: 14px; }
        @media (max-width: 680px) { .mobile-download { padding: 12px 18px 32px; } .page-return { margin-bottom: 20px; } .mobile-hero { margin-bottom: 24px; } h1 { font-size: 36px; } .lead { font-size: 16px; } .platform { padding: 22px; } .access-note { padding: 14px; } .platforms { gap: 16px; } }
      `}</style>
    </>
  );
};

export const getStaticProps: GetStaticProps<Props> = async () => ({
  props: { release: loadNativeMobileRelease() },
});

export default ApocryphaDownload;
