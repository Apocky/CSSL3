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
  /** Text written into the composer. The caret lands at the end, where your part goes. */
  readonly prompt: string;
}

// The site is Apocrypha and the strongest thing behind it is the coder, so these are the coding
// moves. "Make a sigil" and "Draw a reading" used to be here; they advertised Spellcraft and Chaos
// Tarot, which stopped being part of this site the day every link to them was removed.
//
// HONEST NAMING. These are STARTERS, not tools. The chat engine runs a read-only registry
// (apocrypha-readonly-v1) and its own prompt forbids claiming a tool ran -- so a button here cannot
// read your files or run anything, and it must not dress up as though it can. The lane that has
// real tools is the Work console, and the owner gets a door to it at the end of this row.
export const CHAT_TOOLS: readonly ChatTool[] = [
  {
    id: 'write',
    label: 'Write code',
    hint: 'Ask for a small, complete, runnable program',
    prompt: 'Write a small, complete, runnable program that ',
  },
  {
    id: 'explain',
    label: 'Explain this',
    hint: 'Paste code and get what it does and where it breaks',
    prompt: 'Explain what this code does, and name what could go wrong with it: ',
  },
  {
    id: 'debug',
    label: 'Debug it',
    hint: 'Paste an error and the code that produced it',
    prompt: 'Here is an error and the code that produced it. Work out the cause, not just the fix: ',
  },
  {
    id: 'review',
    label: 'Review my code',
    hint: 'Ask for the strongest objection to your approach',
    prompt: 'Review this code. Give me the strongest objection to it, not a list of nitpicks: ',
  },
  {
    id: 'plan',
    label: 'Plan an approach',
    hint: 'Think a problem through before writing anything',
    prompt: 'Help me plan an approach before I write any code. The problem is: ',
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
    // "What Apocrypha can do" was an overclaim: these start a sentence, they do not perform an act.
    <div className={styles.tools} role="group" aria-label="Ways to start">
      {CHAT_TOOLS.map((tool) => (
        <button
          key={tool.id}
          type="button"
          className={styles.tool}
          title={tool.hint}
          // The hint reached only `title`, which does not exist on touch and is not announced on
          // focus. Prefixed with the visible label so WCAG 2.5.3 (label in name) still holds.
          aria-label={`${tool.label}: ${tool.hint}`}
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
