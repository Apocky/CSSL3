// What Apocrypha can actually DO, made visible.
//
// The chat had three conversation openers on the empty state and nothing after that  --  so every
// capability behind this room (sigils, readings, code, the agentic work lane) was reachable only
// if you already knew it existed and typed the right sentence. A capability nobody can see is a
// capability nobody has.
//
// Two rules this strip follows, because the alternative is a chat that does things behind your back:
//
//   NOTHING FIRES ON ITS OWN.  A generative button writes a prompt into the composer and puts the
//   cursor where your part goes. It does not send. You read it, finish it, and send it yourself  -- 
//   so the tool is a shortcut through typing, never an action you did not authorise.
//   THE AGENT IS A DOOR, NOT A BUTTON.  The coding agent edits real files on the machine that hosts
//   this site. It gets a link to its own console with its own consent gates, not an inline trigger.

import Link from 'next/link';
import styles from '@/styles/ApocryphaChat.module.css';

export interface ChatTool {
  readonly id: string;
  readonly label: string;
  readonly hint: string;
  /** Text written into the composer. `cursor` marks where the visitor's own words go. */
  readonly prompt: string;
  readonly cursor?: string;
}

// Ordered by how often someone actually wants them, not by how impressive they sound.
export const CHAT_TOOLS: readonly ChatTool[] = [
  {
    id: 'code',
    label: 'Write code',
    hint: 'Ask for a small, complete, runnable program',
    prompt: 'Write me a small, complete, runnable program that ',
    cursor: 'end',
  },
  {
    id: 'explain',
    label: 'Explain code',
    hint: 'Paste code and get what it does and what could break',
    prompt: 'Explain what this code does, and name what could go wrong with it. Here it is: ',
    cursor: 'end',
  },
  {
    id: 'sigil',
    label: 'Make a sigil',
    hint: 'Turn an intention into a symbol',
    prompt: 'Make me a sigil for this intention, and explain the reduction you used: ',
    cursor: 'end',
  },
  {
    id: 'reading',
    label: 'Draw a reading',
    hint: 'A three-card reading on a question you bring',
    prompt: 'Draw me a three-card reading. My question is: ',
    cursor: 'end',
  },
  {
    id: 'define',
    label: 'Define a word',
    hint: 'A precise definition with its history',
    prompt: 'Define this precisely, and give me its history and how its meaning shifted: ',
    cursor: 'end',
  },
  {
    id: 'think',
    label: 'Think it through',
    hint: 'Work a decision out loud, with the counter-case',
    prompt: 'Help me think through this decision. Give me the strongest case against whatever you land on. Here it is: ',
    cursor: 'end',
  },
];

export function ToolStrip({
  onInsert,
  canAgent,
  disabled,
}: {
  readonly onInsert: (prompt: string) => void;
  /** The owner lane only. The agent writes to disk, so it never appears for anyone else. */
  readonly canAgent: boolean;
  readonly disabled: boolean;
}): JSX.Element {
  return (
    <div className={styles.tools} role="group" aria-label="What Apocrypha can do">
      {CHAT_TOOLS.map((tool) => (
        <button
          key={tool.id}
          type="button"
          className={styles.tool}
          title={tool.hint}
          disabled={disabled}
          onClick={() => onInsert(tool.prompt)}
        >
          {tool.label}
        </button>
      ))}
      {canAgent ? (
        <Link className={styles.toolAgent} href="/work" title="A coding agent that reads and edits real files, with a consent gate on every step">
          Coding agent
          <span aria-hidden="true"> &rarr;</span>
        </Link>
      ) : null}
    </div>
  );
}

export default ToolStrip;
