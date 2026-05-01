// apocky.com/devblog/[slug] · render single post · SSG · markdown→HTML at build.

import type { NextPage, GetStaticPaths, GetStaticProps } from 'next';
import Head from 'next/head';
import { DEVBLOG_POSTS, findPost, type DevblogPost } from '@/lib/devblog-posts';
import { markdownToHtml } from '@/lib/markdown';

interface PostPageProps {
  post: DevblogPost;
  html: string;
  prevSlug: string | null;
  nextSlug: string | null;
}

const PostPage: NextPage<PostPageProps> = ({ post, html, prevSlug, nextSlug }) => {
  return (
    <>
      <Head>
        <title>{post.title} · Apocky devblog</title>
        <meta name="description" content={post.blurb} />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#0a0a0f" />
        <link rel="canonical" href={`https://apocky.com/devblog/${post.slug}`} />
        <meta property="og:title" content={post.title} />
        <meta property="og:description" content={post.blurb} />
        <meta property="og:type" content="article" />
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
          .post .md-h1 { font-size: 1.6rem; margin: 2rem 0 0.6rem; color: #e6e6f0; }
          .post .md-h2 { font-size: 1.2rem; margin: 1.6rem 0 0.5rem; color: #c084fc; }
          .post .md-h3 { font-size: 1rem; margin: 1.2rem 0 0.4rem; color: #7dd3fc; }
          .post .md-p  { margin: 0.6rem 0; color: #cdd6e4; font-size: 0.95rem; line-height: 1.7; }
          .post .md-ul, .post .md-ol { margin: 0.6rem 0; padding-left: 1.4rem; color: #cdd6e4; font-size: 0.95rem; line-height: 1.7; }
          .post .md-code { background: rgba(124, 211, 252, 0.08); padding: 0.1rem 0.3rem; border-radius: 3px; color: #7dd3fc; font-size: 0.88em; }
          .post .md-code-block { background: rgba(15, 15, 25, 0.6); border: 1px solid #1f1f2a; border-radius: 6px; padding: 1rem 1.2rem; font-size: 0.82rem; line-height: 1.55; overflow-x: auto; }
          .post a { color: #7dd3fc; text-decoration: underline; }
          .post strong { color: #e6e6f0; }
          .post em { color: #fbbf24; font-style: normal; }
        `}</style>
      </Head>
      <main
        style={{
          maxWidth: 760,
          margin: '0 auto',
          padding: '4rem 1.5rem 6rem',
          lineHeight: 1.65,
        }}
      >
        <a href="/devblog" style={{ fontSize: '0.85rem', color: '#7a7a8c', display: 'inline-block', marginBottom: '2rem' }}>
          ← /devblog
        </a>

        <div style={{ fontSize: '0.7rem', color: '#7a7a8c', letterSpacing: '0.1em', textTransform: 'uppercase' }}>
          {post.date_iso} · {post.author} · {post.tags.join(' · ')}
        </div>
        <h1
          style={{
            fontSize: 'clamp(1.5rem, 3.5vw, 2.2rem)',
            margin: '0.4rem 0 0',
            fontWeight: 700,
            color: '#e6e6f0',
            letterSpacing: '-0.01em',
          }}
        >
          {post.title}
        </h1>

        <article className="post" style={{ marginTop: '2rem' }} dangerouslySetInnerHTML={{ __html: html }} />

        <nav
          style={{
            marginTop: '2.5rem',
            display: 'flex',
            justifyContent: 'space-between',
            gap: '1rem',
            flexWrap: 'wrap',
          }}
        >
          {prevSlug !== null ? (
            <a href={`/devblog/${prevSlug}`} style={{ fontSize: '0.85rem', color: '#7dd3fc' }}>← {prevSlug}</a>
          ) : <span />}
          {nextSlug !== null ? (
            <a href={`/devblog/${nextSlug}`} style={{ fontSize: '0.85rem', color: '#7dd3fc' }}>{nextSlug} →</a>
          ) : <span />}
        </nav>

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

export const getStaticPaths: GetStaticPaths = () => {
  return {
    paths: DEVBLOG_POSTS.map((p) => ({ params: { slug: p.slug } })),
    fallback: false,
  };
};

export const getStaticProps: GetStaticProps<PostPageProps> = async (ctx) => {
  const slug = typeof ctx.params?.['slug'] === 'string' ? (ctx.params['slug'] as string) : '';
  const post = findPost(slug);
  if (post === null) return { notFound: true };
  const idx = DEVBLOG_POSTS.findIndex((p) => p.slug === slug);
  const prev = idx > 0 ? DEVBLOG_POSTS[idx - 1] : null;
  const next = idx < DEVBLOG_POSTS.length - 1 ? DEVBLOG_POSTS[idx + 1] : null;
  return {
    props: {
      post,
      html: markdownToHtml(post.body),
      prevSlug: prev !== null ? (prev as DevblogPost).slug : null,
      nextSlug: next !== null ? (next as DevblogPost).slug : null,
    },
  };
};

export default PostPage;
