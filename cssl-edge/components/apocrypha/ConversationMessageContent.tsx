import { useMemo } from 'react';
import { markdownToHtml } from '@/lib/markdown';
import styles from './ConversationMessageContent.module.css';

const MAX_FORMATTED_MESSAGE_CHARS = 65_536;

export default function ConversationMessageContent({ content, assistant }: {
  readonly content: string;
  readonly assistant: boolean;
}): JSX.Element {
  const html = useMemo(() => assistant && content.length <= MAX_FORMATTED_MESSAGE_CHARS
    ? markdownToHtml(content) : null, [assistant, content]);
  if (html === null) return <div className={styles.plain}>{content}</div>;
  // § Existing renderer escapes source HTML before fixed markup; hrefs are http(s)-only.
  return <div className={styles.markdown} dangerouslySetInnerHTML={{ __html: html }} />;
}
