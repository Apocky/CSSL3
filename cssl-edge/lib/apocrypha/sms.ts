// Stable server-only entrypoint for the provider-neutral SMS domain and the
// provisional Twilio adapter. Provider-specific webhook details stay in the
// adapter; consent, encryption, session binding, and spend policy do not.

export * from './sms/config';
export * from './sms/core';
export * from './sms/twilio';
