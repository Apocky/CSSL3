import Link from 'next/link';
import { useRouter } from 'next/router';
import { SUPPORT_LINKS } from '../lib/support-links';
import { useSiteSession } from './hub/SiteSession';

type NavItem = { href: string; label: string; shortLabel?: string; ext?: boolean; accent?: boolean };

const NAV: ReadonlyArray<NavItem> = [
  { href: '/#projects', label: 'Creative work', shortLabel: 'Work' },
  { href: '/akashic-records', label: 'Akashic Records', shortLabel: 'Akashic' },
  { href: '/apocrypha', label: 'Talk with Apocrypha', shortLabel: 'Apocrypha' },
  { href: '/clearing', label: 'The Clearing', shortLabel: 'Clearing' },
  { href: '/atlas', label: 'Atlas' },
  { href: '/buy', label: 'Support the work', shortLabel: 'Support', accent: true },
];

const WORK: ReadonlyArray<NavItem> = [
  { href: 'https://chaos-tarot.com', label: 'Enter Chaos Tarot', ext: true },
  { href: '/download', label: 'Explore Labyrinth of Apocalypse' },
  { href: 'https://cssl.dev', label: 'Visit CSSL', ext: true },
  { href: 'https://cssl.dev/CSLv3', label: 'Read about CSLv3', ext: true },
  { href: '/akashic-records', label: 'Read the Akashic Records' },
  { href: '/apocrypha', label: 'Talk with Apocrypha' },
  { href: '/clearing', label: 'Enter the Clearing' },
  { href: '/atlas', label: 'Explore the Atlas' },
];

const ABOUT: ReadonlyArray<NavItem> = [
  { href: 'https://github.com/Apocky', label: 'Code on GitHub', ext: true },
  { href: '/words', label: 'Words & symbols' },
  { href: '/legal/privacy', label: 'Privacy' },
  { href: '/legal/terms', label: 'Terms' },
  { href: '/legal/eula', label: 'Game license' },
  { href: 'mailto:apocky13@gmail.com', label: 'Contact', ext: true },
];

function extProps(item: NavItem) {
  return item.ext && !item.href.startsWith('mailto:')
    ? { target: '_blank' as const, rel: 'noopener noreferrer' }
    : {};
}

function isActivePath(pathname: string, href: string): boolean {
  if (!href.startsWith('/') || href.includes('#')) return false;
  return pathname === href || (href !== '/' && pathname.startsWith(`${href}/`));
}

export default function SiteShell({ children }: { children: React.ReactNode }): JSX.Element {
  const { pathname } = useRouter();
  const { authenticated } = useSiteSession();

  const navLinks = (className = 'apx-nav-link') => NAV.map((item) => {
    const visibleLabel = className === 'apx-mobile-menu-link' ? item.label : (item.shortLabel ?? item.label);
    return (
      <Link
        key={item.label}
        href={item.href}
        {...extProps(item)}
        className={`${className}${item.accent ? ' apx-nav-link--support' : ''}`}
        data-active={isActivePath(pathname, item.href) ? 'true' : undefined}
        aria-current={isActivePath(pathname, item.href) ? 'page' : undefined}
        aria-label={visibleLabel === item.label ? undefined : item.label}
      >
        {visibleLabel}{item.ext ? <span aria-hidden="true"> ↗</span> : null}
      </Link>
    );
  });

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
            <Link
              href={authenticated ? '/account' : '/login?next=%2Faccount'}
              className="apx-nav-action"
            >
              {authenticated ? 'Account' : 'Sign in'}
            </Link>
          </div>

          <details className="apx-mobile-menu">
            <summary>Explore</summary>
            <div className="apx-mobile-menu-panel" role="group" aria-label="Explore Apocky on mobile">
              {navLinks('apx-mobile-menu-link')}
            </div>
          </details>
        </nav>
      </header>

      <div id="main-content" className="apx-main" tabIndex={-1}>{children}</div>

      <footer className="apx-footer">
        <div className="apx-footer-inner">
          <div>
            <Link href="/" className="apx-brand">
              <span className="apx-brand-mark" aria-hidden="true" />
              <span>APOCKY</span>
            </Link>
            <p className="apx-footer-copy">
              Shawn Apocky’s creative home for games, software, language,
              symbolic art, writing, and interconnected works in progress.
            </p>
          </div>
          <div>
            <h2 className="apx-footer-title">Creative work & portals</h2>
            <div className="apx-footer-links">
              {WORK.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
          <div>
            <h2 className="apx-footer-title">Elsewhere & legal</h2>
            <div className="apx-footer-links">
              {ABOUT.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
          <div className="apx-footer-support">
            <h2 className="apx-footer-title">Optional support</h2>
            <p className="apx-footer-support-copy">Help sustain the work if you would like to. No obligation.</p>
            <div className="apx-footer-links">
              {SUPPORT_LINKS.map((item) => (
                <a key={item.name} href={item.href} target="_blank" rel="noopener noreferrer" className="apx-footer-link">
                  {item.label} <span aria-hidden="true">↗</span>
                </a>
              ))}
            </div>
          </div>
        </div>
        <div className="apx-footer-bottom">
          <span>© {new Date().getFullYear()} Apocky</span>
          <span>Plain language first. Technical detail when it helps.</span>
        </div>
      </footer>
    </div>
  );
}
