import Head from 'next/head';
import Link from 'next/link';
import styles from '../styles/UsefulHub.module.css';

const TOOLS = [
  { href: '/sigils', title: 'Make a sigil', description: 'Choose a meaning, shape your symbol, and download it.', mark: '✧', note: 'Create in your browser' },
  { href: '/spellcraft', title: 'Build an intention', description: 'Play with words for change, protection, growth, and more.', mark: '↗', note: 'Try a starting idea' },
  { href: '/spellbook', title: 'Open your spellbook', description: 'Return to the symbols and intentions you saved here.', mark: '▤', note: 'Saved on this device' },
  { href: 'https://chaos-tarot.com/free-reading?source=apocky-tools', title: 'Get a tarot reading', description: 'Bring a question and explore a different way to look at it.', mark: '☾', note: 'Free reading · Chaos Tarot', external: true },
  { href: '/apocrypha', title: 'Talk it through', description: 'Ask Apocrypha a question and keep the conversation going.', mark: '“', note: 'Sign in to chat' },
  { href: '/words', title: 'Find a meaning', description: 'Look up a word or symbol in plain language.', mark: 'Aa', note: 'Search the definitions' },
];
export default function Tools(): JSX.Element {
  return <><Head><title>Tools to make, reflect, and explore · Apocky</title><meta name="description" content="Make a sigil, build an intention, open your spellbook, find a definition, or get a tarot reading." /><meta name="viewport" content="width=device-width, initial-scale=1" /><link rel="canonical" href="https://www.apocky.com/tools" /></Head>
    <main className={styles.page}><header className={styles.pageHeader}><p className={styles.overline}>Choose something to try</p><h1>A little curiosity.<br />Something you can use.</h1><p>Start with a tool. Make it your own.</p></header>
      <div className={styles.toolGrid}>{TOOLS.map(tool => {
        const content = <><span className={styles.toolMark} aria-hidden="true">{tool.mark}</span><span className={styles.note}>{tool.note}</span><h2>{tool.title}</h2><p>{tool.description}</p><span className={styles.cardAction}>Open {tool.external ? '↗' : '→'}</span></>;
        return tool.external ? <a className={styles.toolCard} href={tool.href} key={tool.href} target="_blank" rel="noopener noreferrer">{content}</a> : <Link className={styles.toolCard} href={tool.href} key={tool.href}>{content}</Link>;
      })}</div>
      <aside className={styles.quietNote}>Looking for something you saved? <Link href="/spellbook">Your spellbook</Link> keeps creations on this device. <Link href="/memory-tools">Find your saved work →</Link></aside>
    </main></>;
}
