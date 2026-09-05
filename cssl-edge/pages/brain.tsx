import type { GetServerSideProps, NextApiRequest, NextPage } from 'next';
import Head from 'next/head';

import BrainExperience from '@/components/brain/BrainExperience';
import { requireBrainOwner } from '@/lib/brain/owner';

interface BrainPageProps {
  readonly serverAccess: 'owner' | 'forbidden' | 'unavailable';
}

export const getServerSideProps: GetServerSideProps<BrainPageProps> = async ({ req, res }) => {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
  const decision = await requireBrainOwner(req as NextApiRequest);
  if (!decision.ok && decision.code === 'BRAIN_SESSION_REQUIRED') {
    return { redirect: { destination: '/login?next=%2Fbrain', permanent: false } };
  }
  return {
    props: {
      serverAccess: decision.ok
        ? 'owner'
        : decision.code === 'BRAIN_OWNER_REQUIRED'
          ? 'forbidden'
          : 'unavailable',
    },
  };
};

const BrainPage: NextPage<BrainPageProps> = ({ serverAccess }) => (
  <>
    <Head>
      <title>Private Brain · Apocky</title>
      <meta name="description" content="Owner-private conversation, contextual recall, and source-linked memory exploration." />
      <meta name="robots" content="noindex,nofollow,noarchive,nosnippet" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#03040c" />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="apple-mobile-web-app-capable" content="yes" />
      <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent" />
    </Head>
    <BrainExperience serverAccess={serverAccess} />
  </>
);

export default BrainPage;
