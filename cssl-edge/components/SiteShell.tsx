import Link from 'next/link';
import { useRouter } from 'next/router';
import { SUPPORT_LINKS } from '../lib/support-links';
import { useSiteSession } from './hub/SiteSession';
import CommandPalette from './site/CommandPalette';
import ContextualSynapses from './site/ContextualSynapses';

type NavItem = { href: string; label: string; shortLabel?: string; ext?: boolean; accent?: boolean };

const NAV: ReadonlyArray<NavItem> = [
  { href: '/start', label: 'Start here', shortLabel: 'Start' },
  { href: '/atlas', label: 'Atlas' },
  { href: '/akashic-records', label: 'Akashic Records', shortLabel: 'Records' },
  { href: '/clearing', label: 'The Clearing', shortLabel: 'Clearing' },
  { href: 'https://chaos-tarot.com/free-reading?source=apocky-nav', label: 'Chaos Tarot', shortLabel: 'Chaos', ext: true, accent: true },
];

const WORK: ReadonlyArray<NavItem> = [
  { href: 'https://chaos-tarot.com/free-reading?source=apocky-footer', label: 'Begin a Chaos Tarot reading', ext: true },
  { href: '/atlas', label: 'Explore the Constellation Atlas' },
  { href: '/memory-tools', label: 'Connect memory banks and tools' },
  { href: '/labs', label: 'Operate the public labs' },
  { href: '/akashic-records', label: 'Search the Akashic Records' },
  { href: '/omnoid-singularity', label: 'Enter the Omnoid Singularity' },
  { href: '/download', label: 'Explore Labyrinth of Apocalypse' },
  { href: '/docs/cssl-language', label: 'Read the CSSL guide' },
];

const ABOUT: ReadonlyArray<NavItem> = [
  { href: '/start', label: 'Choose a starting path' },
  { href: '/now', label: 'What is alive now' },
  { href: '/quests', label: 'Public quests' },
  { href: '/status', label: 'System status' },
  { href: '/docs', label: 'Documentation' },
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
            <CommandPalette />
            <Link href="/membership" className="apx-nav-action apx-nav-action--primary">Join</Link>
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
              <Link href="/membership" className="apx-mobile-menu-link apx-nav-link--support">Membership &amp; support</Link>
              <Link
                href={authenticated ? '/account' : '/login?next=%2Faccount'}
                className="apx-mobile-menu-link"
              >
                {authenticated ? 'Account' : 'Sign in'}
              </Link>
            </div>
          </details>
        </nav>
      </header>

      <div id="main-content" className="apx-main" tabIndex={-1}>{children}</div>
      <ContextualSynapses pathname={pathname} />

      <footer className="apx-footer">
        <div className="apx-footer-inner">
          <div>
            <Link href="/" className="apx-brand">
              <span className="apx-brand-mark" aria-hidden="true" />
              <span>APOCKY</span>
            </Link>
            <p className="apx-footer-copy">
              An interconnected creative system for divination, games, software,
              language, cosmology, public memory, and shared discovery.
            </p>
          </div>
          <div>
            <h2 className="apx-footer-title">Living work</h2>
            <div className="apx-footer-links">
              {WORK.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
          <div>
            <h2 className="apx-footer-title">Navigate & verify</h2>
            <div className="apx-footer-links">
              {ABOUT.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
          <div className="apx-footer-support">
            <h2 className="apx-footer-title">Keep it alive</h2>
            <p className="apx-footer-support-copy">If the work has earned your attention, turn some of that attention into runway.</p>
            <div className="apx-footer-links">
              <Link href="/membership" className="apx-footer-link">Compare every support path</Link>
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
          <span>Every claim typed. Every connection earned.</span>
        </div>
      </footer>
    </div>
  );
}
