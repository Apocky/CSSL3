// apocky.com/docs/mycelium

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="mycelium"
      title="Mycelium and Home · Apocky Documentation"
      description="A plain-language introduction to the planned Mycelium network and personal Home design."
    >
      <h1 className="docs-h1">Mycelium and Home</h1>
      <p className="docs-blurb">
        A planned way for people to keep a personal space and choose how, or whether, it connects to others.
      </p>

      <Callout kind="coming-soon" title="This is a design, not a public feature">
        The public LoA test build does not currently provide the complete Home and Mycelium experience described
        here. Source modules and architecture notes exist, but that is not the same as a finished, tested
        multiplayer service.
      </Callout>

      <h2 className="docs-h2">What the names mean</h2>
      <p className="docs-p">
        <strong>Home</strong> is the name for a person’s own game space. <strong>Mycelium</strong> is the name
        for a proposed network through which separate Homes could connect. The biological metaphor describes
        the shape of the design; it does not imply that the software is alive.
      </p>

      <h2 className="docs-h2">The intended experience</h2>
      <p className="docs-p">
        A Home would begin as a private place on the player’s computer. A person could keep it private, invite
        particular people, or make selected parts easier to discover. No one should be expected to publish,
        participate, or contribute data.
      </p>
      <ul className="docs-ul">
        <li>Private play remains a complete and respected choice.</li>
        <li>An invitation applies only to the people and activity it names.</li>
        <li>Sharing game information is separate from sharing diagnostic information.</li>
        <li>Leaving a connection should not require another person’s permission.</li>
      </ul>

      <h2 className="docs-h2">Possible Home styles</h2>
      <p className="docs-p">
        Design notes explore several visual starting points: an orbital platform, a small ship, a cathedral,
        an observatory, a forest, and mixtures of those ideas. These are creative directions, not promises that
        every style is available in the current build.
      </p>

      <h2 className="docs-h2">What shared learning would mean</h2>
      <p className="docs-p">
        Some plans discuss <strong>federated learning</strong>: a method that combines limited mathematical
        updates from separate computers instead of collecting everyone’s full source data in one place. This
        still creates privacy and security risks. It must remain off until a person chooses it, and the
        implementation must be independently tested before any privacy claim is treated as verified.
      </p>

      <h2 className="docs-h2">What must be decided before release</h2>
      <ul className="docs-ul">
        <li>Exactly what information can leave a Home.</li>
        <li>Who can receive it and for how long.</li>
        <li>How invitations, blocking, withdrawal, and deletion work.</li>
        <li>How people can inspect and change every relevant setting.</li>
        <li>Which claims have been confirmed by running tests rather than design documents.</li>
      </ul>

      <h2 className="docs-h2">Related pages</h2>
      <ul className="docs-ul">
        <li><a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>Permissions and data sharing</a></li>
        <li><a href="/docs/substrate" style={{ color: '#7dd3fc' }}>Technical foundations</a></li>
        <li><a href="/words" style={{ color: '#7dd3fc' }}>Definitions for technical words and symbols</a></li>
      </ul>

      <PrevNextNav slug="mycelium" />
    </DocsLayout>
  );
};

export default Page;
