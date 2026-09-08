import { once } from 'node:events';
import { createAdapters } from './adapters';
import { loadGatewayConfig } from './config';
import { createGatewayServer } from './gateway';

async function main(): Promise<void> {
  const config = loadGatewayConfig();
  const server = createGatewayServer(config, createAdapters(config));
  server.listen(config.port, config.host);
  await once(server, 'listening');
  process.stdout.write(JSON.stringify({
    schema: 'apocrypha.memory.gateway.started.v1',
    host: config.host,
    port: config.port,
    authority: 'none',
    read_only: true,
  }) + '\n');
  const stop = () => server.close(() => process.exit(0));
  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);
}

void main().catch(() => {
  process.stderr.write('apocrypha-memory-gateway: startup failed\n');
  process.exitCode = 1;
});
