// § Akashic-Webpage-Records · barrel-export ergonomics
// Public API surface · keep narrow ; internal helpers stay file-scoped.

export {
  init,
  capture,
  flush,
  withConsent,
  currentTier,
  currentPolicy,
  attestVersion,
  purgeAllMine,
  hash16,
  _resetForTests,
  _peekRing,
  _ringSize,
  _sessionId,
  _isInit,
  type InitOpts,
} from './client';

export {
  installPerformanceObservers,
  _resetPerfForTests,
} from './performance-observer';

export { installNetworkTap, _resetNetTapForTests } from './network-tap';
export { installConsoleTap, _resetConsoleTapForTests } from './console-tap';

export {
  AkashicErrorBoundary,
  clusterSignature,
  type AkashicErrorBoundaryProps,
} from './error-boundary';

export {
  CONSENT_TIERS,
  SIGMA_NONE,
  SIGMA_SELF,
  SIGMA_AGGREGATE,
  SIGMA_PATTERN,
  SIGMA_FEDERATED,
  type AkashicEvent,
  type AkashicKind,
  type AkashicBatch,
  type ConsentTier,
  type ConsentPolicy,
  type SigmaMask,
} from './event-types';

export {
  applyGate,
  redactPayload,
  redactString,
  gateEvent,
  KIND_REQUIRED_TIER,
} from './sigma-mask';

// ─── one-shot installer · the "wire-everything" convenience ────────────────
import { init as _init, type InitOpts } from './client';
import { installPerformanceObservers as _ipo } from './performance-observer';
import { installNetworkTap as _int } from './network-tap';
import { installConsoleTap as _ict } from './console-tap';
import { currentPolicy as _cp } from './client';

// Wire everything in the right order : init() FIRST so capture() works ; then
// observers ; console-tap installed unconditionally but the per-kind gate
// denies emission unless tier=akashic.
export function akashicInstall(opts: InitOpts = {}): void {
  const fresh = _init(opts);
  if (!fresh) return;
  _ipo();
  _int();
  // console-tap only when policy permits ; otherwise skip entirely so we
  // don't even monkey-patch console (zero side-effect at lower tiers).
  const policy = _cp();
  if (policy.capture_console) _ict();
}
