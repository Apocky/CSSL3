import { createHash } from 'node:crypto';
import { readFileSync, realpathSync, statSync } from 'node:fs';
import { basename, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { parseDesktopRelease, PREPARING_DESKTOP_RELEASE, type DesktopRelease } from './release';

/**
 * Reads the checked-in desktop release and re-verifies the artifact it names.
 *
 * The page is statically generated, so this runs at build time: a manifest that
 * claims a hash the staged installer does not have degrades to "preparing"
 * rather than publishing a download whose checksum is a lie.
 */
export function loadDesktopRelease(publicRoot = resolve(process.cwd(), 'public')): DesktopRelease {
  let release: DesktopRelease | null;
  try {
    const manifest = join(publicRoot, 'releases', 'apocrypha-desktop', 'manifest.json');
    if (statSync(manifest).size > 16_384) return PREPARING_DESKTOP_RELEASE;
    release = parseDesktopRelease(JSON.parse(readFileSync(manifest, 'utf8')));
  } catch {
    return PREPARING_DESKTOP_RELEASE;
  }
  if (!release) return PREPARING_DESKTOP_RELEASE;
  const artifact = release.windows.artifact;
  if (release.windows.state !== 'ready' || !artifact) return release;
  try {
    const downloads = realpathSync(join(publicRoot, 'downloads'));
    const installer = realpathSync(join(downloads, basename(artifact.href)));
    const rel = relative(downloads, installer);
    if (!rel || isAbsolute(rel) || rel.startsWith(`..${sep}`) || rel === '..' || resolve(downloads, rel) !== installer) {
      throw new Error('ARTIFACT_PATH');
    }
    if (!statSync(installer).isFile() || statSync(installer).size !== artifact.bytes) throw new Error('ARTIFACT_BYTES');
    const digest = createHash('sha256').update(readFileSync(installer)).digest('hex');
    if (digest !== artifact.sha256) throw new Error('ARTIFACT_DIGEST');
    const sidecar = `${installer}.sha256`;
    if (statSync(sidecar).size > 512) throw new Error('ARTIFACT_SIDECAR');
    if (readFileSync(sidecar, 'utf8').trim() !== `${digest}  ${basename(installer)}`) throw new Error('ARTIFACT_SIDECAR');
    return release;
  } catch {
    return {
      ...release,
      windows: { ...release.windows, state: 'preparing', artifact: null },
    };
  }
}
