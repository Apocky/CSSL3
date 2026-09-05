import { createObservationHandler } from '@/lib/brain/observations';

export const config = { api: { responseLimit: '512kb' }, maxDuration: 30 };
export default createObservationHandler();
