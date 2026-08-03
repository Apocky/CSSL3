import Head from 'next/head';

import { PublicChat } from '@/components/apocrypha/PublicChat';

export default function ApocryphaPage(): JSX.Element {
  return (
    <>
      <Head>
        <title>Apocrypha Workspace · Apocky</title>
        <meta
          name="description"
          content="Create in conversation, inspect durable artifacts and background work, and govern every consequential change."
        />
        <meta property="og:title" content="Apocrypha Workspace · Apocky" />
        <meta
          property="og:description"
          content="A conversation-first workspace for content, evidence-bearing artifacts, and governed agentic work."
        />
        <link rel="canonical" href="https://www.apocky.com/apocrypha" />
      </Head>
      <PublicChat />
    </>
  );
}
