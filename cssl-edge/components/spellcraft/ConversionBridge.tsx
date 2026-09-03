import Link from 'next/link';

import { SUPPORT_LINKS } from '../../lib/support-links';
import styles from '../../styles/SymbolicStudio.module.css';

export default function ConversionBridge({ source }: { source: 'spellcraft' | 'sigils' | 'spellbook' }): JSX.Element {
  const koFi = SUPPORT_LINKS[0];
  return (
    <aside className={styles.conversionBridge} aria-labelledby={`${source}-support-title`}>
      <div>
        <p className={styles.resultKicker}>KEEP THE ENGINE EVOLVING</p>
        <h2 id={`${source}-support-title`}>You received the complete tool. Fund the next dimension if it earned it.</h2>
        <p>Support sustains new vocabularies, deeper visual maps, accessibility work, and independent experiments. It never changes the meaning of your result.</p>
      </div>
      <div className={styles.conversionActions}>
        <a href={`${koFi.href}?source=apocky-${source}`} target="_blank" rel="noopener noreferrer">{koFi.label} ↗</a>
        <Link href="/membership">Compare support paths →</Link>
        <a href={`https://chaos-tarot.com/free-reading?source=apocky-${source}`} target="_blank" rel="noopener noreferrer">Go deeper with Chaos Tarot ↗</a>
      </div>
    </aside>
  );
}
