import type { NextApiRequest, NextApiResponse } from 'next';

import {
  CORPUS_REVIEW_HELD_CODE,
  type ConversationCorpusPageResponse,
  type ConversationCorpusRecord,
} from '@/lib/conversation-corpus';
import { getBundledPublicConversationManifest } from '@/lib/server/conversation-corpus-manifest';

const ID = /^[a-f0-9]{20}$/u;
const ROLES = new Set(['all', 'user', 'assistant']);
const BRANCHES = new Set(['all', 'primary', 'alternate']);

function integer(value: string | string[] | undefined, fallback: number, maximum: number): number {
  const raw = Array.isArray(value) ? value[0] : value;
  const parsed = Number.parseInt(raw ?? '', 10);
  return Number.isFinite(parsed) ? Math.max(0, Math.min(maximum, parsed)) : fallback;
}

function held(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store');
  res.status(423).json({
    error: {
      code: CORPUS_REVIEW_HELD_CODE,
      message: 'This conversation remains local while its privacy and publication rights are reviewed.',
    },
  });
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    res.status(405).json({ error: { code: 'CORPUS_METHOD_NOT_ALLOWED', message: 'Use GET for a public conversation page.' } });
    return;
  }
  const id = Array.isArray(req.query.id) ? req.query.id[0] : req.query.id;
  if (typeof id !== 'string' || !ID.test(id)) {
    res.status(400).json({ error: { code: 'CORPUS_ID_INVALID', message: 'Conversation ID must be 20 lowercase hexadecimal characters.' } });
    return;
  }

  try {
    const manifest = getBundledPublicConversationManifest();
    const approvedSummary = manifest.records.find((candidate) => candidate.id === id);
    if (approvedSummary?.editorialReviewState !== 'approved') {
      held(res);
      return;
    }
    const [{ readFile }, { join }] = await Promise.all([import('node:fs/promises'), import('node:path')]);
    const source = await readFile(join(process.cwd(), 'public', 'conversation-corpus', 'approved-records', `${id}.json`), 'utf8');
    const record = JSON.parse(source) as ConversationCorpusRecord;
    if (
      record.id !== id
      || record.projectionSha256 !== approvedSummary.projectionSha256
      || record.editorialReviewState !== 'approved'
      || record.publication.state !== 'owner-approved-public-projection'
      || record.rightsHoldCount !== 0
      || record.privacyHoldCount !== 0
    ) throw new Error('approved record gate mismatch');
    if (record.contentWarnings.length > 0 && req.query.ack !== '1') {
      res.status(428).json({ error: { code: 'CORPUS_CONTENT_NOTICE_REQUIRED', message: 'Acknowledge the record content notice before loading source-derived fields.' } });
      return;
    }

    const roleValue = Array.isArray(req.query.role) ? req.query.role[0] : req.query.role;
    const branchValue = Array.isArray(req.query.branch) ? req.query.branch[0] : req.query.branch;
    const role = ROLES.has(roleValue ?? '') ? roleValue : 'all';
    const branch = BRANCHES.has(branchValue ?? '') ? branchValue : 'all';
    const queryValue = Array.isArray(req.query.q) ? req.query.q[0] : req.query.q;
    const query = (queryValue ?? '').trim().slice(0, 160).toLocaleLowerCase();
    const offset = integer(req.query.offset, 0, 100_000);
    const limit = integer(req.query.limit, 24, 50) || 24;
    const filtered = record.messages.filter((message) => (
      (role === 'all' || message.role === role)
      && (branch === 'all' || message.branch === branch)
      && (query.length === 0 || message.text.toLocaleLowerCase().includes(query))
    ));
    const messages = filtered.slice(offset, offset + limit);
    const nextOffset = offset + messages.length < filtered.length ? offset + messages.length : null;
    const { messages: _messages, schema: _recordSchema, publication: _publication, ...metadata } = record;
    const response: ConversationCorpusPageResponse = {
      schema: 'apocky.public-conversation-page.v1',
      record: {
        ...metadata,
        href: `/conversations/${record.slug}`,
        bodyHref: `/conversation-corpus/approved-records/${record.id}.json`,
      },
      messages,
      page: { offset, limit, total: filtered.length, nextOffset },
    };
    res.setHeader('Cache-Control', 'public, max-age=0, s-maxage=3600, stale-while-revalidate=86400');
    res.status(200).json(response);
  } catch (error: unknown) {
    const code = (error as NodeJS.ErrnoException)?.code;
    if (code === 'ENOENT') {
      res.status(404).json({ error: { code: 'CORPUS_RECORD_NOT_FOUND', message: 'That public conversation record does not exist.' } });
      return;
    }
    res.status(500).json({ error: { code: 'CORPUS_READ_FAILED', message: 'The public conversation projection could not be read.' } });
  }
}
