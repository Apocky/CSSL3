import Link from 'next/link';
import { useRouter } from 'next/router';

// Global site chrome for apocky.com — the hub. Real things only: Apocrypha (the DI), CSSL, CSL,
// Chaos Tarot, and Apocky's channels. Wired in _app.tsx for content pages; auth, admin, and the
// immersive chat page render bare.

type NavItem = { href: string; label: string; ext?: boolean };

const NAV: NavItem[] = [
  { href: '/chat', label: 'Apocrypha' },
  { href: 'https://cssl.dev', label: 'CSSL', ext: true },
  { href: 'https://cssl.dev/CSLv3', label: 'CSL', ext: true },
  { href: 'https://chaos-tarot.com', label: 'Chaos Tarot', ext: true },
];

const SOCIAL: NavItem[] = [
  { href: 'https://medium.com/@noneisone.oneisall', label: 'Medium', ext: true },
  { href: 'https://ko-fi.com/oneinfinity', label: 'Ko-fi', ext: true },
  { href: 'https://www.patreon.com/0ne1nfinity', label: 'Patreon', ext: true },
  { href: 'https://github.com/Apocky', label: 'GitHub', ext: true },
];

const LEGAL: NavItem[] = [
  { href: '/legal/privacy', label: 'Privacy' },
  { href: '/legal/terms', label: 'Terms' },
  { href: '/legal/eula', label: 'EULA' },
];

function extProps(item: NavItem) {
  return item.ext ? { target: '_blank' as const, rel: 'noopener noreferrer' } : {};
}

export default function SiteShell({ children }: { children: React.ReactNode }): JSX.Element {
  const { pathname } = useRouter();
  const active = (h: string) => pathname === h;

  return (
    <div style={S.shell}>
      <nav style={S.nav}>
        <Link href="/" style={S.brand}>APOCKY</Link>
        <div style={S.links}>
          {NAV.map((n) => (
            <Link key={n.label} href={n.href} {...extProps(n)} style={{ ...S.link, ...(active(n.href) ? S.linkActive : {}) }}>
              {n.label}{n.ext ? <span style={S.arr}> ↗</span> : null}
            </Link>
          ))}
        </div>
        <Link href="/login" style={S.signin}>Sign in</Link>
      </nav>

      <div style={S.body}>{children}</div>

      <footer style={S.footer}>
        <div style={S.footRow}>
          <span style={S.footBrand}>APOCKY</span>
          {SOCIAL.map((s) => (
            <Link key={s.label} href={s.href} {...extProps(s)} style={S.footLink}>{s.label}</Link>
          ))}
          <span style={S.sep}>·</span>
          {LEGAL.map((l) => (
            <Link key={l.label} href={l.href} style={S.footLink}>{l.label}</Link>
          ))}
          <a href="mailto:apocky13@gmail.com" style={S.footLink}>Contact</a>
        </div>
        <div style={S.footNote}>© {new Date().getFullYear()} Apocky</div>
      </footer>
    </div>
  );
}

const S: Record<string, React.CSSProperties> = {
  shell: { minHeight: '100vh', display: 'flex', flexDirection: 'column', background: '#06080a', color: '#e6f0ef', fontFamily: 'ui-sans-serif, system-ui, "Segoe UI", sans-serif' },
  nav: { position: 'sticky', top: 0, zIndex: 50, display: 'flex', alignItems: 'center', gap: 8, padding: '0 20px', height: 54, borderBottom: '1px solid #18212a', background: 'rgba(6,8,10,0.72)', backdropFilter: 'blur(8px)' },
  brand: { fontSize: 15, fontWeight: 700, letterSpacing: '0.24em', color: '#e6f0ef', textDecoration: 'none' },
  links: { display: 'flex', gap: 20, marginLeft: 28, flexWrap: 'wrap', flex: 1 },
  link: { color: '#8fb3b0', textDecoration: 'none', fontSize: 13.5 },
  linkActive: { color: '#5fe6d6' },
  arr: { color: '#4a5658', fontSize: 11 },
  signin: { color: '#8fb3b0', textDecoration: 'none', fontSize: 13, whiteSpace: 'nowrap' },
  body: { flex: 1, minWidth: 0 },
  footer: { borderTop: '1px solid #18212a', padding: '22px 20px 30px', display: 'flex', flexDirection: 'column', gap: 10 },
  footRow: { display: 'flex', gap: 16, flexWrap: 'wrap', alignItems: 'center', fontSize: 12.5 },
  footBrand: { color: '#9fb3b0', fontWeight: 700, letterSpacing: '0.22em' },
  footLink: { color: '#6b7d80', textDecoration: 'none' },
  sep: { color: '#2a3338' },
  footNote: { color: '#4a5658', fontSize: 11 },
};
