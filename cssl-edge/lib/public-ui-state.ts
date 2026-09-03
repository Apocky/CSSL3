export type PublicUiPhase = 'idle' | 'ready' | 'empty' | 'retryable_error' | 'terminal_error';

export type PublicUiStateCode =
  | 'ATLAS_READY'
  | 'ATLAS_FILTER_EMPTY'
  | 'ATLAS_RENDER_FAILED'
  | 'EXTERNAL_HANDOFF';

export interface PublicUiState {
  readonly phase: PublicUiPhase;
  readonly code: PublicUiStateCode;
  readonly title: string;
  readonly message: string;
}

export const ATLAS_EMPTY_STATE: PublicUiState = {
  phase: 'empty',
  code: 'ATLAS_FILTER_EMPTY',
  title: 'No matching coordinates',
  message: 'No public destinations match this combination. Reset the filters to restore the complete Atlas.',
};

export const ATLAS_RENDER_ERROR_STATE: PublicUiState = {
  phase: 'retryable_error',
  code: 'ATLAS_RENDER_FAILED',
  title: 'The visual field could not render',
  message: 'The complete route index is still available. Reset this view or use the destination links directly.',
};
