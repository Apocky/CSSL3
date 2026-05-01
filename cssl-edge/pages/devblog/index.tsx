// apocky.com/devblog · index of devblog posts
// SSG · zero-dep markdown rendering at SSG-time.

import type { NextPage, GetStaticProps } from 'next';
import Head from 'next/head';
import { DEVBLOG_POSTS, type DevblogPost } from '@/lib/devblog-posts';

interface DevblogIndexProps {
  posts: ReadonlyArray<Pick<DevblogPost, 'slug' | 'title' | 'date_iso' | 'tags' | 'author' | 'blurb'>>;
}

const DevblogIndex: NextPage<DevblogIndexProps> = ({ posts }) => {
  return (
    <>
      <Head>
        <title>Devblog · Apocky</title>
        <meta name="description" content="Thoughts on substrate-native systems, sovereignty in language design, and the mycelial-network multiplayer thesis." />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href="https://apocky.com/devblog" />
        <style>{`
          * { box-sizing: border-box; }
          html, body { margin: 0; padding: 0; }
          body {
            background: radial-gradient(ellipse at top, #15151f 0%, #0a0a0f 50%, #050507 100%);
            color: #e6e6f0;
            font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
            min-height: 100vh;
            -webkit-font-smoothing: antialiased;
          }
          a { color: inherit; text-decoration: none; }
          a:hover { opacity: 0.85; }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 880,
          margin: '0 auto',
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← apocky.com
        </a>

        <h1
          style={{
            fontSize: 'clamp(1.75rem, 4vw, 2.5rem)',
            margin: 0,
            fontWeight: 700,
            letterSpacing: '-0.02em',
            backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)',
            WebkitBackgroundClip: 'text',
            WebkitTextFillColor: 'transparent',
          }}
        >
          Devblog
        </h1>
        <p style={{ color: '#a8a8b8', marginTop: '0.5rem', fontSize: '0.95rem' }}>
          § Notes on substrate-native systems · sovereignty · mycelial multiplayer
        </p>

        <section style={{ marginTop: '2.5rem', display: 'grid', gap: '0.75rem' }}>
          {posts.map((p) => (
            <a
              key={p.slug}
              href={`/devblog/${p.slug}`}
              style={{
                display: 'block',
                padding: '1.25rem 1.4rem',
                background: 'rgba(20, 20, 30, 0.5)',
                border: '1px solid #1f1f2a',
                borderRadius: 6,
              }}
            >
              <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.1em' }}>
                {p.date_iso} · {p.author} · {p.tags.join(' · ')}
              </div>
              <div style={{ fontSize: '1.05rem', color: '#e6e6f0', marginTop: '0.35rem', fontWeight: 600 }}>
                {p.title}
              </div>
              <div style={{ fontSize: '0.88rem', color: '#a0a0b0', marginTop: '0.45rem' }}>{p.blurb}</div>
            </a>
          ))}
        </section>

        <footer
          style={{
            marginTop: '4rem',
            paddingTop: '2.5rem',
            borderTop: '1px solid #1f1f2a',
            color: '#5a5a6a',
            fontSize: '0.78rem',
          }}
        >
          <p style={{ margin: 0 }}>§ ¬ harm in the making · sovereignty preserved · t∞</p>
        </footer>
      </main>
    </>
  );
};

export const getStaticProps: GetStaticProps<DevblogIndexProps> = async () => {
  return {
    props: {
      posts: DEVBLOG_POSTS.map((p) => ({
        slug: p.slug,
        title: p.title,
        date_iso: p.date_iso,
        tags: p.tags,
        author: p.author,
        blurb: p.blurb,
      })),
    },
  };
};

export default DevblogIndex;
