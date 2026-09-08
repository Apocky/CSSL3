import Link from 'next/link';

import { SUPPORT_LINKS } from '../../lib/support-links';
import styles from '../../styles/SymbolicStudio.module.css';

export default function ConversionBridge({ source }: { source: 'spellcraft' | 'sigils' | 'spellbook' }): JSX.Element {
  const koFi = SUPPORT_LINKS[0];
  return (
    <aside className={styles.conversionBridge} aria-labelledby={`${source}-support-title`}>
      <div>
        <h2 id={`${source}-support-title`}>Enjoying the tools?</h2>
        <p>They’re free to use. If you’d like to help make more, you can support the work.</p>
      </div>
      <div className={styles.conversionActions}>
        <a href={`${koFi.href}?source=apocky-${source}`} target="_blank" rel="noopener noreferrer">{koFi.label} ↗</a>
        <Link href="/membership">Ways to help →</Link>
      </div>
    </aside>
  );
}
