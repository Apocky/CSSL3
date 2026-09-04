import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildReviewCorpus, parseReviewArgs } from './snapshot-conversation-corpus.mjs';

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  let args;
  try {
    args = parseReviewArgs(process.argv.slice(2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
  if (args !== undefined) {
    buildReviewCorpus(args).then((manifest) => {
      console.log(`conversation review corpus : ${args.check ? 'CURRENT' : 'WROTE'} · ${manifest.counts.uniqueConversations} conversations · ${manifest.counts.messages} messages · ${manifest.counts.redactions} redactions · external-output`);
    }).catch((error) => {
      console.error(error instanceof Error ? error.message : String(error));
      process.exitCode = 1;
    });
  }
}
