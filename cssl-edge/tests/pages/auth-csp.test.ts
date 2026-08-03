import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

type HeaderRule = {
  source?: string;
  headers?: Array<{ key?: string; value?: string }>;
};

const config = JSON.parse(
  readFileSync(resolve(process.cwd(), 'vercel.json'), 'utf8'),
) as { headers?: HeaderRule[] };

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

const globalRule = config.headers?.find((rule) => rule.source === '/(.*)');
const csp = globalRule?.headers?.find((header) => header.key === 'Content-Security-Policy')?.value ?? '';
assert(
  csp.includes('connect-src') && csp.includes('https://pzirbmyfmrbtkllrtcmx.supabase.co'),
  'browser auth must be allowed to reach the configured Supabase HTTPS origin',
);
assert(
  csp.includes('wss://pzirbmyfmrbtkllrtcmx.supabase.co'),
  'Clearing realtime must be allowed to reach the configured Supabase websocket origin',
);
assert(!csp.includes('https://*.supabase.co'), 'CSP must not grant every Supabase project');
assert(!csp.includes('wss://*.supabase.co'), 'realtime CSP must stay project-scoped');

const privatePageRule = config.headers?.find((rule) => rule.source?.includes('login|register|account|chat|apocrypha'));
const cacheControl = privatePageRule?.headers?.find((header) => header.key === 'Cache-Control')?.value ?? '';
assert(cacheControl.includes('no-store'), 'auth and chat documents must not be stored');
assert(cacheControl.includes('no-transform'), 'auth and chat documents must not be modified in transit');

console.log('auth-csp.test : OK · Supabase auth/realtime project allowed; private surfaces remain no-store/no-transform');
