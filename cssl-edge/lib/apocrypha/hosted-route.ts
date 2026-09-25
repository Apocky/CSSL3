// Which route a flagship request takes through the AI Gateway.
//
// Measured 2026-09-25: the gateway refuses Opus 5.5 for this team intermittently -- HTTP 429
// "No access to this model at this time", in ~0.15 s, before any provider is tried -- and then
// admits it again minutes later. Owner steering the same day: "If it's a matter of intermittence
// then do a round robin testing each provider in turn when one fails".
//
// So the primary model is tried once per provider (`only: [provider]`, and no gateway `models`
// list, which would let the gateway swap the model silently), rotating on failure; only after
// every provider has refused does the request fall to the other models. The next request starts
// at the provider that last answered.

export const OPUS_55_PROVIDERS: readonly string[] = ['anthropic', 'bedrock', 'vertexAnthropic', 'claudeaws'];

/** Statuses after which another provider (then another model) is worth trying. */
export function retryableStatus(status: number): boolean {
  return status === 403 || status === 429 || status === 529 || (status >= 500 && status <= 504);
}

export interface HostedAttempt {
  readonly model: string;
  /** Pin this attempt to one provider (`providerOptions.gateway.only`). */
  readonly provider?: string;
  /** Gateway-native fallbacks, used only once the primary model's providers are exhausted. */
  readonly models?: readonly string[];
}

let cursor = 0;

export function providerRing(env: Record<string, string | undefined> = process.env): string[] {
  const configured = (env.APOCRYPHA_HOSTED_PROVIDERS ?? '').split(',').map((item) => item.trim()).filter(Boolean);
  return configured.length > 0 ? configured : [...OPUS_55_PROVIDERS];
}

/** Every attempt one flagship request may make, in order. */
export function hostedAttempts(primary: string, fallbacks: readonly string[], env: Record<string, string | undefined> = process.env): HostedAttempt[] {
  const ring = providerRing(env);
  const start = cursor % ring.length;
  const others = fallbacks.filter((model) => model && model !== primary);
  return [
    ...[...ring.slice(start), ...ring.slice(0, start)].map((provider) => ({ model: primary, provider })),
    ...others.map((model, index) => ({ model, models: others.slice(index + 1) })),
  ];
}

/** Move the ring: a provider that answered is where the next request starts; one that refused is passed over. */
export function noteAttempt(attempt: HostedAttempt, answered: boolean, env: Record<string, string | undefined> = process.env): void {
  if (!attempt.provider) return;
  const ring = providerRing(env);
  const index = ring.indexOf(attempt.provider);
  if (index < 0) return;
  cursor = answered ? index : (index + 1) % ring.length;
}

/** The request-body fields an attempt adds to a gateway chat-completions call. */
export function attemptFields(attempt: HostedAttempt): Record<string, unknown> {
  return {
    model: attempt.model,
    ...(attempt.provider ? { providerOptions: { gateway: { only: [attempt.provider] } } } : {}),
    ...(attempt.models && attempt.models.length > 0 ? { models: attempt.models } : {}),
  };
}

export function resetHostedRouteForTests(): void {
  cursor = 0;
}
