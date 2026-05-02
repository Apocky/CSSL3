// cssl-edge · components/ContentDetail.tsx
// W12-6 · UGC-Discover-Browse · full per-package detail view
//
// Surfaces title · author (revocable-pubkey-tag · ⊘ glyph) · description ·
// screenshots · install-button · ratings + distribution · remixes ·
// attribution chain · cosmetic-axiom-attestation pill.
//
// Sovereignty : cap-revocability glyph clickable → /docs/sovereign-cap ·
// cosmetic-axiom-attestation prominent · ¬ engagement-tracking · ¬ scroll-depth.

import type { ContentDetail as ContentDetailType, AttributionLink } from '@/lib/content-fetch';
import { STATUS_PILL, displayAuthor, timeAgo } from '@/lib/content-fetch';

interface ContentDetailProps {
  detail: ContentDetailType;
  stubMode?: boolean;
}

const H2 = ({ children }: { children: React.ReactNode }) => (
  <h2 style={{ fontSize: '0.75rem', textTransform: 'uppercase', letterSpacing: '0.15em', color: '#7a7a8c', margin: '0 0 0.75rem' }}>
    {children}
  </h2>
);

const Pill = ({ bg, fg, title, href, children }: { bg: string; fg: string; title?: string; href?: string; children: React.ReactNode }) => {
  const style: React.CSSProperties = { padding: '0.15rem 0.6rem', background: bg, color: fg, fontSize: '0.7rem', letterSpacing: '0.08em', borderRadius: 999, textTransform: 'uppercase', fontWeight: 600, textDecoration: 'none' };
  return href ? <a href={href} title={title} style={style}>{children}</a> : <span title={title} style={style}>{children}</span>;
};

const ContentDetail = ({ detail, stubMode = false }: ContentDetailProps) => {
  const pill = STATUS_PILL[detail.status];
  const totalRatings = detail.rating_summary.total_ratings;
  const meanScore = detail.rating_summary.mean_score;
  const distribution = detail.rating_summary.distribution;
  const maxBucket = Math.max(1, ...distribution);

  return (
    <article style={{ paddingBottom: '4rem' }}>
      {stubMode && (
        <div role="status" style={{ padding: '0.75rem 1rem', background: 'rgba(251,191,36,0.06)', border: '1px solid rgba(251,191,36,0.25)', borderRadius: 6, fontSize: '0.82rem', color: '#fbbf24', marginBottom: '1.5rem', lineHeight: 1.5 }}>
          <strong>◐ stub-mode</strong> · publish-detail-API (sibling W12-5) not yet wired · placeholder rendered to demonstrate layout
        </div>
      )}

      <header style={{ marginBottom: '2rem' }}>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '0.5rem', marginBottom: '0.75rem' }}>
          <Pill bg={pill.bg} fg={pill.color}>{pill.glyph} {pill.label}</Pill>
          {detail.cosmetic_axiom_attested && (
            <Pill bg="rgba(52,211,153,0.1)" fg="#34d399" title="creator has attested cosmetic-axiom-compliance · no pay-for-power">
              ✓ cosmetic-axiom · attested
            </Pill>
          )}
          {detail.cap_revocable && (
            <Pill bg="rgba(192,132,252,0.1)" fg="#c084fc" title="Σ-mask : creator capability is unilaterally revocable" href="/docs/sovereign-cap">
              ⊘ revocable · sovereign-cap
            </Pill>
          )}
        </div>
        <h1 style={{ fontSize: 'clamp(1.5rem, 4vw, 2.2rem)', margin: '0 0 0.5rem', fontWeight: 700, letterSpacing: '-0.02em', backgroundImage: 'linear-gradient(135deg, #ffffff 0%, #c084fc 60%, #7dd3fc 100%)', WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent', backgroundClip: 'text' }}>
          {detail.title}
        </h1>
        <p style={{ color: '#a0a0b0', fontSize: '0.9rem', margin: 0, display: 'flex', flexWrap: 'wrap', gap: '0.5rem 1rem' }}>
          <span>by {displayAuthor(detail)}</span>
          <span>· published {timeAgo(detail.published_at)} ago</span>
          {detail.tags.length > 0 && <span>· {detail.tags.slice(0, 6).join(' · ')}</span>}
        </p>
      </header>

      {detail.install_url && (
        <div style={{ marginBottom: '2rem' }}>
          <a href={detail.install_url} style={{ display: 'inline-block', padding: '0.75rem 1.5rem', background: 'linear-gradient(135deg, #c084fc 0%, #7dd3fc 100%)', color: '#0a0a0f', fontWeight: 600, borderRadius: 4, fontSize: '0.95rem', textDecoration: 'none' }}>
            ↓ install / launch →
          </a>
          <span style={{ marginLeft: '1rem', fontSize: '0.78rem', color: '#7a7a8c' }}>
            ¬ DRM · ¬ rootkit · sovereign-uninstall always-available
          </span>
        </div>
      )}

      {detail.screenshots.length > 0 && (
        <section style={{ marginBottom: '2.5rem' }}>
          <H2>§ Screenshots</H2>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: '0.6rem' }}>
            {detail.screenshots.map((src, idx) => (
              <img key={src} src={src} alt={`screenshot ${idx + 1} of ${detail.title}`} loading="lazy" style={{ width: '100%', height: 'auto', borderRadius: 4, border: '1px solid #1f1f2a', background: '#14141d' }} />
            ))}
          </div>
        </section>
      )}

      <section style={{ marginBottom: '2.5rem' }}>
        <H2>§ Description</H2>
        <p style={{ color: '#cdd6e4', fontSize: '0.95rem', lineHeight: 1.7, whiteSpace: 'pre-wrap', margin: 0 }}>
          {detail.description}
        </p>
      </section>

      <section style={{ marginBottom: '2.5rem' }}>
        <H2>§ Ratings · {totalRatings} total</H2>
        {totalRatings === 0 ? (
          <p style={{ color: '#5a5a6a', fontSize: '0.85rem', margin: 0 }}>○ no ratings yet · ¬ aggregate available</p>
        ) : (
          <div style={{ padding: '1rem', background: 'rgba(20, 20, 30, 0.5)', border: '1px solid #1f1f2a', borderRadius: 6 }}>
            <div style={{ fontSize: '1.5rem', fontWeight: 600, color: '#fbbf24', marginBottom: '0.6rem' }}>
              {meanScore.toFixed(1)} ★
            </div>
            <div role="img" aria-label={`distribution histogram · ${distribution.join(' · ')}`}>
              {[5, 4, 3, 2, 1].map((star) => {
                const count = distribution[star - 1] ?? 0;
                const width = maxBucket > 0 ? (count / maxBucket) * 100 : 0;
                return (
                  <div key={star} style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', fontSize: '0.78rem', marginBottom: '0.25rem' }}>
                    <span style={{ color: '#7a7a8c', minWidth: '1.25rem' }}>{star}★</span>
                    <div style={{ flexGrow: 1, height: 8, background: '#14141d', borderRadius: 2, overflow: 'hidden' }}>
                      <div style={{ width: `${width}%`, height: '100%', background: 'linear-gradient(90deg, #c084fc 0%, #7dd3fc 100%)' }} />
                    </div>
                    <span style={{ color: '#5a5a6a', minWidth: '2rem', textAlign: 'right' }}>{count}</span>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </section>

      {detail.attribution_chain.length > 1 && (
        <section style={{ marginBottom: '2.5rem' }}>
          <H2>§ Attribution Chain · {detail.attribution_chain.length} generations</H2>
          <ol style={{ listStyle: 'none', padding: 0, margin: 0, borderLeft: '2px solid #2a2a3a', paddingLeft: '1rem' }}>
            {detail.attribution_chain.map((link: AttributionLink, idx) => (
              <li key={link.slug} style={{ marginBottom: idx === detail.attribution_chain.length - 1 ? 0 : '0.75rem', fontSize: '0.85rem', color: '#cdd6e4' }}>
                <span style={{ color: '#a78bfa', marginRight: '0.5rem' }}>gen-{link.generation}</span>
                <a href={`/content/${encodeURIComponent(link.slug)}`} style={{ color: '#7dd3fc', textDecoration: 'none' }}>{link.title}</a>
                <span style={{ color: '#5a5a6a', marginLeft: '0.5rem', fontSize: '0.78rem' }}>by {displayAuthor({ author_pubkey: link.author_pubkey })}</span>
              </li>
            ))}
          </ol>
        </section>
      )}

      {detail.remix_slugs.length > 0 && (
        <section style={{ marginBottom: '2.5rem' }}>
          <H2>§ Remixes · {detail.remix_slugs.length} downstream</H2>
          <ul style={{ listStyle: 'none', padding: 0, margin: 0, display: 'flex', flexWrap: 'wrap', gap: '0.4rem' }}>
            {detail.remix_slugs.map((slug) => (
              <li key={slug}>
                <a href={`/content/${encodeURIComponent(slug)}`} style={{ display: 'inline-block', padding: '0.3rem 0.75rem', background: 'rgba(125,211,252,0.06)', border: '1px solid rgba(125,211,252,0.2)', borderRadius: 4, fontSize: '0.78rem', color: '#7dd3fc', textDecoration: 'none' }}>
                  ⊔ {slug}
                </a>
              </li>
            ))}
          </ul>
        </section>
      )}
    </article>
  );
};

export default ContentDetail;
