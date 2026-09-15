// apocky.com/docs/apocrypha
//
// Until now the docs section documented a withdrawn game and said nothing about the one thing a
// visitor can actually use. This page describes only observed behaviour of the live surface; where
// something is a design rather than a running feature, it says so.

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => (
  <DocsLayout
    activeSlug="apocrypha"
    title="Talking to Apocrypha · Apocky Documentation"
    description="How to ask Apocrypha a question in the browser, what it keeps, and what signing in changes."
  >
    <h1 className="docs-h1">Talking to Apocrypha</h1>
    <p className="docs-blurb">
      Apocrypha answers questions in your browser. No account is required to ask one.
    </p>

    <h2 className="docs-h2">Asking a question</h2>
    <p className="docs-p">
      Open <a href="/apocrypha" style={{ color: '#7dd3fc' }}>apocky.com/apocrypha</a> and type. Answers are
      generated on a machine Apocky runs, not a hosted API, so a reply takes longer than a commercial
      chat product — usually tens of seconds rather than an instant. The page shows that it is working
      while it waits.
    </p>

    <h2 className="docs-h2">What happens to what you type</h2>
    <p className="docs-p">
      Signed out, the visible thread lives in your own browser and will not follow you to another
      device. The question itself does reach the server: it is stored as a work item so the machine
      can pick it up and answer, and the reply is stored with it. Those records are deleted
      automatically after 30 days. No account is attached to them — a signed-out visitor is
      identified only by a random value in a cookie, and even that reaches the database only as a
      one-way hash.
    </p>
    <p className="docs-p">
      Nothing you type is used to train a model. Each answer carries a receipt stating that, which the
      page checks before showing you anything — if a reply arrives without it, the reply is discarded
      rather than displayed.
    </p>

    <Callout kind="note" title="Signed out has limits, on purpose">
      One machine answers every question, so signed-out visitors share a short hourly budget and can
      have one question in flight at a time. If you hit it, the page says so and tells you when to try
      again. Signing in raises the limit and keeps your history.
    </Callout>

    <h2 className="docs-h2">What signing in changes</h2>
    <p className="docs-p">
      An account moves your conversation off the browser and onto the server, so it is the same
      conversation on your phone, your desktop, and the apps. It also raises the per-hour limit. It
      does not change who answers or how.
    </p>

    <h2 className="docs-h2">Memory</h2>
    <p className="docs-p">
      Apocrypha reads from a record of earlier work when a question touches it, and it is expected to
      name the record it relied on. If it has nothing relevant, the correct answer is that it does not
      know — not a guess. Signed-out conversations are answered from a separate partition that cannot
      reach Apocky&rsquo;s own records.
    </p>

    <h2 className="docs-h2">The apps</h2>
    <p className="docs-p">
      The same conversation is available on Windows, iPhone, and Android. See{' '}
      <a href="/download/apocrypha" style={{ color: '#7dd3fc' }}>the app downloads</a>. The apps talk to
      the same machine as the website; they are not a separate assistant.
    </p>

    <Callout kind="warn" title="This is one machine, not a service">
      Apocrypha runs on hardware in one room. If that machine is off or busy, the website will tell you
      it could not take your message. That is the honest state of it rather than a queue that silently
      never returns.
    </Callout>

    <PrevNextNav slug="apocrypha" />
  </DocsLayout>
);

export default Page;
