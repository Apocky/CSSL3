// The experimental gear-sharing page must remain absent until a verified,
// attributable listing service and withdrawal contract are connected.

import GearSharePage, {
  getServerSideProps,
  _testPageExportsAndFraming,
} from '@/pages/gear-share';

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

async function main(): Promise<void> {
  assert(typeof GearSharePage === 'function', 'default export must remain renderable');
  assert(_testPageExportsAndFraming(), 'page framing self-check must pass');

  const result = await getServerSideProps({} as never);
  assert(
    'notFound' in result && result.notFound === true,
    'route must return 404 until it can serve verified attributable records',
  );

  // eslint-disable-next-line no-console
  console.log('gear-share.test : OK · gated route remains absent');
}

void main().catch((error: unknown) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exitCode = 1;
});
