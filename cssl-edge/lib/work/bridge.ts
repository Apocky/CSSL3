import { readFile } from 'node:fs/promises';
import { join } from 'node:path';

export const WORK_STATE_DIR = process.env.APOCRYPHA_WORK_STATE_DIR?.trim() || 'C:/Apocrypha/work';
export const WORK_BASE_URL = (process.env.APOCRYPHA_WORK_URL?.trim() || 'http://127.0.0.1:19130').replace(/\/+$/, '');

export class WorkLaneUnavailable extends Error {
  readonly reason: 'no_token' | 'unreachable';

  constructor(reason: 'no_token' | 'unreachable', message: string) {
    super(message);
    this.name = 'WorkLaneUnavailable';
    this.reason = reason;
  }
}

let cached: { token: string; at: number } | null = null;
const TOKEN_TTL_MS = 30_000;

/**
 * Read the Work service token from disk.
 *
 * The token never reaches the browser: this proxy holds it and the page talks only to same-origin
 * routes. That keeps a credential capable of writing files and running commands out of any
 * client-side storage, where an extension or an XSS could reach it.
 */
export async function workToken(): Promise<string> {
  const supplied = process.env.APOCRYPHA_WORK_TOKEN?.trim();
  if (supplied) return supplied;
  if (cached && Date.now() - cached.at < TOKEN_TTL_MS) return cached.token;
  const raw = await readFile(join(WORK_STATE_DIR, 'work.token'), 'utf8').catch(() => null);
  const token = raw?.trim();
  if (!token) {
    throw new WorkLaneUnavailable(
      'no_token',
      'The Work lane runs on your own machine and this server cannot read its token. Open the Work tab from a local Apocky instance.',
    );
  }
  cached = { token, at: Date.now() };
  return token;
}

export function unavailablePayload(error: WorkLaneUnavailable): Record<string, unknown> {
  return {
    error: 'work_lane_unavailable',
    reason: error.reason,
    detail: error.message,
    hint: error.reason === 'no_token'
      ? 'Start the Work service with tools/run-work-service.ps1 on the machine that holds your workspace.'
      : `Nothing is listening on ${WORK_BASE_URL}. Start the Work service, then reload.`,
  };
}
