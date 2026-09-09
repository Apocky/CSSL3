// Distribution contract for the Apocrypha desktop client.
//
// Kept separate from lib/mobile/release.ts on purpose: the mobile contract is
// already live, and widening its schema to carry a desktop block would put a
// shipping surface at risk for a new one. The two are read side by side by
// pages/download/apocrypha.tsx.

export type Check = 'pending' | 'passed';

export interface WindowsDesktopArtifact {
  readonly href: string;
  readonly sha256: string;
  readonly bytes: number;
  readonly format: 'nsis-installer';
}

export interface DesktopRelease {
  readonly schema_version: 'apocky.desktop-release.v1';
  readonly access: 'account';
  readonly channel: 'preview';
  readonly version: string;
  readonly windows: {
    readonly state: 'preparing' | 'ready';
    readonly artifact: WindowsDesktopArtifact | null;
    /**
     * Whether the installer carries an Authenticode signature. An unsigned
     * download makes Windows show a SmartScreen warning, so the page must say
     * so plainly rather than let a person meet it unprepared.
     */
    readonly signing: 'unsigned' | 'authenticode';
    readonly verification: {
      readonly launch: Check;
      readonly service_configuration: Check;
      readonly account_sign_in_and_chat: Check;
      readonly installer_install_and_uninstall: Check;
    };
  };
}

export const PREPARING_DESKTOP_RELEASE: DesktopRelease = {
  schema_version: 'apocky.desktop-release.v1',
  access: 'account',
  channel: 'preview',
  version: '0.1.0',
  windows: {
    state: 'preparing',
    artifact: null,
    signing: 'unsigned',
    verification: {
      launch: 'pending',
      service_configuration: 'pending',
      account_sign_in_and_chat: 'pending',
      installer_install_and_uninstall: 'pending',
    },
  },
};

export const CHECK_NAMES = [
  'launch',
  'service_configuration',
  'account_sign_in_and_chat',
  'installer_install_and_uninstall',
] as const;

const HASH = /^[0-9a-f]{64}$/;
const VERSION = /^[0-9]{1,4}\.[0-9]{1,4}\.[0-9]{1,4}$/;
const EXE_PATH = /^\/downloads\/[A-Za-z0-9][A-Za-z0-9._-]*\.exe$/;
const MAX_BYTES = 300 * 1024 * 1024;

function row(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>) : null;
}

function exact(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && [...keys].sort().every((key, index) => actual[index] === key);
}

function check(value: unknown): value is Check {
  return value === 'pending' || value === 'passed';
}

function artifact(value: unknown): value is WindowsDesktopArtifact {
  const item = row(value);
  if (!item || !exact(item, ['href', 'sha256', 'bytes', 'format'])) return false;
  if (item.format !== 'nsis-installer') return false;
  if (typeof item.sha256 !== 'string' || !HASH.test(item.sha256)) return false;
  if (typeof item.bytes !== 'number' || !Number.isInteger(item.bytes) || item.bytes < 1 || item.bytes > MAX_BYTES) return false;
  if (typeof item.href !== 'string' || !EXE_PATH.test(item.href)) return false;
  // A path that survives the pattern but not decoding would still be a way out
  // of /downloads; refuse anything that does not round-trip.
  try {
    if (decodeURIComponent(item.href) !== item.href) return false;
  } catch {
    return false;
  }
  return true;
}

export function parseDesktopRelease(value: unknown): DesktopRelease | null {
  const release = row(value);
  if (!release || !exact(release, ['schema_version', 'access', 'channel', 'version', 'windows'])) return null;
  if (release.schema_version !== 'apocky.desktop-release.v1') return null;
  if (release.access !== 'account' || release.channel !== 'preview') return null;
  if (typeof release.version !== 'string' || !VERSION.test(release.version)) return null;

  const windows = row(release.windows);
  if (!windows || !exact(windows, ['state', 'artifact', 'signing', 'verification'])) return null;
  if (windows.state !== 'preparing' && windows.state !== 'ready') return null;
  if (windows.signing !== 'unsigned' && windows.signing !== 'authenticode') return null;

  const verification = row(windows.verification);
  if (!verification || !exact(verification, CHECK_NAMES)) return null;
  for (const name of CHECK_NAMES) {
    if (!check(verification[name])) return null;
  }

  if (windows.state === 'preparing') {
    if (windows.artifact !== null) return null;
  } else if (!artifact(windows.artifact)) {
    return null;
  }

  // A downloadable build must have been started at least once, and its ability
  // to reach the service must have been observed. Everything else may remain
  // an openly declared pending check on a preview.
  if (windows.state === 'ready') {
    if (verification.launch !== 'passed' || verification.service_configuration !== 'passed') return null;
  }

  return release as unknown as DesktopRelease;
}

/** Checks a person still has to make themselves before trusting this build. */
export function pendingChecks(release: DesktopRelease): string[] {
  const labels: Record<(typeof CHECK_NAMES)[number], string> = {
    launch: 'starting the application',
    service_configuration: 'reaching the service',
    account_sign_in_and_chat: 'account sign-in and chat',
    installer_install_and_uninstall: 'install and uninstall on a clean computer',
  };
  return CHECK_NAMES.filter((name) => release.windows.verification[name] !== 'passed').map((name) => labels[name]);
}
