// § Akashic-Webpage-Records · console-tap.ts
// console.error / console.warn capture · ONLY active when consent_tier =
// 'akashic' (full-fidelity tier · gate-table enforces). Wraps once · idempotent.
//
// Substrate-flavor : console messages = side-channel observations. Σ-mask
// gate denies emission for tiers ⊏ akashic ; this module only listens. Real
// gate is in client.capture().

import { _isInit, capture, currentTier, isTelemetryBlackoutPath } from './client';
import { redactString } from './sigma-mask';

let installed = false;
let originalError: typeof console.error | null = null;
let originalWarn: typeof console.warn | null = null;
let wrappedError: typeof console.error | null = null;
let wrappedWarn: typeof console.warn | null = null;

function safeStringify(arg: unknown): string {
  if (typeof arg === 'string') return arg;
  if (arg instanceof Error) return `${arg.name}: ${arg.message}`;
  try {
    return JSON.stringify(arg);
  } catch {
    return String(arg);
  }
}

function joinArgs(args: unknown[]): string {
  return args.map((a) => safeStringify(a)).join(' ').slice(0, 2000);
}

export function installConsoleTap(): void {
  if (installed) return;
  if (typeof console === 'undefined') return;
  if (!_isInit() || currentTier() !== 'akashic' || isTelemetryBlackoutPath()) return;
  installed = true;

  originalError = console.error;
  originalWarn = console.warn;
  const origErr = originalError.bind(console);
  const origWarn = originalWarn.bind(console);

  wrappedError = (...args: unknown[]): void => {
    try {
      capture('console.error', {
        message: redactString(joinArgs(args)),
      });
    } catch {
      // never break user-code
    }
    origErr(...args);
  };
  console.error = wrappedError;

  wrappedWarn = (...args: unknown[]): void => {
    try {
      capture('console.warn', {
        message: redactString(joinArgs(args)),
      });
    } catch {
      // never break user-code
    }
    origWarn(...args);
  };
  console.warn = wrappedWarn;
}

export function uninstallConsoleTap(): void {
  if (
    typeof console !== 'undefined' &&
    originalError !== null &&
    wrappedError !== null &&
    console.error === wrappedError
  ) {
    console.error = originalError;
  }
  if (
    typeof console !== 'undefined' &&
    originalWarn !== null &&
    wrappedWarn !== null &&
    console.warn === wrappedWarn
  ) {
    console.warn = originalWarn;
  }
  originalError = null;
  originalWarn = null;
  wrappedError = null;
  wrappedWarn = null;
  installed = false;
}

export function _resetConsoleTapForTests(): void {
  uninstallConsoleTap();
}
