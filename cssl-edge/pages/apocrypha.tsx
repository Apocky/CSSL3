import Head from 'next/head';
import type { GetServerSideProps } from 'next';
import AccountChat from '@/components/apocrypha/AccountChat';

export const getServerSideProps: GetServerSideProps = async ({ res }) => {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  return { props: {} };
};

export default function ApocryphaPage(): JSX.Element {
  return <>
    <Head>
      <title>Apocrypha · Apocky</title>
      <meta name="description" content="Chat with Apocrypha from your browser. Sign in to your Apocky account to keep your own conversations together." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="robots" content="noindex,nofollow,noarchive,nosnippet" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    <AccountChat />
  </>;
}
