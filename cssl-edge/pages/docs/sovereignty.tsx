// apocky.com/docs/sovereignty

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="sovereignty"
      title="Permissions and data sharing · Apocky Documentation"
      description="What the current public website and LoA test build do with permissions and data, separated from future design plans."
    >
      <h1 className="docs-h1">Permissions and data sharing</h1>
      <p className="docs-blurb">
        What is available now, what remains a design, and what you can turn off.
      </p>

      <Callout kind="warn" title="Current behavior and future plans are different">
        Older versions of this page described a proposed permission system as if every part already worked.
        That was not clear enough. This page now separates observed public behavior, release statements, and
        planned architecture. A design document is not proof that a feature is running.
      </Callout>

      <h2 className="docs-h2">The public website</h2>
      <p className="docs-p">
        The website can offer optional diagnostic data sharing. Diagnostic data means information used to find
        errors or performance problems. It is off until you save a sharing choice, and the site should continue
        to work when it is off. You can reopen the choice and turn it off.
      </p>
      <p className="docs-p">
        Signing in, buying something, or publishing content may require additional information for that specific
        action. The relevant screen must explain what it needs before you continue. See the{' '}
        <a href="/legal/privacy" style={{ color: '#7dd3fc' }}>privacy policy</a> for the legal details.
      </p>

      <h2 className="docs-h2">The current LoA test build</h2>
      <p className="docs-p">
        Labyrinth of Apocalypse (LoA) is currently offered as an early Windows test build. The release is
        intended to run locally, meaning on your own computer. It has no copy-protection system and no
        operating-system-level anti-cheat driver. Those statements do not prove that every future feature will
        be offline or that every unfinished component has passed a privacy audit.
      </p>
      <p className="docs-p">
        The safest practical control is still your operating system and network firewall. If a future release
        adds an online feature, its purpose, destination, data, and off switch must be explained before use.
      </p>

      <h2 className="docs-h2">What “cap” means in technical material</h2>
      <p className="docs-p">
        Some source documents use <strong>cap</strong> as shorthand for <strong>capability</strong>. In
        computing, a capability is a permission represented in code. The phrase{' '}
        <code className="docs-ic">sovereign-cap</code> is an internal engineering term for such an
        authorization. It is not a claim that a machine, program, or tool is a sovereign person.
      </p>
      <p className="docs-p">
        The current public test build does not provide a complete, verified visitor-facing control panel for
        every capability named in the architecture specifications. References to files, masks, grants, or
        revocation flows in those specifications describe implementation work or plans unless a release page
        explicitly marks them as available and they have been tested in that release.
      </p>

      <h2 className="docs-h2">The planned permission design</h2>
      <p className="docs-p">
        The design goal is simple: a networked action should require a narrow permission; the permission should
        identify what it allows; and withdrawing it should stop future use. Different actions should not be
        bundled into one vague agreement.
      </p>
      <ul className="docs-ul">
        <li>Local play should not silently become data sharing.</li>
        <li>Joining another player should be separate from sharing diagnostic data.</li>
        <li>Connecting an outside language model should be separate from either of those actions.</li>
        <li>A record should show which permission was used, without exposing private content.</li>
      </ul>

      <h2 className="docs-h2">How to read stronger technical claims</h2>
      <p className="docs-p">
        Words such as <em>default-deny</em>, <em>air-gapped</em>, and <em>revocable</em> describe testable
        properties. They should be treated as verified only when the named release has matching tests and
        observable behavior. If the evidence is missing, the claim is a goal—not a guarantee.
      </p>

      <h2 className="docs-h2">Related pages</h2>
      <ul className="docs-ul">
        <li><a href="/words" style={{ color: '#7dd3fc' }}>Definitions for technical words and symbols</a></li>
        <li><a href="/docs/substrate" style={{ color: '#7dd3fc' }}>Technical foundations and their current status</a></li>
        <li><a href="/docs/mycelium" style={{ color: '#7dd3fc' }}>The planned Mycelium design</a></li>
        <li><a href="/legal/privacy" style={{ color: '#7dd3fc' }}>Privacy policy</a></li>
      </ul>

      <PrevNextNav slug="sovereignty" />
    </DocsLayout>
  );
};

export default Page;
