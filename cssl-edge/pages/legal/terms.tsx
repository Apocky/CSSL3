import type { NextPage } from 'next';
import LegalDocument from '@/components/LegalDocument';

const Terms: NextPage = () => (
  <LegalDocument
    title="Terms of service"
    description="Plain-language terms for using apocky.com and its current public features."
    updated="July 27, 2026"
  >
    <p>
      These terms apply when you use apocky.com or an account feature hosted on it. A downloaded program may
      also have its own license. A linked outside service or separately hosted project applies its own terms.
    </p>

    <h2>Accepting these terms</h2>
    <p>
      By using a feature covered by these terms, you agree to them. If you do not agree, do not use that
      feature. Browsing the public project links does not create a duty to register, publish, donate, or
      participate.
    </p>

    <h2>Who may use account features</h2>
    <p>
      You must be at least 13 and legally able to agree to these terms. A payment or age-restricted feature may
      require you to be older. You are responsible for complying with the laws that apply where you live.
    </p>

    <h2>Accounts</h2>
    <ul>
      <li>Provide accurate information and keep access to your sign-in method secure.</li>
      <li>Do not sell, transfer, or impersonate another person’s account.</li>
      <li>Signing out ends the local session, but does not necessarily delete the account or required records.</li>
      <li>Contact Apocky if you believe another person has accessed your account.</li>
    </ul>

    <h2>Acceptable use</h2>
    <p>Do not use the site to:</p>
    <ul>
      <li>Break the law, threaten or harass people, or violate another person’s rights.</li>
      <li>Upload malware or attempt to damage, overload, or bypass the service’s security.</li>
      <li>Access another person’s account, private content, or restricted system without permission.</li>
      <li>Misrepresent automated activity as another person’s expression.</li>
      <li>Submit content you do not have the right to share.</li>
    </ul>

    <h2>Content you submit</h2>
    <p>
      You keep ownership of content you own. When you deliberately submit content for display or distribution,
      you give Apocky a non-exclusive license to host, copy, format, and display it only as needed to provide
      that feature. You remain responsible for the content. Apocky may remove content that violates these terms
      or creates a security or legal risk.
    </p>

    <h2>External links, donations, and purchases</h2>
    <p>
      Ko-fi, Patreon, GitHub, Medium, and other linked services are operated by other organizations. Their terms
      govern activity on their sites. A donation supports the work; it does not purchase control over creative
      decisions, a person, or another participant.
    </p>
    <p>
      If apocky.com offers a direct purchase, the checkout screen must show the item, price, renewal terms if
      any, and refund terms before payment. No future product described in a plan is sold merely because it
      appears in documentation.
    </p>

    <h2>Software and intellectual property</h2>
    <p>
      Each download, source repository, and project is governed by the license displayed with it. These terms
      do not replace an open-source license or the Labyrinth of Apocalypse End-User License Agreement. Project
      names, site design, writing, and other material remain protected to the extent allowed by law.
    </p>

    <h2>Unfinished software</h2>
    <p>
      Alpha and test software is unfinished. It may fail, change, or lose data. Do not rely on it for safety-
      critical work or use it where failure could cause serious harm. Back up important files before testing.
    </p>

    <h2>No warranty</h2>
    <p>
      To the extent allowed by law, the site and unfinished software are provided “as is” and “as available,”
      without implied warranties of merchantability, fitness for a particular purpose, or non-infringement.
      Rights that cannot legally be waived remain in effect.
    </p>

    <h2>Limits on liability</h2>
    <p>
      To the extent allowed by law, Apocky’s total liability arising from these terms is limited to the greater
      of 100 US dollars or the amount you paid Apocky for the affected service during the previous twelve
      months. This limit does not apply where the law does not allow it.
    </p>

    <h2>Suspension and ending use</h2>
    <p>
      You may stop using the service at any time. Apocky may restrict an account or remove content when
      reasonably necessary to address a material breach, security risk, or legal duty. When practical, notice
      and an opportunity to correct the problem should be provided.
    </p>

    <h2>Governing law</h2>
    <p>
      Arizona law governs these terms without overriding consumer protections that must apply where you live.
      Courts in Maricopa County, Arizona have jurisdiction unless applicable law requires another forum.
    </p>

    <h2>Changes</h2>
    <p>
      The date at the top changes when these terms change. Material changes should be explained before they
      take effect when advance notice is reasonably possible. If you do not accept a change, stop using the
      affected feature.
    </p>

    <h2>Contact</h2>
    <p>
      Email <a href="mailto:apocky13@gmail.com?subject=%5Bterms%5D">apocky13@gmail.com</a> with
      <code> [terms]</code> in the subject.
    </p>

    <footer>
      <p>
        Related pages: <a href="/legal/privacy">Privacy policy</a> and{' '}
        <a href="/legal/eula">Labyrinth of Apocalypse license</a>.
      </p>
    </footer>
  </LegalDocument>
);

export default Terms;
