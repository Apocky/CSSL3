/**
 * Bridge configuration, read once at startup.
 *
 * THE TOKEN RULE: the bot token enters this process from an env file and leaves it only inside an
 * Authorization header. It is never logged, never echoed, never included in an error message, and
 * never written to state. describe() below exists so startup can print a full configuration
 * summary without the one field that must not appear in a log.
 */

export interface BridgeConfig {
  token: string;
  ownerId: string;
  optInChannels: ReadonlySet<string>;
  /** The local mind service: message in, Apocrypha out, with persona and memory. */
  mindUrl: string;
  mindModel: string;
  /** How long to wait for an answer before telling the person it is not coming. */
  requestTimeoutMs: number;
  host: string;
  port: number;
  stateDir: string;
  /** Only used to print an invite URL at startup. Not a secret. */
  applicationId: string;
}

export class ConfigError extends Error {}

/** Discord snowflakes are 17-20 digit decimal ids. */
const SNOWFLAKE = /^[0-9]{17,20}$/;

function required(env: NodeJS.ProcessEnv, key: string): string {
  const value = (env[key] ?? '').trim();
  if (!value) {
    throw new ConfigError(
      key + ' is not set. The Discord bridge cannot start without it. '
      + 'See scripts/apocrypha-discord/discord.env.example.',
    );
  }
  return value;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): BridgeConfig {
  const token = required(env, 'APOCRYPHA_DISCORD_TOKEN');
  const ownerId = required(env, 'APOCRYPHA_DISCORD_OWNER_ID');
  if (!SNOWFLAKE.test(ownerId)) {
    throw new ConfigError(
      'APOCRYPHA_DISCORD_OWNER_ID must be a Discord user id (17-20 digits), not a username. '
      + 'Enable Developer Mode in Discord, then right-click your name and Copy User ID.',
    );
  }

  const channels = (env.APOCRYPHA_DISCORD_CHANNELS ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  for (const id of channels) {
    if (!SNOWFLAKE.test(id)) {
      throw new ConfigError('APOCRYPHA_DISCORD_CHANNELS contains a value that is not a channel id: ' + id);
    }
  }

  return {
    token,
    ownerId,
    optInChannels: new Set(channels),
    // 19132, not 19131. The mind service's own code default is 19131, but that port is probed
    // elsewhere as a "stray second engine" check, so binding it makes the work lane's doctor
    // report a phantom. The mind is expected on 19132.
    mindUrl: (env.APOCRYPHA_DISCORD_MIND_URL ?? 'http://127.0.0.1:19132').replace(/\/+$/, ''),
    mindModel: env.APOCRYPHA_DISCORD_MIND_MODEL ?? 'apocrypha',
    requestTimeoutMs: positiveInt(env.APOCRYPHA_DISCORD_TIMEOUT_MS, 180_000),
    host: env.APOCRYPHA_DISCORD_HOST ?? '127.0.0.1',
    port: positiveInt(env.APOCRYPHA_DISCORD_PORT, 19133),
    stateDir: env.APOCRYPHA_DISCORD_STATE_DIR ?? 'D:\\Apocrypha\\discord',
    applicationId: (env.APOCRYPHA_DISCORD_APPLICATION_ID ?? '').trim(),
  };
}

function positiveInt(raw: string | undefined, fallback: number): number {
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : fallback;
}

/** Everything about the configuration that is safe to print. Note the absence of `token`. */
export function describe(config: BridgeConfig): Record<string, string | number> {
  return {
    owner: config.ownerId,
    opt_in_channels: config.optInChannels.size,
    mind: config.mindUrl,
    model: config.mindModel,
    timeout_ms: config.requestTimeoutMs,
    listen: config.host + ':' + String(config.port),
    state: config.stateDir,
    token: 'set (' + String(config.token.length) + ' chars, not shown)',
  };
}
