// The licence for the Apocrypha apps.
//
// The terms of service say "each download, source repository, and project is governed by the
// license displayed with it" — and the Apocrypha downloads displayed none. The only EULA on the
// site covers the archived Labyrinth of Apocalypse test build, which is a different product that
// is no longer distributed. So apocky.com was shipping signed-in desktop and Android software
// under terms that did not exist.
//
// This closes that gap. It is modelled on the Labyrinth EULA deliberately: same structure, same
// plain-language register, same governing law and contact convention, so the two read as one
// house style rather than two unrelated documents.

import type { NextPage } from 'next';
import LegalDocument from '@/components/LegalDocument';

const ApocryphaLicense: NextPage = () => (
  <LegalDocument
    title="Apocrypha app license"
    description="End-User License Agreement for the Apocrypha desktop and Android applications distributed from apocky.com."
    updated="September 15, 2026"
  >
    <p>
      This is the End-User License Agreement (EULA) for the Apocrypha applications distributed from
      apocky.com — the Windows installer and the Android package — and the files installed with them.
      By installing or running them, you accept this agreement.
    </p>
    <p>
      Using Apocrypha in a web browser is covered by the{' '}
      <a href="/legal/terms">terms of service</a> instead. This agreement is about the copies you
      install on your own device.
    </p>

    <h2>1. Permission to use the app</h2>
    <p>Apocky gives you a limited, non-exclusive, non-transferable license to:</p>
    <ul>
      <li>Install and run Apocrypha on personal devices you own or control.</li>
      <li>Use it with your own Apocky account, or signed out where the app allows it.</li>
      <li>Measure its performance, take screenshots or recordings, review it, and send feedback.</li>
    </ul>

    <h2>2. Restrictions</h2>
    <p>Unless the law gives you a right that cannot be restricted, you may not:</p>
    <ul>
      <li>Redistribute, sell, sublicense, or commercially exploit the application files.</li>
      <li>Decompile or extract proprietary code or other protected implementation material.</li>
      <li>Use extracted proprietary material in another product without separate written permission.</li>
      <li>Remove copyright, license, or attribution notices.</li>
      <li>
        Use another person&rsquo;s account, or work around the account limits, rate limits, or
        capacity controls the service applies.
      </li>
    </ul>

    <h2>3. Separately licensed material</h2>
    <p>
      This agreement applies only to files distributed under it. Source code, specifications,
      libraries, or other material that displays a separate open-source or proprietary license
      remains governed by that license. This agreement does not take away rights granted by a
      separate license.
    </p>

    <h2>4. What the app sends, and what it does not</h2>
    <p>
      Apocrypha is a client for a service. When you send a message, that message is transmitted to
      the Apocrypha service to be answered; answers come back the same way. What you type is not
      used to train a model. Signed-in conversations are stored against your account so they follow
      you between devices; a signed-out conversation is kept in the app on that device, and the
      copy the service needs to produce an answer is deleted on a fixed schedule described in the{' '}
      <a href="/legal/privacy">privacy policy</a>.
    </p>
    <p>
      The app is intended to run without digital rights management (DRM), a rootkit, or a
      kernel-level driver. DRM is technology that restricts copying or use. A kernel driver runs
      with deep operating-system privileges. If a later feature uses a camera, microphone,
      location, or contacts, it must ask for that access separately; this agreement is not
      permission for undisclosed collection.
    </p>
    <p>
      Your sign-in is stored on your own device, encrypted under your operating-system account, and
      signing out inside the app deletes it.
    </p>

    <h2>5. Availability</h2>
    <p>
      Apocrypha answers on a single machine operated by Apocky. It can be slow, rate-limited, or
      briefly unavailable, and no uptime is promised. Access can be limited or withdrawn where it is
      needed to protect the service or other people using it.
    </p>

    <h2>6. Unfinished software and backups</h2>
    <p>
      These are preview builds. They are unfinished and may contain serious bugs, including bugs
      that damage or lose data. Back up important files. Do not use Apocrypha for safety-critical
      work or where a failure could cause serious harm, and do not rely on an answer for medical,
      legal, financial, or other professional advice.
    </p>

    <h2>7. No warranty</h2>
    <p>
      To the extent allowed by law, the applications are provided &ldquo;as is,&rdquo; without
      implied warranties of merchantability, fitness for a particular purpose, or non-infringement.
      Rights that cannot legally be waived remain in effect.
    </p>

    <h2>8. Limits on liability</h2>
    <p>
      To the extent allowed by law, Apocky&rsquo;s total liability under this agreement is limited
      to the greater of 100 US dollars or the amount you paid for the affected application. This
      limit does not apply where the law does not allow it.
    </p>

    <h2>9. Feedback</h2>
    <p>
      You are not required to provide feedback. If you deliberately send feedback, you give Apocky a
      non-exclusive, worldwide, royalty-free license to use it to develop and improve projects. This
      does not transfer ownership of unrelated material or private content you did not choose to
      submit.
    </p>

    <h2>10. Ending the license</h2>
    <p>
      You may stop using Apocrypha at any time and uninstall it. This license ends if you materially
      violate it. When it ends, stop using and delete the application files covered by it. Terms that
      logically need to survive, including warranty, liability, and governing-law terms, continue to
      apply.
    </p>

    <h2>11. Changes to this agreement</h2>
    <p>
      If these terms change, the revision date above changes with them and the previous text stays
      recoverable from the site&rsquo;s public history. A copy you already installed keeps working;
      the terms you agreed to are not rewritten behind you.
    </p>

    <h2>12. Governing law</h2>
    <p>
      Arizona law governs this agreement without overriding consumer protections that must apply
      where you live. Courts in Maricopa County, Arizona have jurisdiction unless applicable law
      requires another forum.
    </p>

    <h2>13. Contact</h2>
    <p>
      Email{' '}
      <a href="mailto:apocky13@gmail.com?subject=%5Bapocrypha-license%5D">apocky13@gmail.com</a> with{' '}
      <code>[apocrypha-license]</code> in the subject.
    </p>
  </LegalDocument>
);

export default ApocryphaLicense;
