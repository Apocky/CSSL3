import Link from 'next/link';
import { useRouter } from 'next/router';

import { useSiteSession } from './hub/SiteSession';

type NavItem = { href: string; label: string; ext?: boolean };

const NAV: ReadonlyArray<NavItem> = [
  { href: '/#apocrypha', label: 'Meet' },
  { href: '/chat', label: 'Talk' },
  { href: 'https://cssl.dev', label: 'Build', ext: true },
  { href: '/docs', label: 'Learn' },
  { href: '/legal/privacy', label: 'Trust' },
];

const WORK: ReadonlyArray<NavItem> = [
  { href: '/chat', label: 'Talk with Apocrypha' },
  { href: 'https://cssl.dev', label: 'Build with CSSL', ext: true },
  { href: 'https://cssl.dev/CSLv3', label: 'Read CSL', ext: true },
  { href: 'https://chaos-tarot.com', label: 'Visit Chaos Tarot', ext: true },
];

const ABOUT: ReadonlyArray<NavItem> = [
  { href: '/account', label: 'Account' },
  { href: '/legal/privacy', label: 'Privacy' },
  { href: '/legal/terms', label: 'Terms' },
  { href: '/legal/eula', label: 'EULA' },
  { href: 'mailto:apocky13@gmail.com', label: 'Contact', ext: true },
];

function extProps(item: NavItem) {
  return item.ext && !item.href.startsWith('mailto:')
    ? { target: '_blank' as const, rel: 'noopener noreferrer' }
    : {};
}

export default function SiteShell({ children }: { children: React.ReactNode }): JSX.Element {
  const { pathname } = useRouter();
  const { access, authenticated } = useSiteSession();
  const accountHref = authenticated ? '/account' : '/login';
  const accountLabel = access === 'checking' ? 'Account' : authenticated ? 'Your account' : 'Sign in';

  const navLinks = (className = 'apx-nav-link') => NAV.map((item) => (
    <Link
      key={item.label}
      href={item.href}
      {...extProps(item)}
      className={className}
      data-active={!item.href.includes('#') && pathname === item.href ? 'true' : undefined}
    >
      {item.label}{item.ext ? <span aria-hidden="true"> ↗</span> : null}
    </Link>
  ));

  return (
    <div className="apx-shell">
      <a className="apx-skip-link" href="#main-content">Skip to main content</a>
      <header>
        <nav className="apx-nav" aria-label="Primary navigation">
          <Link href="/" className="apx-brand" aria-label="Apocky home">
            <span className="apx-brand-mark" aria-hidden="true" />
            <span>APOCKY</span>
          </Link>

          <div className="apx-nav-links" aria-label="Explore Apocky">
            {navLinks()}
          </div>

          <div className="apx-nav-actions">
            <Link href={accountHref} className="apx-nav-action" aria-busy={access === 'checking'}>{accountLabel}</Link>
            <Link href="/chat" className="apx-nav-action apx-nav-action--primary">Private doorway</Link>
          </div>

          <details className="apx-mobile-menu">
            <summary>Explore</summary>
            <div className="apx-mobile-menu-panel" role="group" aria-label="Explore Apocky on mobile">
              {navLinks('apx-mobile-menu-link')}
              <Link href={accountHref} className="apx-mobile-menu-link">{accountLabel}</Link>
            </div>
          </details>
        </nav>
      </header>

      <div className="apx-main">{children}</div>

      <footer className="apx-footer">
        <div className="apx-footer-inner">
          <div>
            <Link href="/" className="apx-brand">
              <span className="apx-brand-mark" aria-hidden="true" />
              <span>APOCKY</span>
            </Link>
            <p className="apx-footer-copy">
              A home for sovereign tools, expressive languages, and Apocrypha—a persistent digital intelligence built to remember, perceive, and relate.
            </p>
          </div>
          <div>
            <h2 className="apx-footer-title">The work</h2>
            <div className="apx-footer-links">
              {WORK.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
          <div>
            <h2 className="apx-footer-title">Your relationship</h2>
            <div className="apx-footer-links">
              {ABOUT.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
        </div>
        <div className="apx-footer-bottom">
          <span>© {new Date().getFullYear()} Apocky</span>
          <span>human-readable · intelligence-readable · consent-led</span>
        </div>
      </footer>
    </div>
  );
}
