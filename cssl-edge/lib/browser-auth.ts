import { getAuthClient, persistSessionToCookie } from './auth';

export async function getBrowserAuthHeaders(): Promise<Headers> {
  const headers = new Headers();
  const client = getAuthClient();
  if (!client) return headers;

  try {
    const { data } = await client.auth.getSession();
    if (data.session?.access_token) {
      persistSessionToCookie(data.session.access_token, data.session.refresh_token ?? undefined);
      headers.set('Authorization', `Bearer ${data.session.access_token}`);
    }
  } catch {
    // The server check still has the cookie path if this lookup is temporarily unavailable.
  }
  return headers;
}

export async function authFetch(input: RequestInfo | URL, init: RequestInit = {}): Promise<Response> {
  const authHeaders = await getBrowserAuthHeaders();
  const headers = new Headers(init.headers);
  authHeaders.forEach((value, key) => headers.set(key, value));
  return fetch(input, { ...init, headers });
}