import Head from 'next/head';
import Link from 'next/link';
import dynamic from 'next/dynamic';

import { useSiteSession } from '@/components/hub/SiteSession';
import ReturnLinks from '@/components/nav/ReturnLinks';

// The console opens an event stream and reads local service state on mount; there is nothing
// meaningful to render on the server, and pre-rendering it would flash an offline state.
const WorkConsole = dynamic(() => import('@/components/work/WorkConsole'), { ssr: false });

export default function WorkPage(): JSX.Element {
  const session = useSiteSession();

  // /work is in _app's isBare() list, so there is no SiteShell nav and no footer. Every branch
  // except the console therefore has to carry its own way out: this page used to render an <h1>, a
  // <p>, and not one anchor — arrive here from a bookmark or an autocomplete and the only exit was
  // the back button, which does not exist in a fresh tab.
  //
  // The links go INSIDE each non-owner branch rather than above the ternary: WorkConsole.module.css
  // gives the console height:100dvh, so a nav above it would push the composer off the screen.
  const gate = (heading: string, body: React.ReactNode, action?: React.ReactNode) => (
    <main id="main-content" style={{ padding: 24, maxWidth: 560 }}>
      <h1>{heading}</h1>
      <p>{body}</p>
      {action}
      <ReturnLinks />
    </main>
  );

  return <>
    <Head>
      <title>Work · Apocrypha</title>
      <meta name="description" content="Local coding agent. Runs on your own machine, reads and edits your own files." />
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
      <meta name="robots" content="noindex,nofollow,noarchive,nosnippet" />
      <meta name="referrer" content="no-referrer" />
      <meta name="theme-color" content="#05060b" />
    </Head>
    {session.access === 'checking'
      // This branch IS the server-rendered response, so the links here are what a crawler, a
      // reader with JavaScript off, and anyone on a slow hydration actually receive.
      ? <main id="main-content" style={{ padding: 24, maxWidth: 560 }}>
        <p role="status">Checking your access…</p>
        <ReturnLinks />
      </main>
      : session.access === 'owner'
        ? <main id="main-content" aria-label="Apocrypha Work"><WorkConsole /></main>
        : session.access === 'signed-out'
          ? gate(
            'Work',
            <>This lane drives a coding agent with access to the files on the machine that hosts it,
              so it is limited to the owner account. Nothing here is shared with Chat.</>,
            <p><Link href="/login?next=%2Fwork">Sign in</Link></p>,
          )
          : session.access === 'unavailable'
            // Not "limited to the owner account" — that asserts a permission verdict the site never
            // actually received. An owner hitting an auth outage was being told they are not the
            // owner.
            ? gate(
              'Work',
              <>We could not check your access just now. This is a problem reaching the account
                service, not an answer about your account. Try again in a moment.</>,
            )
            : gate(
              'Work',
              <>This lane drives a coding agent with access to the files on the machine that hosts it,
                so it is limited to the owner account. Nothing here is shared with Chat.</>,
            )}
  </>;
}
