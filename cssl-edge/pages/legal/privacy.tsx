import type { NextPage } from 'next';
import LegalDocument from '@/components/LegalDocument';

const Privacy: NextPage = () => (
  <LegalDocument
    title="Privacy policy"
    description="What apocky.com collects, why it is used, and the choices available to visitors."
    updated="July 27, 2026"
  >
    <p>
      This policy covers apocky.com and the website features it currently provides. A separately hosted project
      may have its own policy. Links to services such as Ko-fi, Patreon, GitHub, Medium, and other project sites
      take you to services operated by other organizations.
    </p>

    <h2>Short version</h2>
    <ul>
      <li>Apocky does not sell personal information or use it for targeted advertising.</li>
      <li>Optional diagnostic sharing is off until you save a choice.</li>
      <li>The site can still be used when optional diagnostic sharing is off.</li>
      <li>The public homepage does not require an account.</li>
      <li>Do not upload a log, image, or other file until you have checked it for private information.</li>
    </ul>

    <h2>Information the site may process</h2>
    <h3>Basic web requests</h3>
    <p>
      Hosting and security systems may process an internet address, browser information, requested page,
      timestamp, and response status to deliver the site and respond to abuse or failures. These are ordinary
      server records, not a promise that no infrastructure provider ever receives an internet address.
    </p>

    <h3>Optional site data</h3>
    <p>
      If you choose to share it, the site may send limited diagnostic information. The choice panel describes
      each level before you save it. Depending on the selected level, this can include page errors, timing
      measurements, or technical details needed to reproduce a failure. “Diagnostics” means information used
      to find and fix technical problems.
    </p>

    <h3>Account information</h3>
    <p>
      If you use an account feature, the authentication provider may supply an email address, provider account
      identifier, display name, and session information. A <strong>session</strong> is the temporary record that
      keeps you signed in. The public homepage and project links do not require this.
    </p>

    <h3>Payments and external support</h3>
    <p>
      Ko-fi, Patreon, and other linked support services apply their own privacy policies. If apocky.com offers a
      direct payment screen, the payment processor handles card details; Apocky may receive transaction
      identifiers, status, amount, and contact information needed for the transaction or refund.
    </p>

    <h3>Content you deliberately submit</h3>
    <p>
      A publishing or sharing feature processes the text, file, title, account attribution, and settings you
      choose to submit. Nothing on the public homepage requires you to publish. Review private material before
      submitting it.
    </p>

    <h2>Camera, microphone, and local files</h2>
    <p>
      The public homepage does not request camera or microphone access. A separate private experience may offer
      those inputs, but the browser and the page must ask before access begins. A local game log, screenshot, or
      save file remains on your computer unless you choose a feature that uploads it.
    </p>

    <h2>Browser storage and cookies</h2>
    <p>
      The site can use browser storage to remember your optional-data choice and interface preferences. Account
      features use session storage or cookies to keep you signed in. Blocking or clearing this storage may sign
      you out or reset preferences.
    </p>
    <p>
      Public quests and the symbolic Spellbook use separate versioned browser-local records. Quest progress is
      saved as completion markers. The Spellbook stores only workings you explicitly choose to save, including
      their source text, symbolic plan, interpretation, and integrity receipt. Oracle questions and unsaved
      Spellcraft input are not written by those tools. The symbolic tools do not send question or working text
      through site telemetry, and the Spellbook provides export, delete-one, and delete-all controls.
    </p>

    <h2>Why information is used</h2>
    <ul>
      <li>To deliver pages and files you request.</li>
      <li>To keep an account signed in when you choose to use one.</li>
      <li>To complete and document a transaction you initiate.</li>
      <li>To display content you deliberately submit.</li>
      <li>To secure the service and diagnose errors.</li>
    </ul>

    <h2>Sharing with service providers</h2>
    <p>
      Hosting, authentication, database, email, and payment providers may process information only as needed to
      provide their services. Their own systems, locations, and retention practices also apply. Apocky may
      disclose information when legally required or when reasonably necessary to protect people and the service.
    </p>

    <h2>Retention and deletion</h2>
    <p>
      Different records need different retention periods. Session data may be short-lived; security and
      transaction records may need to remain longer. This policy does not promise an automatic deletion period
      that the current system has not verified. You may ask what is held about you or request correction or
      deletion. Some records may need to remain when required by law, fraud prevention, or an unresolved dispute.
    </p>

    <h2>Your choices</h2>
    <ul>
      <li>Keep optional site data off or change the saved level.</li>
      <li>Use the public pages without creating an account.</li>
      <li>Sign out and clear browser storage.</li>
      <li>Export or delete one or all device-local Spellbook entries.</li>
      <li>Choose whether to submit or upload content.</li>
      <li>Ask for access, correction, export, or deletion of personal information associated with you.</li>
    </ul>

    <h2>Children</h2>
    <p>
      The service is not directed to children under 13, and Apocky does not knowingly collect their personal
      information. Contact Apocky if you believe a child has submitted personal information.
    </p>

    <h2>Security</h2>
    <p>
      Reasonable technical and organizational measures are used to protect information, but no website or
      storage system can promise perfect security. Report a suspected security issue privately rather than
      posting sensitive details in public.
    </p>

    <h2>Contact</h2>
    <p>
      For a privacy request, email{' '}
      <a href="mailto:apocky13@gmail.com?subject=%5Bprivacy%5D">apocky13@gmail.com</a> with
      <code> [privacy]</code> in the subject.
    </p>

    <h2>Changes to this policy</h2>
    <p>
      The date at the top changes when this policy changes. Material changes should be explained before they
      take effect when advance notice is reasonably possible.
    </p>

    <footer>
      <p>
        Related pages: <a href="/legal/terms">Terms of service</a> and{' '}
        <a href="/legal/eula">Labyrinth of Apocalypse license</a>.
      </p>
    </footer>
  </LegalDocument>
);

export default Privacy;
