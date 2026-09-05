// cssl-edge · tests/pages/run-share-feed.test.tsx
// The experimental run-sharing page must remain absent while its endpoint
// contains demonstration records rather than retained, attributable shares.

import RunShareFeedPage, {
  getServerSideProps,
  _testPageExportsAndFraming,
} from '@/pages/run-share-feed';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export async function testPageStaysAbsentWithoutRealData(): Promise<void> {
  assert(
    typeof RunShareFeedPage === 'function',
    `default export must be a function, got ${typeof RunShareFeedPage}`
  );
  assert(_testPageExportsAndFraming(), '_testPageExportsAndFraming must return true');

  const result = await getServerSideProps({} as never);
  assert(
    'notFound' in result && result.notFound === true,
    'route must return 404 until it can serve real records',
  );
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  testPageStaysAbsentWithoutRealData()
    .then(() => {
      // eslint-disable-next-line no-console
      console.log('run-share-feed.test : OK · 1 test passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
