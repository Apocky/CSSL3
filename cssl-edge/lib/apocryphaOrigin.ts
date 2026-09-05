export function apocryphaOriginHeaders(): Record<string, string> {
  const headers: Record<string, string> = {};
  const clientId = process.env.APOCRYPHA_CF_ACCESS_CLIENT_ID;
  const clientSecret = process.env.APOCRYPHA_CF_ACCESS_CLIENT_SECRET;
  if (clientId && clientSecret) {
    headers['CF-Access-Client-Id'] = clientId;
    headers['CF-Access-Client-Secret'] = clientSecret;
  }
  const edgeToken = process.env.APOCRYPHA_EDGE_TOKEN;
  if (edgeToken) headers.authorization = `Bearer ${edgeToken}`;
  return headers;
}
