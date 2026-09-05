import type { NextPage } from 'next';
import Head from 'next/head';

import ConstellationAtlas from '../components/atlas/ConstellationAtlas';
import { PUBLIC_SURFACE_NODES } from '../lib/public-surface-graph';

const AtlasPage: NextPage = () => {
  const structuredData = {
    '@context': 'https://schema.org',
    '@type': 'CollectionPage',
    name: 'Constellation Atlas',
    description: 'An interactive visual map, multidimensional index, and plain-language dictionary for public Apocky projects and spaces.',
    url: 'https://www.apocky.com/atlas',
    hasPart: PUBLIC_SURFACE_NODES.map((node) => ({
      '@type': node.external ? 'WebSite' : 'WebPage',
      name: node.title,
      url: node.external ? node.href : `https://www.apocky.com${node.href}`,
      description: node.summary,
    })),
  };

  return (
    <>
      <Head>
        <title>Find a tool, story, or idea · Apocky</title>
        <meta
          name="description"
          content="Search the tools, stories, writing, and useful ideas on Apocky. Find a page and get straight to it."
        />
        <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
        <meta property="og:title" content="Constellation Atlas — Explore Apocky" />
        <meta property="og:description" content="Trace the public projects, archives, shared spaces, and external creative work connected through Apocky." />
        <meta property="og:type" content="website" />
        <meta property="og:url" content="https://www.apocky.com/atlas" />
        <meta property="og:site_name" content="Apocky" />
        <link rel="canonical" href="https://www.apocky.com/atlas" />
        <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData) }} />
      </Head>
      <ConstellationAtlas />
    </>
  );
};

export default AtlasPage;
