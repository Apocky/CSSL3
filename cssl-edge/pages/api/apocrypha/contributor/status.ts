import { createContributorRouteHandler } from '@/lib/apocrypha/contributor-http';

export const config = {
  api: { bodyParser: false, responseLimit: '64kb' },
  maxDuration: 10,
};

export default createContributorRouteHandler('status');

