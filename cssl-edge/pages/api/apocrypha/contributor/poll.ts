import { createContributorRouteHandler } from '@/lib/apocrypha/contributor-http';

export const config = {
  api: { bodyParser: false, responseLimit: '256kb' },
  maxDuration: 30,
};

/** Public node-authenticated lease polling; no controller bearer is accepted
 * or required on this route.  The body signature is checked against the
 * enrolled node key before the controller signs and stores a dispatch. */
export default createContributorRouteHandler('poll');

