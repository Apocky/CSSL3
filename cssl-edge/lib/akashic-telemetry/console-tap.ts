// § Akashic-Webpage-Records · console-tap.ts
// console.error / console.warn capture · ONLY active when consent_tier =
// 'akashic' (full-fidelity tier · gate-table enforces). Wraps once · idempotent.
//
// Substrate-flavor : console messages = side-channel observations. Σ-mask
// gate denies emission for tiers ⊏ akashic ; this module only listens. Real
// gate is in client.capture().

import { capture } from './client';
import { redactString } from './sigma-mask';

let installed = false;

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
  installed = true;

  const origErr = console.error.bind(console);
  const origWarn = console.warn.bind(console);

  console.error = (...args: unknown[]): void => {
    try {
      capture('console.error', {
        message: redactString(joinArgs(args)),
      });
    } catch {
      // never break user-code
    }
    origErr(...args);
  };

  console.warn = (...args: unknown[]): void => {
    try {
      capture('console.warn', {
        message: redactString(joinArgs(args)),
      });
    } catch {
      // never break user-code
    }
    origWarn(...args);
  };
}

export function _resetConsoleTapForTests(): void {
  installed = false;
}
