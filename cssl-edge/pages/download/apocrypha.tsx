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
        .mobile-download { color: var(--apx-ink); background: radial-gradient(ellipse at 50% 0, rgba(109,93,252,.13), transparent 55%); padding: 64px 24px 72px; }
        .mobile-wrap { max-width: 1060px; margin: 0 auto; }
        .mobile-hero { max-width: 760px; margin: 0 auto 48px; text-align: center; }
        .mobile-identity { width: 68px; height: 68px; display: grid; place-items: center; margin: 0 auto 28px; border: 1px solid var(--apx-line); border-radius: 20px; background: rgba(185,152,255,.07); }
        .mobile-identity :global(.apx-brand-mark) { width: 36px; height: 36px; }
        .eyebrow { color: var(--apx-violet); font: 600 11px/1.5 var(--apx-mono); letter-spacing: .16em; }
        h1 { margin: 18px 0 24px; font-size: clamp(44px, 7vw, 76px); line-height: 1.04; letter-spacing: -.055em; }
        h1 em { color: #c7b3ff; font-family: var(--apx-display); font-weight: 400; }
        .lead { max-width: 590px; margin: 0 auto; font-size: 18px; line-height: 1.7; color: var(--apx-copy); }
        .access-note { display: flex; gap: 14px; align-items: flex-start; text-align: left; margin: 28px auto 0; max-width: 620px; padding: 16px 20px; border: 1px solid var(--apx-line); border-radius: 14px; color: var(--apx-muted); }
        .access-note > span { color: var(--apx-violet); font-size: 24px; line-height: 1.3; }
        .access-note p { margin: 0; font-size: 13px; line-height: 1.7; } .access-note strong { color: var(--apx-ink); }
        .access-note :global(a) { color: var(--apx-mint); text-underline-offset: 3px; }
        .platforms { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 20px; }
        .platform { background: linear-gradient(155deg, rgba(20,24,50,.83), rgba(7,9,21,.96)); border: 1px solid var(--apx-line); padding: 32px; border-radius: 24px; display: flex; flex-direction: column; }
        .platform-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 20px; }
        .platform-icon { font-size: 30px; color: var(--apx-mint); }
        .iphone .platform-icon { color: var(--apx-violet); }
        .badge { font: 500 11px/1.3 var(--apx-mono); color: var(--apx-copy); border: 1px solid var(--apx-line); border-radius: 20px; padding: 7px 11px; }
        h2 { font-size: 28px; letter-spacing: -.035em; line-height: 1.2; margin: 0 0 12px; }
        .platform > p { margin: 0; line-height: 1.7; color: var(--apx-muted); max-width: 330px; }
        .platform-action { margin-top: 28px; }
        .download-button { display: flex; align-items: center; justify-content: space-between; gap: 16px; min-height: 54px; padding: 14px 18px; border-radius: 12px; color: #06111a; background: var(--apx-mint); font-weight: 650; text-decoration: none; }
        .download-button.apple { background: #cfbcff; color: #120a20; }
        .unavailable { margin: 0; min-height: 56px; padding: 14px 16px; border-radius: 12px; border: 1px dashed rgba(169,181,255,.3); color: var(--apx-copy); font-size: 14px; line-height: 1.7; }
        .fine { color: var(--apx-muted); font-size: 12px; line-height: 1.7; margin: 12px 0 0; }
        .integrity { border-top: 1px solid var(--apx-line); margin-top: 24px; padding-top: 8px; font-size: 12px; color: var(--apx-muted); }
        .integrity summary { cursor: pointer; min-height: 44px; padding: 13px 0; color: var(--apx-copy); }
        .integrity a { color: var(--apx-mint); display: inline-block; padding: 12px 0; }
        .integrity p { line-height: 1.7; } .integrity dt { margin: 12px 0 5px; } .integrity dd { margin: 0; } .integrity code { overflow-wrap: anywhere; font: 11px/1.8 var(--apx-mono); }
        .verification { margin-top: 24px; border-bottom: 1px solid var(--apx-line); }
        .verification > summary { min-height: 56px; padding: 18px 0; font-size: 14px; color: var(--apx-copy); cursor: pointer; }
        .verification-body { display: grid; grid-template-columns: 1fr 1fr; gap: 44px; align-items: start; padding-bottom: 26px; }
        .verification p { color: var(--apx-muted); font-size: 14px; line-height: 1.75; margin: 0; }
        .checklist { margin: 0; } .checklist > div { display: flex; align-items: center; justify-content: space-between; gap: 14px; border-bottom: 1px solid var(--apx-line); min-height: 49px; }
        .checklist dt { color: var(--apx-copy); font-size: 13px; } .checklist dd { margin: 0; font: 11px var(--apx-mono); color: var(--apx-violet); }
        .next-steps { display: grid; grid-template-columns: 1fr 1fr; gap: 44px; padding: 34px 0 28px; } .next-steps h3 { font-size: 15px; margin: 0 0 8px; } .next-steps p { font-size: 13px; line-height: 1.8; color: var(--apx-muted); margin: 0; }
        .mobile-footer { display: flex; align-items: center; flex-wrap: wrap; gap: 8px 24px; border-top: 1px solid var(--apx-line); padding-top: 16px; font-size: 12px; }
        .mobile-footer :global(a) { color: var(--apx-copy); min-height: 44px; display: inline-flex; align-items: center; text-underline-offset: 4px; }
        a:focus-visible, summary:focus-visible { outline: 2px solid var(--apx-mint); outline-offset: 5px; }
        @media (max-width: 680px) { .mobile-download { padding: 36px 18px 44px; } .mobile-hero { margin-bottom: 30px; } .lead { font-size: 16px; } .platforms, .verification-body, .next-steps { grid-template-columns: 1fr; } .platform { padding: 24px; border-radius: 20px; } .verification-body { gap: 20px; } .next-steps { gap: 22px; } .access-note { padding: 14px; } .mobile-footer { gap: 2px 20px; } }
      `}</style>
    </>
  );
};

export const getStaticProps: GetStaticProps<Props> = async () => ({
  props: { release: loadNativeMobileRelease() },
});

export default ApocryphaDownload;
