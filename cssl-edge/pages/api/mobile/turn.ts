import { createAccountHandler } from '@/lib/mobile/account-api';
export const config = { api: { bodyParser: { sizeLimit: '100kb' } }, maxDuration: 120 };
export default createAccountHandler('turn');
