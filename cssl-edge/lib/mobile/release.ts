export interface AndroidMobileArtifact {
  readonly href: string;
  readonly sha256: string;
  readonly bytes: number;
  readonly signing_certificate_sha256: string;
}

export interface NativeMobileRelease {
  readonly schema_version: 'apocky.native-mobile-release.v1';
  readonly access: 'account';
  readonly channel: 'preview';
  readonly version: string;
  readonly android: {
    readonly state: 'preparing' | 'ready';
    readonly artifact: AndroidMobileArtifact | null;
    readonly verification: {
      readonly package_signature: 'pending' | 'verified';
      readonly emulator_launch: 'pending' | 'passed';
      readonly account_sign_in_and_chat: 'pending' | 'passed';
      readonly physical_device: 'pending' | 'passed';
    };
  };
  readonly ios: {
    readonly state: 'preparing' | 'ready';
    readonly distribution: {
      readonly channel: 'app-store' | 'testflight';
      readonly url: string;
    } | null;
  };
}

export const PREPARING_MOBILE_RELEASE: NativeMobileRelease = {
  schema_version: 'apocky.native-mobile-release.v1',
  access: 'account',
  channel: 'preview',
  version: '1.0.0',
  android: {
    state: 'preparing',
    artifact: null,
    verification: {
      package_signature: 'pending',
      emulator_launch: 'pending',
      account_sign_in_and_chat: 'pending',
      physical_device: 'pending',
    },
  },
  ios: { state: 'preparing', distribution: null },
};

const HASH = /^[0-9a-f]{64}$/;
const APK_PATH = /^\/downloads\/[A-Za-z0-9][A-Za-z0-9._-]*\.apk$/;

function row(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown> : null;
}

function exact(value: Record<string, unknown>, keys: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  return actual.length === keys.length && [...keys].sort().every((key, index) => actual[index] === key);
}

function hash(value: unknown): value is string {
  return typeof value === 'string' && HASH.test(value);
}

function appleDistribution(value: unknown): value is NonNullable<NativeMobileRelease['ios']['distribution']> {
  const item = row(value);
  if (!item || !exact(item, ['channel', 'url']) || typeof item.url !== 'string' || item.url.length > 512) return false;
  try {
    const url = new URL(item.url);
    if (url.protocol !== 'https:' || url.username || url.password || url.port || url.search || url.hash) return false;
    if (item.url !== url.toString()) return false;
    if (item.channel === 'testflight') {
      return url.hostname === 'testflight.apple.com' && /^\/join\/[A-Za-z0-9]{8}$/.test(url.pathname);
    }
    return item.channel === 'app-store' && url.hostname === 'apps.apple.com'
      && /^\/(?:[a-z]{2}\/)?app\/(?:[a-z0-9-]+\/)?id[0-9]+$/.test(url.pathname);
  } catch {
    return false;
  }
}

export function parseNativeMobileRelease(value: unknown): NativeMobileRelease | null {
  const source = row(value);
  if (!source || !exact(source, ['schema_version', 'access', 'channel', 'version', 'android', 'ios'])
    || source.schema_version !== 'apocky.native-mobile-release.v1' || source.access !== 'account'
    || source.channel !== 'preview' || typeof source.version !== 'string'
    || !/^[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?$/.test(source.version) || source.version.length > 64) return null;
  const android = row(source.android);
  const ios = row(source.ios);
  if (!android || !exact(android, ['state', 'artifact', 'verification'])
    || !ios || !exact(ios, ['state', 'distribution'])) return null;
  const verification = row(android.verification);
  if (!verification || !exact(verification, ['package_signature', 'emulator_launch', 'account_sign_in_and_chat', 'physical_device'])
    || typeof verification.package_signature !== 'string' || !['pending', 'verified'].includes(verification.package_signature)
    || typeof verification.emulator_launch !== 'string' || !['pending', 'passed'].includes(verification.emulator_launch)
    || typeof verification.account_sign_in_and_chat !== 'string' || !['pending', 'passed'].includes(verification.account_sign_in_and_chat)
    || typeof verification.physical_device !== 'string' || !['pending', 'passed'].includes(verification.physical_device)) return null;
  if (android.state === 'ready') {
    const artifact = row(android.artifact);
    if (!artifact || !exact(artifact, ['href', 'sha256', 'bytes', 'signing_certificate_sha256'])
      || typeof artifact.href !== 'string' || artifact.href.length > 220 || !APK_PATH.test(artifact.href)
      || !hash(artifact.sha256) || !hash(artifact.signing_certificate_sha256)
      || !Number.isSafeInteger(artifact.bytes) || Number(artifact.bytes) <= 0 || Number(artifact.bytes) > 512 * 1024 * 1024
      || verification.package_signature !== 'verified') return null;
  } else if (android.state !== 'preparing' || android.artifact !== null) return null;
  if (ios.state === 'ready') {
    if (!appleDistribution(ios.distribution)) return null;
  } else if (ios.state !== 'preparing' || ios.distribution !== null) return null;
  return value as NativeMobileRelease;
}
