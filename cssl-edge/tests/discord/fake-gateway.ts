/**
 * A real WebSocket server that speaks Discord's gateway protocol, so the bridge can be tested
 * without a bot token.
 *
 * WHY THIS EXISTS: the token is Apocky's to create and I never hold one. Without a test double,
 * every gate for this bridge would amount to "it looked right when I read it" -- and the
 * reconnect, resume and heartbeat paths are precisely the ones that only misbehave against a live
 * socket. So the socket here is real: RFC 6455 framing over real TCP on loopback. Only the far
 * end is fake.
 *
 * It is deliberately a SERVER and not a mocked WebSocket class. Mocking the class would test my
 * idea of what Node's WebSocket does; this tests what it actually does, which is the whole point.
 *
 * Scope: text frames, close, ping/pong, payloads under 64 KiB -- every gateway payload this bridge
 * will ever see. Binary and continuation frames throw instead of being half-supported, because a
 * harness that silently mishandles a frame teaches you something false.
 */
import { createHash } from 'node:crypto';
import { createServer, type Server } from 'node:http';
import type { Duplex } from 'node:stream';

// RFC 6455 s1.3. Pinned by the RFC's own worked example in fake-gateway.test.ts, because the
// first version of this constant had the last group's leading C moved to the end
// (...-95CA-5AB0DC85B11A instead of ...-95CA-C5AB0DC85B11). Every handshake failed with a bare
// close 1006 and no diagnostic, which is what a wrong accept key looks like from the client side.
const WS_GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11';

export const OP = {
  DISPATCH: 0,
  HEARTBEAT: 1,
  IDENTIFY: 2,
  RESUME: 6,
  RECONNECT: 7,
  INVALID_SESSION: 9,
  HELLO: 10,
  HEARTBEAT_ACK: 11,
} as const;

function acceptKey(key: string): string {
  return createHash('sha1').update(key + WS_GUID).digest('base64');
}

/** Server -> client frames are never masked (RFC 6455 s5.1). */
function encodeFrame(opcode: number, payload: Buffer): Buffer {
  const n = payload.length;
  if (n > 0xffff) throw new Error('fake-gateway: payload exceeds the 64 KiB test ceiling');
  const head = n < 126
    ? Buffer.from([0x80 | opcode, n])
    : Buffer.from([0x80 | opcode, 126, (n >> 8) & 0xff, n & 0xff]);
  return Buffer.concat([head, payload]);
}

interface Decoded {
  opcode: number;
  payload: Buffer;
  rest: Buffer;
}

/** Client -> server frames MUST be masked; an unmasked one is a protocol violation, not a quirk. */
function decodeFrame(buf: Buffer): Decoded | null {
  if (buf.length < 2) return null;
  const fin = (buf[0] & 0x80) !== 0;
  const opcode = buf[0] & 0x0f;
  const masked = (buf[1] & 0x80) !== 0;
  let len = buf[1] & 0x7f;
  let off = 2;
  if (len === 126) {
    if (buf.length < 4) return null;
    len = buf.readUInt16BE(2);
    off = 4;
  } else if (len === 127) {
    throw new Error('fake-gateway: 64-bit frame length is out of test scope');
  }
  if (!fin) throw new Error('fake-gateway: continuation frames are out of test scope');
  if (!masked) throw new Error('fake-gateway: client frame was not masked (RFC 6455 violation)');
  if (buf.length < off + 4 + len) return null;
  const mask = buf.subarray(off, off + 4);
  const data = Buffer.from(buf.subarray(off + 4, off + 4 + len));
  for (let i = 0; i < data.length; i += 1) data[i] ^= mask[i & 3];
  return { opcode, payload: data, rest: buf.subarray(off + 4 + len) };
}

export interface GatewayPayload {
  op: number;
  d?: unknown;
  s?: number | null;
  t?: string | null;
}

export interface FakeGatewayOptions {
  /** ms. The bridge must jitter its FIRST heartbeat somewhere inside this window. */
  heartbeatInterval?: number;
  /** Refuse the next RESUME with op9 d:false, forcing a fresh IDENTIFY. */
  invalidateSession?: boolean;
  /** Stop ACKing heartbeats, so the client has to notice a zombie connection and reconnect. */
  withholdAcks?: boolean;
}

/**
 * One connected bridge. Records what it received so tests can assert on the handshake itself
 * rather than on log output.
 */
export class FakeConnection {
  readonly received: GatewayPayload[] = [];
  readonly heartbeatsAt: number[] = [];
  identify: Record<string, unknown> | null = null;
  resume: Record<string, unknown> | null = null;
  closedWith: number | null = null;
  sequence = 0;
  readonly openedAt = Date.now();

  constructor(
    private readonly socket: Duplex,
    private readonly opts: FakeGatewayOptions,
    readonly sessionId: string,
    /** Handed back as resume_gateway_url so a resume actually reconnects somewhere real. */
    private readonly selfUrl: string,
  ) {}

  send(payload: GatewayPayload): void {
    if (this.socket.destroyed) return;
    this.socket.write(encodeFrame(0x1, Buffer.from(JSON.stringify(payload), 'utf8')));
  }

  /** Emit an event as if Discord dispatched it, with a real monotonic sequence number. */
  dispatch(type: string, data: unknown): number {
    this.sequence += 1;
    this.send({ op: OP.DISPATCH, t: type, s: this.sequence, d: data });
    return this.sequence;
  }

  close(code: number, reason = ''): void {
    if (this.socket.destroyed) return;
    const body = Buffer.concat([
      Buffer.from([(code >> 8) & 0xff, code & 0xff]),
      Buffer.from(reason, 'utf8'),
    ]);
    this.socket.write(encodeFrame(0x8, body));
    this.socket.end();
  }

  handle(payload: GatewayPayload): void {
    this.received.push(payload);
    if (payload.op === OP.HEARTBEAT) {
      this.heartbeatsAt.push(Date.now());
      if (!this.opts.withholdAcks) this.send({ op: OP.HEARTBEAT_ACK });
      return;
    }
    if (payload.op === OP.IDENTIFY) {
      this.identify = payload.d as Record<string, unknown>;
      this.dispatch('READY', {
        v: 10,
        user: { id: '900000000000000001', username: 'apocrypha', bot: true },
        session_id: this.sessionId,
        resume_gateway_url: this.selfUrl,
        guilds: [],
        application: { id: '900000000000000002' },
      });
      return;
    }
    if (payload.op === OP.RESUME) {
      this.resume = payload.d as Record<string, unknown>;
      if (this.opts.invalidateSession) this.send({ op: OP.INVALID_SESSION, d: false });
      else this.dispatch('RESUMED', {});
    }
  }
}

export class FakeGateway {
  /** Set by listen(); handed to each connection as resume_gateway_url. */
  url = '';

  private readonly server: Server;
  readonly connections: FakeConnection[] = [];
  private readonly sockets = new Set<Duplex>();
  private seq = 0;

  constructor(private readonly opts: FakeGatewayOptions = {}) {
    this.server = createServer((_req, res) => {
      res.writeHead(400).end('websocket only');
    });
    this.server.on('upgrade', (req, socket) => this.onUpgrade(req.headers, socket as Duplex));
  }

  get heartbeatInterval(): number {
    return this.opts.heartbeatInterval ?? 45_000;
  }

  /** The connection a test cares about: the most recent one. */
  get latest(): FakeConnection {
    const conn = this.connections.at(-1);
    if (!conn) throw new Error('fake-gateway: nothing has connected yet');
    return conn;
  }

  private onUpgrade(headers: Record<string, unknown>, socket: Duplex): void {
    const key = headers['sec-websocket-key'];
    if (typeof key !== 'string') {
      socket.destroy();
      return;
    }
    socket.write(
      'HTTP/1.1 101 Switching Protocols\r\n'
      + 'Upgrade: websocket\r\n'
      + 'Connection: Upgrade\r\n'
      + 'Sec-WebSocket-Accept: ' + acceptKey(key) + '\r\n\r\n',
    );
    this.seq += 1;
    this.sockets.add(socket);
    socket.on('close', () => this.sockets.delete(socket));
    const conn = new FakeConnection(socket, this.opts, 'fake-session-' + String(this.seq),
      this.url);
    this.connections.push(conn);

    let buffer = Buffer.alloc(0);
    socket.on('data', (chunk: Buffer) => {
      buffer = Buffer.concat([buffer, chunk]);
      for (;;) {
        const frame = decodeFrame(buffer);
        if (!frame) break;
        buffer = frame.rest;
        if (frame.opcode === 0x8) {
          conn.closedWith = frame.payload.length >= 2 ? frame.payload.readUInt16BE(0) : 1005;
          socket.end();
          return;
        }
        if (frame.opcode === 0x9) {
          socket.write(encodeFrame(0xa, frame.payload));
          continue;
        }
        if (frame.opcode !== 0x1) continue;
        conn.handle(JSON.parse(frame.payload.toString('utf8')) as GatewayPayload);
      }
    });
    socket.on('error', () => socket.destroy());

    // HELLO starts the client's clock, so it goes out immediately on connect.
    conn.send({ op: OP.HELLO, d: { heartbeat_interval: this.heartbeatInterval } });
  }

  async listen(): Promise<string> {
    await new Promise<void>((resolve) => {
      this.server.listen(0, '127.0.0.1', resolve);
    });
    const addr = this.server.address();
    if (!addr || typeof addr === 'string') throw new Error('fake-gateway: no port assigned');
    this.url = 'ws://127.0.0.1:' + String(addr.port) + '/';
    return this.url;
  }

  /**
   * server.close() only fires once every connection has ended, and a half-open upgraded socket is
   * enough to hang it forever. A test harness that never exits is worse than a failing one: it
   * looks like a hang in the code under test. So the sockets are destroyed outright.
   */
  async stop(): Promise<void> {
    for (const conn of this.connections) conn.close(1001);
    for (const socket of this.sockets) socket.destroy();
    this.sockets.clear();
    this.server.closeAllConnections?.();
    await new Promise<void>((resolve) => {
      this.server.close(() => resolve());
    });
  }
}
