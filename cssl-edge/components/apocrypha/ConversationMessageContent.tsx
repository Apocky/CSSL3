import { useMemo } from 'react';
import { markdownToHtml } from '@/lib/markdown';
import { presentable } from '@/lib/apocrypha/deliberation';
import styles from './ConversationMessageContent.module.css';

const MAX_FORMATTED_MESSAGE_CHARS = 65_536;

export default function ConversationMessageContent({ content, assistant }: {
  readonly content: string;
  readonly assistant: boolean;
}): JSX.Element {
  // A leaked thought is never the answer (lib/apocrypha/deliberation.ts). Applied to assistant
  // text only; a member's own words are never rewritten.
  const shown = useMemo(() => assistant ? presentable(content) : { text: content, withheld: null }, [assistant, content]);
  const html = useMemo(() => assistant && shown.withheld === null && shown.text.length <= MAX_FORMATTED_MESSAGE_CHARS
    ? markdownToHtml(shown.text) : null, [assistant, shown]);
  if (shown.withheld !== null) return <div className={styles.plain} data-withheld={shown.withheld}>{shown.text}</div>;
  if (html === null) return <div className={styles.plain}>{shown.text}</div>;
  // § Existing renderer escapes source HTML before fixed markup; hrefs are http(s)-only.
  return <div className={styles.markdown} dangerouslySetInnerHTML={{ __html: html }} />;
}
