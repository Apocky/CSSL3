import Head from 'next/head';
import { useRouter } from 'next/router';

import { ApocryphaThreshold } from '../components/apocrypha/Threshold';
import { usePublicPresence } from '../components/apocrypha/usePublicPresence';
import { useSiteSession } from '../components/hub/SiteSession';

const DEFAULT_ROOM = 'north-clearing';
const ROOM_SLUG = /^[a-z0-9][a-z0-9-]{1,47}$/;

export default function ApocryphaThresholdPage(): JSX.Element {
  const router = useRouter();
  const session = useSiteSession();
  const presence = usePublicPresence();
  const requested = typeof router.query.room === 'string' ? router.query.room.toLowerCase() : DEFAULT_ROOM;
  const room = ROOM_SLUG.test(requested) ? requested : DEFAULT_ROOM;
  const returnPath = router.isReady ? router.asPath : '/apocrypha';

  const retry = (): void => {
    void Promise.all([session.refresh(), presence.refresh()]);
  };

  return (
    <>
      <Head>
        <title>Apocrypha · private threshold</title>
        <meta
          name="description"
          content="A fail-closed doorway to Apocrypha and The Clearing."
        />
        <meta property="og:title" content="Apocrypha · private threshold" />
        <link rel="canonical" href="https://www.apocky.com/apocrypha" />
      </Head>
      <ApocryphaThreshold
        access={session.access}
        presence={presence.state}
        roomHref={`/apocrypha/rooms/${encodeURIComponent(room)}`}
        signInHref={`/login?next=${encodeURIComponent(returnPath)}`}
        onRetry={retry}
      />
    </>
  );
}
