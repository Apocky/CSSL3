import Head from 'next/head';
import type { GetServerSideProps, NextApiRequest } from 'next';
import AccountChat from '@/components/apocrypha/AccountChat';
import BrainExperience from '@/components/brain/BrainExperience';
import { requireBrainOwner } from '@/lib/brain/owner';
import { usesOwnerRuntime } from '@/lib/mobile/owner-runtime';

interface ApocryphaPageProps { readonly ownerConversation: boolean }

export const getServerSideProps: GetServerSideProps<ApocryphaPageProps> = async ({ req, res }) => {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Vary', 'Cookie, Authorization');
  const owner = await requireBrainOwner(req as NextApiRequest);
  return { props: { ownerConversation: owner.ok && usesOwnerRuntime(owner.user) } };
};

export default function ApocryphaPage({ ownerConversation }: ApocryphaPageProps): JSX.Element {
  return <>
    <Head>
      <title>Apocrypha · Apocky</title>
      <meta name="description" content="Chat with Apocrypha from your browser. Sign in to your Apocky account to keep your own conversations together." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="robots" content="noindex,nofollow,noarchive,nosnippet" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    {ownerConversation ? <BrainExperience serverAccess="owner" /> : <AccountChat />}
  </>;
}
