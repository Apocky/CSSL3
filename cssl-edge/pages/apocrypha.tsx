import type { GetServerSideProps, NextApiRequest, NextPage } from 'next';
import Head from 'next/head';

import BrainExperience from '@/components/brain/BrainExperience';
import { requireBrainOwner } from '@/lib/brain/owner';

interface ApocryphaPageProps {
  readonly serverAccess: 'owner' | 'forbidden' | 'unavailable';
}

export const getServerSideProps: GetServerSideProps<ApocryphaPageProps> = async ({ req, res }) => {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
  const decision = await requireBrainOwner(req as NextApiRequest);
  if (!decision.ok && decision.code === 'BRAIN_SESSION_REQUIRED') {
    return { redirect: { destination: '/login?next=%2Fapocrypha', permanent: false } };
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

const ApocryphaPage: NextPage<ApocryphaPageProps> = ({ serverAccess }) => (
  <>
    <Head>
      <title>Apocrypha · Apocky</title>
      <meta name="description" content="Persistent owner-private Apocrypha conversation, contextual recall, and source-linked memory exploration." />
      <meta name="robots" content="noindex,nofollow,noarchive,nosnippet" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#03040c" />
    </Head>
    <BrainExperience serverAccess={serverAccess} />
  </>
);

export default ApocryphaPage;
