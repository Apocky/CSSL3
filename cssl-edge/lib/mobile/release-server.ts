import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { basename, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { parseNativeMobileRelease, PREPARING_MOBILE_RELEASE, type NativeMobileRelease } from './release';

export function loadNativeMobileRelease(publicRoot = resolve(process.cwd(), 'public')): NativeMobileRelease {
  let release: NativeMobileRelease | null;
  try {
    const manifest = join(publicRoot, 'releases', 'apocrypha-mobile', 'manifest.json');
    if (statSync(manifest).size > 16_384) return PREPARING_MOBILE_RELEASE;
    release = parseNativeMobileRelease(JSON.parse(readFileSync(manifest, 'utf8')));
  } catch {
    return PREPARING_MOBILE_RELEASE;
  }
  if (!release) return PREPARING_MOBILE_RELEASE;
  const artifact = release.android.artifact;
  if (release.android.state !== 'ready' || !artifact) return release;
  try {
    const downloads = realpathSync(join(publicRoot, 'downloads'));
    const apk = realpathSync(join(downloads, basename(artifact.href)));
    const rel = relative(downloads, apk);
    if (!rel || isAbsolute(rel) || rel.startsWith(`..${sep}`) || rel === '..' || resolve(downloads, rel) !== apk) throw new Error('ARTIFACT_PATH');
    if (!statSync(apk).isFile() || statSync(apk).size !== artifact.bytes) throw new Error('ARTIFACT_BYTES');
    const digest = createHash('sha256').update(readFileSync(apk)).digest('hex');
    if (digest !== artifact.sha256) throw new Error('ARTIFACT_DIGEST');
    const sidecar = `${apk}.sha256`;
    if (statSync(sidecar).size > 512) throw new Error('ARTIFACT_SIDECAR');
    if (readFileSync(sidecar, 'utf8').trim() !== `${digest}  ${basename(apk)}`) throw new Error('ARTIFACT_SIDECAR');
    return release;
  } catch {
    return {
      ...release,
      android: { ...release.android, state: 'preparing', artifact: null },
    };
  }
}
