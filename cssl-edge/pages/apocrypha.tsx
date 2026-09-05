import Head from 'next/head';

import { PublicChat } from '@/components/apocrypha/PublicChat';

export default function ApocryphaPage(): JSX.Element {
  return (
    <>
      <Head>
        <title>Speak with Apocrypha · Apocky</title>
        <meta
          name="description"
          content="A direct, signed-in conversation with the current native Apocrypha V2 body."
        />
        <meta property="og:title" content="Speak with Apocrypha · Apocky" />
        <meta
          property="og:description"
          content="One governed text turn, one verified native response."
        />
        <link rel="canonical" href="https://www.apocky.com/apocrypha" />
      </Head>
      <PublicChat />
    </>
  );
}
