// § Akashic-Webpage-Records · barrel-export ergonomics
// Public API surface · keep narrow ; internal helpers stay file-scoped.

export {
  init,
  capture,
  capturePageView,
  flush,
  withConsent,
  currentTier,
  currentPolicy,
  storedConsentTier,
  syncConsentFromStorage,
  isTelemetryBlackoutPath,
  shutdown,
  CONSENT_CHANGE_EVENT,
  CONSENT_STORAGE_KEY,
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
  uninstallPerformanceObservers,
  _resetPerfForTests,
} from './performance-observer';

export { installNetworkTap, uninstallNetworkTap, _resetNetTapForTests } from './network-tap';
export { installConsoleTap, uninstallConsoleTap, _resetConsoleTapForTests } from './console-tap';

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
import {
  init as _init,
  shutdown as _shutdown,
  isTelemetryBlackoutPath as _isBlackout,
  _isInit as _initialized,
  _registerLifecycleReconciler,
  syncConsentFromStorage as _syncStoredConsent,
  CONSENT_STORAGE_KEY as _consentStorageKey,
  type InitOpts,
} from './client';
import {
  installPerformanceObservers as _ipo,
  uninstallPerformanceObservers as _upo,
} from './performance-observer';
import { installNetworkTap as _int, uninstallNetworkTap as _unt } from './network-tap';
import { installConsoleTap as _ict, uninstallConsoleTap as _uct } from './console-tap';
import { currentPolicy as _cp } from './client';

let last_opts: InitOpts = {};
let storage_listener_attached = false;

const handleConsentStorage = (event: StorageEvent): void => {
  if (event.key === _consentStorageKey || event.key === null) _syncStoredConsent();
};

function installConsentStorageListener(): void {
  if (storage_listener_attached || typeof window === 'undefined') return;
  window.addEventListener('storage', handleConsentStorage);
  storage_listener_attached = true;
}

function uninstallConsentStorageListener(): void {
  if (!storage_listener_attached || typeof window === 'undefined') return;
  window.removeEventListener('storage', handleConsentStorage);
  storage_listener_attached = false;
}

function uninstallObservers(): void {
  uninstallConsentStorageListener();
  _upo();
  _unt();
  _uct();
}

// Stops observer effects and client state without changing the stored choice.
// Auth/clinical routes use this suspend operation; explicit None additionally
// persists the None choice before the lifecycle reconciler reaches here.
export function akashicDisable(): void {
  uninstallObservers();
  _shutdown();
}

// Reconcile desired consent + route state against the live singleton. This is
// deliberately safe to call on every route and preference transition.
export function akashicInstall(opts: InitOpts = {}): boolean {
  last_opts = opts;
  if (_isBlackout()) {
    akashicDisable();
    return false;
  }
  _init(last_opts);
  if (!_initialized()) {
    uninstallObservers();
    return false;
  }
  if (last_opts.install_observers === false) {
    uninstallObservers();
    return true;
  }
  installConsentStorageListener();
  _ipo();
  _int();
  const policy = _cp();
  if (policy.capture_console) {
    _ict();
  } else {
    _uct();
  }
  return true;
}

_registerLifecycleReconciler(() => {
  akashicInstall(last_opts);
});
