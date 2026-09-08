import { createContributorRouteHandler } from '@/lib/apocrypha/contributor-http';

export const config = {
  api: { bodyParser: false, responseLimit: '256kb' },
  maxDuration: 30,
};

export default createContributorRouteHandler('enroll');

