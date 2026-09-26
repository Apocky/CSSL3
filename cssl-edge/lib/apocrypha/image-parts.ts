// Images attached to a turn, as OpenAI-dialect content parts for the flagship (the AI Gateway
// passes image_url parts through to Claude). The local model cannot see; it gets the name only.

export type ContentPart = { type: 'text'; text: string } | { type: 'image_url'; image_url: { url: string } };

export function attachedImageUrls(request: Record<string, unknown>): string[] {
  const list = Array.isArray(request.attachments) ? request.attachments as Array<Record<string, unknown>> : [];
  return list.map((a) => a.image_url).filter((u): u is string => typeof u === 'string' && /^https:\/\//u.test(u)).slice(0, 8);
}

/** The final user turn with the images appended, or the text unchanged when there are none. */
export function withImages(text: string, urls: readonly string[]): string | ContentPart[] {
  if (urls.length === 0) return text;
  return [{ type: 'text', text }, ...urls.map((url) => ({ type: 'image_url' as const, image_url: { url } }))];
}
