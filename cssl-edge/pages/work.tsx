import Head from 'next/head';
import dynamic from 'next/dynamic';

import { useSiteSession } from '@/components/hub/SiteSession';

// The console opens an event stream and reads local service state on mount; there is nothing
// meaningful to render on the server, and pre-rendering it would flash an offline state.
const WorkConsole = dynamic(() => import('@/components/work/WorkConsole'), { ssr: false });

export default function WorkPage(): JSX.Element {
  const session = useSiteSession();
  const owner = session.access === 'owner';

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
      ? <main id="main-content" role="status" style={{ padding: 24 }}><p>Checking your access…</p></main>
      : owner
        ? <main id="main-content" aria-label="Apocrypha Work"><WorkConsole /></main>
        : <main id="main-content" style={{ padding: 24, maxWidth: 560 }}>
          <h1>Work</h1>
          <p>
            This lane drives a coding agent with access to the files on the machine that hosts it,
            so it is limited to the owner account. Nothing here is shared with Chat.
          </p>
        </main>}
  </>;
}
