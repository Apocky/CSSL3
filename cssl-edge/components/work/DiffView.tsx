import styles from './WorkConsole.module.css';

const MAX_LINES = 400;

export function DiffView({ patch }: { readonly patch: string }): JSX.Element {
  const lines = patch.split('\n');
  const shown = lines.slice(0, MAX_LINES);
  return (
    <div className={styles.diff} role="group" aria-label="Proposed change">
      {shown.map((line, index) => {
        const cls = line.startsWith('+++') || line.startsWith('---') || line.startsWith('@@')
          ? styles.diffMeta
          : line.startsWith('+') ? styles.diffAdd
            : line.startsWith('-') ? styles.diffDel
              : undefined;
        return <span key={index} className={`${styles.diffLine} ${cls ?? ''}`}>{line || ' '}</span>;
      })}
      {lines.length > MAX_LINES
        ? <span className={`${styles.diffLine} ${styles.diffMeta}`}>… {lines.length - MAX_LINES} more lines</span>
        : null}
    </div>
  );
}
