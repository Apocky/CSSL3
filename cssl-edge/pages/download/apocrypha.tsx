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
  return (
    <>
      <Head>
        <title>Apocrypha for iPhone and Android · Apocky</title>
        <meta name="description" content="Native Apocrypha apps for iPhone and Android. Check download availability, preview verification, and account access before installing." />
        <meta property="og:title" content="Apocrypha. On your phone." />
        <meta property="og:description" content="Native mobile apps, with clear release status and private account access." />
        <link rel="canonical" href="https://www.apocky.com/download/apocrypha" />
      </Head>
      <main id="main-content" className="mobile-download">
        <div className="mobile-wrap">
          <header className="mobile-hero">
            <div className="mobile-identity" aria-hidden="true"><span className="apx-brand-mark" /></div>
            <p className="eyebrow">APOCRYPHA · NATIVE MOBILE</p>
            <h1>Apocrypha.<br /><em>On your phone.</em></h1>
            <p className="lead">A dedicated app for your private conversations with Apocrypha. Choose your phone below to see what is available.</p>
            <div className="access-note"><span aria-hidden="true">◇</span><p><strong>Sign in with your Apocky account.</strong> The app uses your account for private chat and conversation history. New here? <Link href="/register?next=%2Fapocrypha">Create an account.</Link></p></div>
          </header>

          <section className="platforms" aria-label="Choose your phone">
            <article className="platform android" aria-labelledby="android-title">
              <div className="platform-heading"><span className="platform-icon" aria-hidden="true">↧</span><span className="badge">{apk ? 'Preview available' : 'In preparation'}</span></div>
              <h2 id="android-title">Android</h2>
              <p>A native Android app, installed directly from apocky.com.</p>
              <div className="platform-action">
                {apk ? <a className="download-button" href={apk.href} download>Download Android preview <span aria-hidden="true">↓</span></a>
                  : <p className="unavailable">The signed Android download is being prepared.</p>}
                {apk ? <p className="fine">Version {release.version} · {apk.bytes < 1024 * 1024 ? `${Math.ceil(apk.bytes / 1024)} KB` : `${(apk.bytes / (1024 * 1024)).toFixed(1)} MB`} · APK</p>
                  : <p className="fine">The download will appear here when the package and its checksum are available.</p>}
              </div>
              {apk ? <details className="integrity"><summary>Check this download</summary><p>Compare the downloaded file with its SHA-256 checksum.</p><a href={`${apk.href}.sha256`}>Open checksum file</a><dl><dt>File SHA-256</dt><dd><code>{apk.sha256}</code></dd><dt>Signing certificate SHA-256</dt><dd><code>{apk.signing_certificate_sha256}</code></dd></dl></details> : null}
            </article>

            <article className="platform iphone" aria-labelledby="iphone-title">
              <div className="platform-heading"><span className="platform-icon" aria-hidden="true">↗</span><span className="badge">{iphone ? 'Available' : 'In preparation'}</span></div>
              <h2 id="iphone-title">iPhone</h2>
              <p>A native iPhone app, delivered through Apple’s installation service.</p>
              <div className="platform-action">
                {iphone ? <a className="download-button apple" href={iphone.url} rel="noopener noreferrer">{iphone.channel === 'testflight' ? 'Open in TestFlight' : 'View on the App Store'} <span aria-hidden="true">↗</span></a>
                  : <p className="unavailable">The iPhone build is being prepared for distribution.</p>}
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
            <div><h3>Your conversations stay yours.</h3><p>Sign-in is checked by apocky.com. Your conversation history is linked to your account.</p></div>
            <div><h3>Start here next time.</h3><p>This page is the download location for both phones. Each install link appears only when that platform’s release is available.</p></div>
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
      `}</style>
    </>
  );
};

export const getStaticProps: GetStaticProps<Props> = async () => ({
  props: { release: loadNativeMobileRelease() },
});

export default ApocryphaDownload;
