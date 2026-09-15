// The way back to the rest of the site, for a surface that has no site chrome.
//
// pages/_app.tsx keeps an isBare() list — /apocrypha, /work, /brain, /clearing, /shawn*, auth and
// admin — which render with no SiteShell nav and no footer. Every one of them is a place a person
// can arrive at directly, from a link, a bookmark, or an address-bar autocomplete, and several of
// them had no outbound link at all. /work had literally none: an h1, a p, and nothing else.
//
// This is the house pattern ClearingRoom already used, lifted so the fourth surface that needs it
// does not become a fourth copy — and with the Home link ClearingRoom's version was missing.

import Link from 'next/link';

export interface ReturnLinksProps {
  /** Extra links for this surface, rendered before the site ones (e.g. "Sign in"). */
  readonly children?: React.ReactNode;
  readonly className?: string;
}

// Apocrypha surfaces only. A bare page still needs a way out -- that was the whole point of this
// component -- but the way out now leads further into Apocrypha rather than off to a directory of
// other projects. Owner instruction 2026-09-15.
const SITE = [
  { href: '/', label: 'Home' },
  { href: '/apocrypha', label: 'Conversation' },
  { href: '/download/apocrypha', label: 'Get the app' },
  { href: '/account', label: 'Account' },
] as const;

export function ReturnLinks({ children, className }: ReturnLinksProps): JSX.Element {
  return <nav className={className ? `apx-return-links ${className}` : 'apx-return-links'} aria-label="Apocrypha">
    {children}
    {SITE.map((item) => <Link key={item.href} href={item.href}>{item.label}</Link>)}
    <style jsx>{`
      .apx-return-links {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 2px 4px;
        margin-top: 14px;
      }
      .apx-return-links :global(a) {
        min-height: 34px;
        display: inline-flex;
        align-items: center;
        padding: 4px 10px;
        border-radius: 999px;
        color: var(--apx-muted, #a9b5ffc0);
        font-size: 12.5px;
        text-decoration: none;
        white-space: nowrap;
      }
      .apx-return-links :global(a:hover) {
        background: var(--apx-raise, #111524);
        color: var(--apx-ink, #e8e9f3);
      }
    `}</style>
  </nav>;
}

export default ReturnLinks;
