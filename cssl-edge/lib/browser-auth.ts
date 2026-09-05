import { browserAuthMutationAllowsProtectedOpen, getAuthClient } from './auth';

export async function getBrowserAuthHeaders(): Promise<Headers> {
  const headers = new Headers();
  if (!browserAuthMutationAllowsProtectedOpen()) return headers;
  const client = getAuthClient();
  if (!client) return headers;

  try {
    const { data } = await client.auth.getSession();
    if (data.session?.access_token) {
      headers.set('Authorization', `Bearer ${data.session.access_token}`);
    }
  } catch {
    // The server check still has the cookie path if this lookup is temporarily unavailable.
  }
  return headers;
}

export async function authFetch(input: RequestInfo | URL, init: RequestInit = {}): Promise<Response> {
  const signal = init.signal;
  const authHeaders = signal
    ? await Promise.race([
      getBrowserAuthHeaders(),
      new Promise<never>((_resolve, reject) => {
        if (signal.aborted) {
          reject(signal.reason ?? new DOMException('Aborted', 'AbortError'));
          return;
        }
        signal.addEventListener('abort', () => reject(signal.reason ?? new DOMException('Aborted', 'AbortError')), { once: true });
      }),
    ])
    : await getBrowserAuthHeaders();
  const headers = new Headers(init.headers);
  authHeaders.forEach((value, key) => headers.set(key, value));
  return fetch(input, { ...init, headers });
}
