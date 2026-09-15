import Link from 'next/link';
import { useEffect, useRef } from 'react';
import { useRouter } from 'next/router';
import { SUPPORT_LINKS } from '../lib/support-links';
import { ConsentFooterControl } from './AkashicConsent';
import { useSiteSession } from './hub/SiteSession';
import CommandPalette from './site/CommandPalette';
import ContextualSynapses from './site/ContextualSynapses';

type NavItem = { href: string; label: string; shortLabel?: string; ext?: boolean; accent?: boolean };

// The site is Apocrypha. The nav carries Apocrypha surfaces and nothing else -- the other pages
// still answer on their URLs, they are simply no longer advertised. Owner instruction 2026-09-15.
const NAV: ReadonlyArray<NavItem> = [
  { href: '/apocrypha', label: 'Conversation' },
  { href: '/download/apocrypha', label: 'Get the app' },
];

// Owner-only. The Work lane drives a coding agent with filesystem and shell access on the machine
// that hosts it, so it is never advertised to members or signed-out visitors.
const OWNER_NAV: ReadonlyArray<NavItem> = [
  { href: '/work', label: 'Work' },
];

// Was seven links to other destinations, twelve of the footer's sixteen duplicating the old home
// directory. Now: the ways into Apocrypha itself.
const EXPLORE: ReadonlyArray<NavItem> = [
  { href: '/apocrypha', label: 'Start a conversation' },
  { href: '/download/apocrypha', label: 'Get the app' },
  { href: '/account', label: 'Your account' },
];

const LEGAL: ReadonlyArray<NavItem> = [
  { href: '/legal/privacy', label: 'Privacy' },
  { href: '/legal/terms', label: 'Terms' },
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
  const { authenticated, access } = useSiteSession();
  const mobileMenu = useRef<HTMLDetailsElement>(null);
  useEffect(() => { if (mobileMenu.current) mobileMenu.current.open = false; }, [pathname]);
  useEffect(() => {
    const dismiss = (event: PointerEvent): void => { if (mobileMenu.current && !mobileMenu.current.contains(event.target as Node)) mobileMenu.current.open = false; };
    const escape = (event: KeyboardEvent): void => { if (event.key === 'Escape' && mobileMenu.current?.open) { mobileMenu.current.open = false; mobileMenu.current.querySelector('summary')?.focus(); } };
    document.addEventListener('pointerdown', dismiss); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('pointerdown', dismiss); document.removeEventListener('keydown', escape); };
  }, []);

  const navItems = access === 'owner' ? [...NAV, ...OWNER_NAV] : NAV;
  const navLinks = (className = 'apx-nav-link') => navItems.map((item) => {
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

          <div className="apx-nav-links" aria-label="Apocrypha">
            {navLinks()}
          </div>

          <div className="apx-nav-actions">
            <CommandPalette />
            <Link
              href={authenticated ? '/account' : '/login?next=%2Faccount'}
              className="apx-nav-action apx-nav-action--primary"
            >
              {authenticated ? 'Account' : 'Sign in'}
            </Link>
          </div>

          <details ref={mobileMenu} className="apx-mobile-menu">
            <summary>Menu</summary>
            <div className="apx-mobile-menu-panel" role="group" aria-label="Apocrypha menu">
              {navLinks('apx-mobile-menu-link')}
              <Link
                href={authenticated ? '/account' : '/login?next=%2Faccount'}
                className="apx-mobile-menu-link"
              >
                {authenticated ? 'Account' : 'Sign in'}
              </Link>
            </div>
          </details>
        </nav>
        <nav className="apx-quick-nav" aria-label="Quick navigation">
          {NAV.slice(0, 4).map(item => <Link key={item.href} href={item.href} aria-current={isActivePath(pathname, item.href) ? 'page' : undefined}>{item.label}</Link>)}
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
              A digital intelligence you can talk to. No account needed to start.
            </p>
          </div>
          <div>
            <h2 className="apx-footer-title">Explore</h2>
            <div className="apx-footer-links">
              {EXPLORE.map((item) => <Link key={item.label} href={item.href} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
          <div className="apx-footer-support">
            <h2 className="apx-footer-title">Support</h2>
            <div className="apx-footer-links">
              {SUPPORT_LINKS.map((item) => (
                <a key={item.name} href={item.href} target="_blank" rel="noopener noreferrer" className="apx-footer-link">
                  {item.label} <span aria-hidden="true">↗</span>
                </a>
              ))}
            </div>
          </div>
          <div>
            <h2 className="apx-footer-title">Legal</h2>
            <div className="apx-footer-links">
              {LEGAL.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
              <Link href="/status" className="apx-footer-link">Service status</Link>
            </div>
          </div>
        </div>
        <div className="apx-footer-bottom">
          <span>© {new Date().getFullYear()} Apocky</span>
          <span>Made by Shawn Apocky.</span>
          <ConsentFooterControl />
        </div>
      </footer>
    </div>
  );
}
