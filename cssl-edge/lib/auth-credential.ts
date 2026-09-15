// What a person can paste into the sign-in box.
//
// Whether the email shows a six-digit code at all is decided by the mail template, not by this
// code. When the template only carries a link, asking for a code asks for something the person
// does not have -- and inside the app, tapping that link opens the system browser, so the session
// is established THERE and the app stays signed out.
//
// Accepting the pasted link as well as the code keeps the whole exchange inside the app: long-press
// the link, copy, paste. It costs nothing when a code is present.

export interface EmailCredential {
  readonly kind: 'code' | 'link';
  readonly token: string;
}

/**
 * Normalise pasted input into something verifyOtp can use.
 *
 * Returns null when a link was pasted but carried no verifiable token, so the caller can say that
 * plainly instead of reporting an invalid code for something that was never a code.
 */
export function credentialFromInput(raw: string): EmailCredential | null {
  const value = typeof raw === 'string' ? raw.trim() : '';
  if (value === '') return null;

  const lower = value.toLowerCase();
  if (!lower.startsWith('http://') && !lower.startsWith('https://')) {
    // Anything that is not a URL is treated as a code, including formats this build has not seen.
    // Letting the auth service reject it beats guessing a shape here and refusing a valid code.
    return { kind: 'code', token: value };
  }

  try {
    const url = new URL(value);
    const hash = new URLSearchParams(url.hash.replace(/^#/, ''));
    const token = url.searchParams.get('token_hash')
      ?? url.searchParams.get('token')
      ?? hash.get('token_hash')
      ?? hash.get('token');
    if (token !== null && token !== '') return { kind: 'link', token };
    // A link whose fragment already carries a session belongs to the callback page, not here.
    return null;
  } catch {
    return null;
  }
}
