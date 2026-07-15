import type { GetServerSideProps, InferGetServerSidePropsType, NextPage } from 'next';
import Head from 'next/head';
import Link from 'next/link';
import { ReferencePage } from '@/components/shawn';
import { referenceBySlug } from '@/lib/shawn/catalog';
import { atlasData } from '@/lib/shawn/atlas';
import type { ReferenceRecord } from '@/lib/shawn/types';
import styles from '@/components/shawn/Atlas.module.css';

interface ReferenceRouteProps {
  readonly reference: ReferenceRecord;
}

const ReferenceRoute: NextPage<InferGetServerSidePropsType<typeof getServerSideProps>> = ({ reference }) => (
  <>
    <Head>
      <title>{`${reference.title} — Shawn / Apocky Atlas`}</title>
      <meta name="description" content={reference.orientation} />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="referrer" content="no-referrer" />
      <meta property="og:title" content={`${reference.title} — Shawn / Apocky Atlas`} />
      <meta property="og:description" content={reference.orientation} />
      <meta property="og:type" content="article" />
      <meta property="og:url" content={`https://apocky.com/shawn/reference/${encodeURIComponent(reference.slug)}`} />
      <link rel="canonical" href={`https://apocky.com/shawn/reference/${encodeURIComponent(reference.slug)}`} />
    </Head>
    <main className={styles.referencePageShell}>
      <nav className={styles.referencePageNav} aria-label="Atlas return">
        <Link href="/shawn#references">← Return to the atlas</Link>
        <span>Canonical explanatory reference</span>
      </nav>
      <ReferencePage reference={reference} referenceBySlug={referenceBySlug} />
    </main>
  </>
);

export const getServerSideProps: GetServerSideProps<ReferenceRouteProps> = async (context) => {
  const { evaluateAtlasPublicationGate } = await import('@/lib/shawn/publication-gate');
  const gate = evaluateAtlasPublicationGate(process.env, {
    version: atlasData.version,
    status: atlasData.status,
  });
  if (!gate.allowed) return { notFound: true };
  const slug = typeof context.params?.['slug'] === 'string' ? context.params['slug'] : null;
  if (!slug) return { notFound: true };
  const reference = referenceBySlug(slug);
  if (!reference) return { notFound: true };
  return { props: { reference } };
};

export default ReferenceRoute;
