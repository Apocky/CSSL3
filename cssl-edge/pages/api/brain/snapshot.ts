import type { NextApiRequest, NextApiResponse } from 'next';

import type { BrainMessage, BrainMemory, BrainSnapshot } from '@/lib/brain/contracts';
import { ownerBrainRuntimeConfigured } from '@/lib/brain/runtime-provider';
import {
  requireBrainOwner,
  respondBrainOwnerFailure,
  setBrainPrivateHeaders,
} from '@/lib/brain/owner';
import { deriveMemberProfileId, requireStoredMnemeProfile, respondMnemeMemberFailure } from '@/lib/mneme/member-profile';
import {
  getMessagesByIds,
  getMnemeClient,
  listMemories,
  listMessages,
  memoryToPublic,
} from '@/lib/mneme/store';
import type { Message } from '@/lib/mneme/types';
import { envelope } from '@/lib/response';

const MEMORY_LIMIT = 200;
const RECENT_MESSAGE_LIMIT = 120;
const SOURCE_MESSAGE_LIMIT = 200;

function publicMemory(memory: ReturnType<typeof memoryToPublic>): BrainMemory {
  return {
    id: memory.id,
    type: memory.type,
    csl: memory.csl,
    paraphrase: memory.paraphrase,
    topic_key: memory.topic_key,
    search_queries: memory.search_queries,
    source_msg_ids: memory.source_msg_ids,
    created_at: memory.created_at,
  };
}

function publicMessage(message: Message, sourceOnly: boolean): BrainMessage {
  return {
    id: message.id,
    session_id: message.session_id,
    role: message.role,
    content: message.content,
    ts: message.ts,
    source_only: sourceOnly,
  };
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setBrainPrivateHeaders(res);
  res.setHeader('Allow', 'GET');
  if (req.method !== 'GET') {
    res.status(405).json({ error: 'Method not allowed', code: 'BRAIN_METHOD_NOT_ALLOWED', ...envelope() });
    return;
  }

  const owner = await requireBrainOwner(req);
  if (!owner.ok) {
    respondBrainOwnerFailure(res, owner);
    return;
  }

  const client = getMnemeClient();
  const profileId = deriveMemberProfileId(owner.user.id);
  const storageFailure = await requireStoredMnemeProfile(client, profileId);
  if (storageFailure) {
    respondMnemeMemberFailure(res, storageFailure);
    return;
  }

  try {
    const listed = await listMemories(client!, profileId, { limit: MEMORY_LIMIT });
    const recentMessages = await listMessages(client!, profileId, { limit: RECENT_MESSAGE_LIMIT });
    const recentIds = new Set(recentMessages.map(message => message.id));
    const sourceIds = listed.memories
      .flatMap(memory => memory.source_msg_ids)
      .filter(id => !recentIds.has(id))
      .slice(0, SOURCE_MESSAGE_LIMIT);
    const sourceMessages = await getMessagesByIds(client!, profileId, sourceIds);
    const memories = listed.memories.map(memoryToPublic).map(publicMemory);
    const messages = [
      ...recentMessages.map(message => publicMessage(message, false)),
      ...sourceMessages.map(message => publicMessage(message, true)),
    ];
    const env = envelope();
    const body: BrainSnapshot = {
      schema_version: 'apocky.owner-brain.snapshot.v1',
      status: 'live',
      connectors: {
        mneme_storage: 'live',
        source_projection: 'live',
        local_apocv4: ownerBrainRuntimeConfigured() ? 'degraded' : 'retired',
      },
      memories,
      messages,
      counts: {
        memories: memories.length,
        messages: recentMessages.length,
        source_links: memories.reduce((sum, memory) => sum + memory.source_msg_ids.length, 0),
      },
      limits: {
        memories: MEMORY_LIMIT,
        recent_messages: RECENT_MESSAGE_LIMIT,
        source_messages: SOURCE_MESSAGE_LIMIT,
      },
      served_by: env.served_by,
      ts: env.ts,
    };
    res.status(200).json(body);
  } catch (error) {
    // Never log private memory payloads or raw upstream messages.
    // eslint-disable-next-line no-console
    console.error(JSON.stringify({ evt: 'brain.snapshot.fail', code: error instanceof Error ? error.name : 'UNKNOWN' }));
    res.status(502).json({
      error: 'The private Brain could not read its memory projection. No data was changed.',
      code: 'BRAIN_SNAPSHOT_FAILED',
      ...envelope(),
    });
  }
}
