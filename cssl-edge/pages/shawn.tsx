import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { ShawnAtlas } from '@/components/shawn';
import { atlasData } from '@/lib/shawn/atlas';

const ShawnPage: NextPage = () => (
  <>
    <Head>
      <title>Shawn / Apocky — Interactive evidence atlas</title>
      <meta
        name="description"
        content="A correctable, source-linked overview of Shawn Apocky’s work, reasoning, experiments, and changing interpretations."
      />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="referrer" content="no-referrer" />
      <meta property="og:title" content="Shawn / Apocky — Interactive evidence atlas" />
      <meta property="og:description" content="A source-linked overview that separates observations, reports, inferences, and proposals." />
      <meta property="og:type" content="website" />
      <meta property="og:url" content="https://apocky.com/shawn" />
      <link rel="canonical" href="https://apocky.com/shawn" />
    </Head>
    <ShawnAtlas />
  </>
);

export const getServerSideProps: GetServerSideProps<Record<string, never>> = async () => {
  const { evaluateAtlasPublicationGate } = await import('@/lib/shawn/publication-gate');
  const gate = evaluateAtlasPublicationGate(process.env, {
    version: atlasData.version,
    status: atlasData.status,
  });
  if (!gate.allowed) return { notFound: true };
  return { props: {} };
};

export default ShawnPage;
