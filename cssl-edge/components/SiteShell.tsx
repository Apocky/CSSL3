import Link from 'next/link';
import { useRouter } from 'next/router';

type NavItem = { href: string; label: string; ext?: boolean };

const NAV: ReadonlyArray<NavItem> = [
  { href: '/#projects', label: 'Projects' },
  { href: '/#apocrypha', label: 'Apocrypha' },
  { href: '/#elsewhere', label: 'Elsewhere' },
  { href: '/#support', label: 'Support' },
  { href: '/words', label: 'Words & symbols' },
];

const WORK: ReadonlyArray<NavItem> = [
  { href: 'https://cssl.dev', label: 'Visit CSSL', ext: true },
  { href: 'https://cssl.dev/CSLv3', label: 'Read about CSLv3', ext: true },
  { href: 'https://chaos-tarot.com', label: 'Visit Chaos Tarot', ext: true },
  { href: '/download', label: 'Download Labyrinth of Apocalypse' },
];

const ABOUT: ReadonlyArray<NavItem> = [
  { href: 'https://medium.com/@noneisone.oneisall', label: 'Writing on Medium', ext: true },
  { href: 'https://github.com/Apocky', label: 'Code on GitHub', ext: true },
  { href: 'https://ko-fi.com/oneinfinity', label: 'Support on Ko-fi', ext: true },
  { href: 'https://www.patreon.com/0ne1nfinity', label: 'Support on Patreon', ext: true },
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

export default function SiteShell({ children }: { children: React.ReactNode }): JSX.Element {
  const { pathname } = useRouter();

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
              Shawn Apocky&apos;s home for projects, writing, social links, and
              ways to support the work.
            </p>
          </div>
          <div>
            <h2 className="apx-footer-title">Projects</h2>
            <div className="apx-footer-links">
              {WORK.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
            </div>
          </div>
          <div>
            <h2 className="apx-footer-title">Elsewhere & support</h2>
            <div className="apx-footer-links">
              {ABOUT.map((item) => <Link key={item.label} href={item.href} {...extProps(item)} className="apx-footer-link">{item.label}</Link>)}
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
