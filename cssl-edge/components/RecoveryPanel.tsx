import type { PublicUiState } from '../lib/public-ui-state';

interface RecoveryPanelProps {
  readonly state: PublicUiState;
  readonly onReset?: () => void;
  readonly resetLabel?: string;
  readonly className?: string;
}

export default function RecoveryPanel({
  state,
  onReset,
  resetLabel = 'Reset view',
  className,
}: RecoveryPanelProps): JSX.Element {
  const isError = state.phase === 'retryable_error' || state.phase === 'terminal_error';

  return (
    <section
      className={className}
      role={isError ? 'alert' : 'status'}
      aria-live={isError ? 'assertive' : 'polite'}
      aria-atomic="true"
      data-state={state.phase}
      data-code={state.code}
    >
      <p>{state.code}</p>
      <h3>{state.title}</h3>
      <p>{state.message}</p>
      {onReset ? <button type="button" onClick={onReset}>{resetLabel}</button> : null}
    </section>
  );
}
