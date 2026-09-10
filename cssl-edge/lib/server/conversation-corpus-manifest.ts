import publicManifestSource from '@/public/conversation-corpus/public-index.v1.json';

import {
  validatePublicConversationManifest,
  type ConversationCorpusManifest,
} from '@/lib/conversation-corpus';

// Static import is intentional: Vercel traces this manifest into every server bundle
// that resolves corpus admission, instead of depending on process.cwd()/public at runtime.
const publicManifest = publicManifestSource as unknown as ConversationCorpusManifest;
validatePublicConversationManifest(publicManifest);

export function getBundledPublicConversationManifest(): ConversationCorpusManifest {
  return publicManifest;
}
