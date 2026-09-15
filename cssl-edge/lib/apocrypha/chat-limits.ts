// Message size ceilings, shared by the browser that must refuse early and the server that
// actually enforces them.
//
// This module deliberately imports NOTHING. The ceilings first lived in lib/apocrypha/guest-chat.ts
// beside the code that enforces them, which reads well and broke the build: guest-chat is a server
// module that imports node:crypto, and the moment the browser lane imported a constant from it,
// webpack tried to bundle node:crypto for the client.
//
// The client needs these because a cap it cannot see is a cap that empties the composer and then
// refuses. The server needs them because the client cannot be trusted. Neither can own the number.

/** Guest turns. Counted in BYTES, not characters — see the note on the member cap below. */
export const GUEST_MESSAGE_MAX_BYTES = 8_192;

/**
 * Member turns.
 *
 * Bytes, not UTF-16 units. The composer used to cap on `string.length`, so roughly 4,100 characters
 * of any non-Latin script passed the browser check and was refused by the server — after the
 * composer had already cleared itself and the words were gone.
 */
export const MEMBER_MESSAGE_MAX_BYTES = 16_384;
