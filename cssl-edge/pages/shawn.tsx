import type { GetServerSideProps, NextPage } from 'next';
import Head from 'next/head';
import { ShawnAtlas } from '@/components/shawn';
import { atlasData } from '@/lib/shawn/atlas';

const ShawnPage: NextPage = () => (
  <>
    <Head>
      <title>Shawn / Apocky — Interactive Cognitive and Epistemic Atlas</title>
      <meta
        name="description"
        content="A chronological, evidence-typed atlas of Shawn Apocky's reasoning, experiments, artifacts, contradictions, and evolving models."
      />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="referrer" content="no-referrer" />
      <meta property="og:title" content="Shawn / Apocky — Interactive Cognitive and Epistemic Atlas" />
      <meta property="og:description" content="A question becomes a field. The field is rotated. What survives becomes a model." />
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
