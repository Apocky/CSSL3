import type { NextPage } from 'next';
import LegalDocument from '@/components/LegalDocument';

const Eula: NextPage = () => (
  <LegalDocument
    title="Labyrinth of Apocalypse license"
    description="End-User License Agreement for the Labyrinth of Apocalypse alpha test build."
    updated="July 27, 2026"
  >
    <p>
      This is the End-User License Agreement (EULA) for the Labyrinth of Apocalypse alpha test build,
      including <code>LoA.exe</code> and proprietary files distributed with it. By extracting or running those
      files, you accept this agreement.
    </p>

    <h2>1. Permission to use the test build</h2>
    <p>Apocky gives you a limited, non-exclusive, non-transferable license to:</p>
    <ul>
      <li>Run the test build on personal devices you own or control.</li>
      <li>Measure its performance and take screenshots or recordings.</li>
      <li>Stream it, review it, and send feedback.</li>
    </ul>

    <h2>2. Restrictions</h2>
    <p>Unless the law gives you a right that cannot be restricted, you may not:</p>
    <ul>
      <li>Redistribute, sell, sublicense, or commercially exploit the proprietary test-build files.</li>
      <li>Decompile or extract proprietary code, compiled shaders, model data, or other protected implementation material.</li>
      <li>Use extracted proprietary material in another product without separate written permission.</li>
      <li>Remove copyright, license, or attribution notices.</li>
    </ul>

    <h2>3. Separately licensed material</h2>
    <p>
      This agreement applies only to files distributed under it. Source code, specifications, libraries, or
      other material that displays a separate open-source or proprietary license remains governed by that
      license. This agreement does not take away rights granted by a separate license.
    </p>

    <h2>4. Privacy and system access</h2>
    <p>
      The public build is intended to run without digital rights management (DRM), a rootkit, or a kernel-level
      anti-cheat driver. DRM is technology that restricts copying or use. A kernel driver runs with deep
      operating-system privileges. If a later feature uses a network, camera, microphone, or account, it must
      explain that access separately; this agreement is not permission for undisclosed collection.
    </p>

    <h2>5. Unfinished software and backups</h2>
    <p>
      This is alpha software. It is unfinished and may contain serious bugs, including bugs that damage or lose
      data. Back up important files. Do not use the test build for safety-critical work or where a failure could
      cause serious harm.
    </p>

    <h2>6. No warranty</h2>
    <p>
      To the extent allowed by law, the test build is provided “as is,” without implied warranties of
      merchantability, fitness for a particular purpose, or non-infringement. Rights that cannot legally be
      waived remain in effect.
    </p>

    <h2>7. Limits on liability</h2>
    <p>
      To the extent allowed by law, Apocky’s total liability under this agreement is limited to the greater of
      100 US dollars or the amount you paid for the affected test build. This limit does not apply where the
      law does not allow it.
    </p>

    <h2>8. Feedback</h2>
    <p>
      You are not required to provide feedback. If you deliberately send feedback, you give Apocky a
      non-exclusive, worldwide, royalty-free license to use it to develop and improve projects. This does not
      transfer ownership of unrelated material or private content you did not choose to submit.
    </p>

    <h2>9. Ending the license</h2>
    <p>
      You may stop using the test build at any time. This license ends if you materially violate it. When it
      ends, stop using and delete the proprietary files covered by it. Terms that logically need to survive,
      including warranty, liability, and governing-law terms, continue to apply.
    </p>

    <h2>10. Governing law</h2>
    <p>
      Arizona law governs this agreement without overriding consumer protections that must apply where you
      live. Courts in Maricopa County, Arizona have jurisdiction unless applicable law requires another forum.
    </p>

    <h2>11. Contact</h2>
    <p>
      Email <a href="mailto:apocky13@gmail.com?subject=%5BLoA-LICENSE%5D">apocky13@gmail.com</a> with
      <code> [LoA-LICENSE]</code> in the subject.
    </p>

    <footer>
      <p>
        Related pages: <a href="/legal/privacy">Privacy policy</a> and{' '}
        <a href="/legal/terms">Terms of service</a>.
      </p>
    </footer>
  </LegalDocument>
);

export default Eula;
