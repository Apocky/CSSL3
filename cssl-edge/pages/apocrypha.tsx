import Head from 'next/head';

import { PublicChat } from '@/components/apocrypha/PublicChat';

export default function ApocryphaPage(): JSX.Element {
  return (
    <>
      <Head>
        <title>Talk with Apocrypha · Apocky</title>
        <meta
          name="description"
          content="A clear, conversation-centered portal for communicating with Apocrypha."
        />
        <meta property="og:title" content="Talk with Apocrypha · Apocky" />
        <meta
          property="og:description"
          content="A clear, conversation-centered portal for communicating with Apocrypha."
        />
        <link rel="canonical" href="https://www.apocky.com/apocrypha" />
      </Head>
      <PublicChat />
    </>
  );
}
