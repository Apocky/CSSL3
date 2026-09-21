/**
 * Entrypoint for the Discord bridge service.
 *
 * Run:  node --env-file=D:\Apocrypha\discord.env --import tsx scripts/apocrypha-discord/runner.ts
 *
 * Exposes a loopback health endpoint on 19133 so `apocrypha` and apx-diag can see it, and so a
 * probe distinguishes "the bridge is down" from "the bridge is up but its mind is not" -- which
 * are different problems with different fixes and used to be indistinguishable from outside.
 */
import { createServer } from 'node:http';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { Bridge, memoryStore, type BridgeState, type StateStore } from './bridge';
import { ConfigError, describe, loadConfig } from './config';
import { MindClient } from './mind';
import { DiscordRest, inviteUrl } from './rest';

function log(event: string, fields: Record<string, string | number> = {}): void {
  console.log(JSON.stringify({ ts: new Date().toISOString(), event, ...fields }));
}

/**
 * State on disk: who asked to be left alone, and where we have already introduced ourselves.
 * A halt must survive a restart, or "stop" means "stop until the next reboot", which is not what
 * the word means.
 */
function fileStore(dir: string): StateStore {
  const path = join(dir, 'bridge-state.json');
  try {
    mkdirSync(dir, { recursive: true });
  } catch (err) {
    log('state.dir_unavailable', { dir, error: String((err as Error).message) });
    return memoryStore();
  }
  return {
    read: () => {
      try {
        const parsed = JSON.parse(readFileSync(path, 'utf8')) as Partial<BridgeState>;
        return {
          halted: Array.isArray(parsed.halted) ? parsed.halted.map(String) : [],
          announced: Array.isArray(parsed.announced) ? parsed.announced.map(String) : [],
        };
      } catch {
        return { halted: [], announced: [] };
      }
    },
    write: (state) => {
      writeFileSync(path, JSON.stringify(state, null, 2), 'utf8');
    },
  };
}

/**
 * `--check` answers "did I set this up right?" WITHOUT opening a gateway connection.
 *
 * It exists because the alternative is starting the bot to find out, and a bad token or a missing
 * intent costs an IDENTIFY against a 1000-per-day budget and produces a close code rather than a
 * sentence. This validates everything reachable locally and says exactly what is left to do.
 */
async function check(): Promise<never> {
  let config;
  try {
    config = loadConfig();
  } catch (err) {
    if (err instanceof ConfigError) {
      console.error('\n  NOT READY\n\n  ' + err.message + '\n');
      process.exit(1);
    }
    throw err;
  }

  const summary = describe(config);
  console.log('\n  Discord bridge configuration');
  for (const [key, value] of Object.entries(summary)) {
    console.log('    ' + key.padEnd(18) + String(value));
  }

  const mind = new MindClient({
    baseUrl: config.mindUrl,
    model: config.mindModel,
    timeoutMs: 5_000,
  });
  const mindUp = await mind.healthy();
  console.log('\n  mind service        ' + (mindUp ? 'UP' : 'DOWN  <- start it or Discord has nothing to talk to'));

  if (config.applicationId) {
    console.log('\n  Invite link (least privilege: view, send, read history, threads):');
    console.log('    ' + inviteUrl(config.applicationId));
  } else {
    console.log('\n  APOCRYPHA_DISCORD_APPLICATION_ID is unset, so no invite link can be printed.');
  }

  console.log('\n  Remaining manual steps, if you have not done them:');
  console.log('    1. Developer portal -> Bot -> Privileged Gateway Intents -> MESSAGE CONTENT: ON');
  console.log('       Without it, guild messages arrive with EMPTY content and the bot looks mute.');
  console.log('    2. Open the invite link above and add it to a server (DMs work without this).');
  console.log('');
  process.exit(mindUp ? 0 : 1);
}

function main(): void {
  if (process.argv.includes('--check')) {
    void check();
    return;
  }

  let config;
  try {
    config = loadConfig();
  } catch (err) {
    if (err instanceof ConfigError) {
      // A configuration error is the operator's to fix, so it is printed as prose rather than
      // buried in a stack trace.
      console.error('\nDiscord bridge cannot start.\n\n  ' + err.message + '\n');
      process.exit(2);
    }
    throw err;
  }

  log('bridge.config', describe(config));
  if (config.applicationId) {
    log('bridge.invite', { url: inviteUrl(config.applicationId) });
  }

  const rest = new DiscordRest({ token: config.token, log });
  const mind = new MindClient({
    baseUrl: config.mindUrl,
    model: config.mindModel,
    timeoutMs: config.requestTimeoutMs,
  });
  const bridge = new Bridge({
    config,
    rest,
    mind,
    store: fileStore(config.stateDir),
    log,
    onFatal: (code, explanation) => {
      console.error(
        '\nDiscord refused the connection and retrying cannot fix it.\n\n'
        + '  close code ' + String(code) + ': ' + explanation + '\n'
        + (code === 4014
          ? '\n  Fix: open the application in the Discord developer portal, go to Bot, and turn\n'
            + '  on the MESSAGE CONTENT INTENT. It is off by default.\n'
          : code === 4004
            ? '\n  Fix: the token in APOCRYPHA_DISCORD_TOKEN is wrong or has been regenerated.\n'
            : '\n'),
      );
      process.exit(3);
    },
  });

  const health = createServer((req, res) => {
    if (req.method !== 'GET' || !['/health', '/ready'].includes(req.url ?? '')) {
      res.statusCode = 404;
      res.end(JSON.stringify({ error: 'not_found' }));
      return;
    }
    void mind.healthy().then((mindUp) => {
      const body = {
        ok: bridge.gateway.connected,
        gateway: bridge.gateway.connected ? 'connected' : 'disconnected',
        bot_id: bridge.gateway.botId || null,
        mind: mindUp ? 'up' : 'down',
        mind_url: config.mindUrl,
        stats: bridge.stats,
      };
      res.statusCode = body.ok && mindUp ? 200 : 503;
      res.setHeader('content-type', 'application/json');
      res.end(JSON.stringify(body));
    });
  });
  health.listen(config.port, config.host, () => {
    log('bridge.health_listening', { url: 'http://' + config.host + ':' + String(config.port) });
  });

  bridge.start();

  const shutdown = (signal: string): void => {
    log('bridge.shutdown', { signal });
    bridge.stop();
    health.close();
    process.exit(0);
  };
  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));
}

main();
