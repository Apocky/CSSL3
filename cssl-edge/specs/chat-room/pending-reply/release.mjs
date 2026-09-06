import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// § authority: prepare=local; build=explicit; network=status|deploy; mutation=deploy only.
const HERE = path.dirname(fileURLToPath(import.meta.url));
const CLI = 'C:/Users/Apocky/AppData/Roaming/npm/node_modules/vercel/dist/vc.js';
const GIT = 'C:/Program Files/Git/cmd/git.exe';
const OWNED = ["components/brain/BrainExperience.tsx","lib/brain/mini-brain.ts","tests/e2e/brain.spec.ts","tests/lib/mini-brain-writer.test.ts"];
const hash = (b, algorithm = 'sha256') => createHash(algorithm).update(b).digest('hex');
const json = x => JSON.stringify(x, null, 2) + '\n';
const check = (ok, message) => { if (!ok) throw new Error(message); };
const print = x => process.stdout.write(json(x));
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const stamp = () => new Date().toISOString();
function safe(file) {
  check(typeof file === 'string' && !file.includes('\\') && !file.includes(':') && !file.includes('\0') &&
    !file.startsWith('/') && file.split('/').every(x => x && x !== '.' && x !== '..'), 'Unsafe source path.');
  return file;
}
function inside(root, target) {
  const relative = path.relative(root, target);
  return relative !== '' && !relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative);
}
async function read(root, file) {
  const target = path.join(root, ...safe(file).split('/'));
  check(inside(await fs.realpath(root), await fs.realpath(target)), 'Source path escaped its root: ' + file);
  const stat = await fs.lstat(target);
  check(stat.isFile() && !stat.isSymbolicLink(), 'Source must be a regular file: ' + file);
  return fs.readFile(target);
}
function pin(file, data) { return { file, bytes: data.length, sha1: hash(data, 'sha1'), sha256: hash(data) }; }
function verify(data, expected, file) {
  const actual = pin(file, data);
  check(['bytes', 'sha1', 'sha256'].every(k => actual[k] === expected[k]), 'Source pin changed: ' + file);
}
function catalogue(rows) {
  const result = new Map(), folded = new Set();
  for (const row of rows) {
    safe(row.file);
    check(!folded.has(row.file.toLowerCase()), 'Duplicate or case-colliding source path.');
    folded.add(row.file.toLowerCase()); result.set(row.file, row);
  }
  return result;
}
function exactRefs(want, actual) {
  const a = catalogue(want), b = catalogue(actual);
  check(a.size === b.size && [...a].every(([file, row]) => b.get(file)?.sha === row.sha), 'Deployment source references differ.');
}
async function exclusive(file, data) {
  await fs.mkdir(path.dirname(file), { recursive: true });
  await fs.writeFile(file, data, { flag: 'wx' });
}
async function absent(file) {
  try { await fs.access(file); } catch (e) { if (e.code === 'ENOENT') return; throw e; }
  throw new Error('Attempt already recorded; inspect its result before another action.');
}
function options() {
  const out = {}, booleans = new Set(['prepare', 'build', 'status', 'deploy', 'promote']);
  const values = new Set(['stage', 'manifest-sha256', 'git-sha', 'qa-receipt', 'qa-sha256']);
  const args = process.argv.slice(2);
  while (args.length) {
    const key = args.shift()?.replace(/^--/, '');
    check((booleans.has(key) || values.has(key)) && !(key in out), 'Unknown or repeated option.');
    out[key] = booleans.has(key) ? true : args.shift();
    check(out[key] && !String(out[key]).startsWith('--'), 'Missing option value.');
  }
  const modes = ['prepare', 'build', 'status', 'deploy'].filter(x => out[x]);
  check(modes.length <= 1 && (!out.promote || out.deploy), 'Choose one mode; promotion requires --deploy.');
  out.mode = modes[0] || 'prepare';
  check(out.mode !== 'prepare' || !out.stage, 'Prepare always creates a new temporary candidate.');
  return out;
}
async function baseline(scope) {
  const raw = await fs.readFile(scope.baseline.envelope), frozen = await fs.readFile(scope.baseline.manifest);
  check(hash(raw) === scope.baseline.envelopeSha256 && hash(frozen) === scope.baseline.manifestSha256, 'Historical release seal changed.');
  const body = JSON.parse(raw), manifest = JSON.parse(frozen);
  check(body.project === scope.project && body.target === 'production', 'Historical project binding changed.');
  check(manifest.sourceFiles.length === 730 && body.files.length === 730, 'Historical source count changed.');
  exactRefs(body.files, manifest.sourceFiles.map(x => ({ file: x.file, sha: x.sha1 })));
  const records = catalogue(manifest.sourceFiles), payloads = new Map();
  for (const row of records.values()) {
    const data = await read(scope.baseline.app, row.file); verify(data, row, row.file); payloads.set(row.file, data);
  }
  check(same(scope.overlays.map(x => x.file), OWNED), 'Only the four reviewed files may be overlaid.');
  let replaced = 0, added = 0;
  for (const row of scope.overlays) {
    const previous = records.get(row.file);
    if (previous) { verify(payloads.get(row.file), row.previous, row.file); replaced++; }
    else { check(row.previous === null, 'Added file unexpectedly has a preimage.'); added++; }
    const data = await read(path.join(scope.repo, scope.appPrefix), row.file);
    verify(data, row.expected, row.file); payloads.set(row.file, data);
  }
  check(replaced === 4 && added === 0 && payloads.size === 730, 'Release denominator changed.');
  const sources = [...payloads].map(([file, data]) => pin(file, data)).sort((a, b) => a.file < b.file ? -1 : a.file > b.file ? 1 : 0);
  return { body, payloads, sources, sourceSetSha256: hash(json(sources)) };
}
async function prepare(scope, scopeSha256, frozen) {
  const stage = await fs.mkdtemp(path.join(os.tmpdir(), 'apocky-pending-reply-'));
  for (const [file, data] of frozen.payloads) await exclusive(path.join(stage, 'app', file), data);
  const manifest = { schema: 'apocky.pending-reply.candidate.v1', createdAt: stamp(), scopeSha256,
    baselineId: scope.baseline.id, project: scope.project, counts: scope.counts,
    sourceSetSha256: frozen.sourceSetSha256, sourceFiles: frozen.sources, state: 'PREPARED_LOCAL_ONLY' };
  const bytes = Buffer.from(json(manifest));
  await exclusive(path.join(stage, 'manifest.json'), bytes);
  print({ state: manifest.state, stage, manifestSha256: hash(bytes), counts: scope.counts, sourceSetSha256: frozen.sourceSetSha256 });
}
async function loadStage(scope, scopeSha256, frozen, opts) {
  check(opts.stage && /^[a-f0-9]{64}$/.test(opts['manifest-sha256'] || ''), 'Supply --stage and its reviewed --manifest-sha256.');
  const stage = path.resolve(opts.stage), real = await fs.realpath(stage), temp = await fs.realpath(os.tmpdir());
  check(inside(temp, real) && path.basename(real).startsWith('apocky-pending-reply-') && real === await fs.realpath(path.dirname(stage)).then(p => path.join(p, path.basename(stage))), 'Candidate must be an owned temporary directory.');
  const bytes = await read(stage, 'manifest.json');
  check(hash(bytes) === opts['manifest-sha256'], 'Candidate manifest seal changed.');
  const manifest = JSON.parse(bytes);
  check(manifest.schema === 'apocky.pending-reply.candidate.v1' && manifest.scopeSha256 === scopeSha256 &&
    manifest.baselineId === scope.baseline.id && manifest.project === scope.project &&
    same(manifest.counts, scope.counts) && same(manifest.sourceFiles, frozen.sources) &&
    manifest.sourceSetSha256 === frozen.sourceSetSha256, 'Candidate no longer matches the reviewed scope.');
  for (const row of frozen.sources) verify(await read(path.join(stage, 'app'), row.file), row, row.file);
  return { stage, manifest, manifestSha256: hash(bytes), ...frozen };
}
async function run(command, args, cwd, timeout = 60000) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd, shell: false, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'],
      env: { ...process.env, CI: '1', NO_COLOR: '1', NEXT_TELEMETRY_DISABLED: '1', VERCEL_TELEMETRY_DISABLED: '1', GIT_TERMINAL_PROMPT: '0' } });
    const parts = []; let size = 0, stopped = false;
    const timer = setTimeout(() => { stopped = true; child.kill(); }, timeout);
    child.stdout.on('data', b => { size += b.length; if (size <= 64 * 1024 * 1024) parts.push(b); else { stopped = true; child.kill(); } });
    child.stderr.on('data', () => {}); // N! credential/error payload → logs.
    child.on('error', () => { clearTimeout(timer); reject(new Error('Required executable unavailable.')); });
    child.on('close', code => { clearTimeout(timer); if (code || stopped) reject(new Error('Command failed or timed out; no credential or response details printed. Complete ordinary CLI authentication if required.')); else resolve(Buffer.concat(parts)); });
  });
}
async function build(scope, prepared) {
  const target = path.join(prepared.stage, 'app', 'node_modules'), dependencies = path.join(scope.repo, scope.appPrefix, 'node_modules');
  await absent(target); await absent(path.join(prepared.stage, 'build-attempt.json'));
  const installed = JSON.parse(await fs.readFile(path.join(dependencies, 'next/package.json'), 'utf8'));
  const expected = JSON.parse(prepared.payloads.get('package.json')).dependencies.next;
  check(installed.version === expected, 'Installed Next version differs from the retained package.');
  await exclusive(path.join(prepared.stage, 'build-attempt.json'), json({ at: stamp(), sourceSetSha256: prepared.sourceSetSha256, command: 'next build; preserved generated sources', node: process.version, next: installed.version }));
  await fs.symlink(dependencies, target, process.platform === 'win32' ? 'junction' : 'dir');
  await run(process.execPath, [path.join(dependencies, 'next/dist/bin/next'), 'build'], path.join(prepared.stage, 'app'), 15 * 60 * 1000);
  for (const row of prepared.sources) verify(await read(path.join(prepared.stage, 'app'), row.file), row, row.file);
  await exclusive(path.join(prepared.stage, 'build-passed.json'), json({ at: stamp(), sourceSetSha256: prepared.sourceSetSha256, sourcesUnchanged: 730, next: installed.version, node: process.version }));
  print({ state: 'LOCAL_NEXT_BUILD_PASSED', stage: prepared.stage, sourcesUnchanged: 730 });
}
async function gitProof(scope, prepared, commit) {
  check(/^[a-f0-9]{40}$/.test(commit || ''), 'Supply the reviewed --git-sha.');
  const git = args => run(GIT, args, scope.repo);
  const text = async args => (await git(args)).toString('utf8').trim();
  check(await text(['rev-parse', 'HEAD']) === commit && await text(['branch', '--show-current']) === scope.branch, 'Local commit or branch differs.');
  check(await text(['remote', 'get-url', 'origin']) === scope.remote, 'Origin differs from the reviewed remote.');
  const rows = [...scope.overlays.map(x => ({ file: x.file, data: prepared.payloads.get(x.file) }))];
  for (const file of ['release.mjs', 'scope.json', 'RELEASE.csl']) rows.push({ file: 'specs/chat-room/pending-reply/' + file, data: await read(HERE, file) });
  const comparisons = [];
  for (const row of rows) {
    const committed = await git(['show', commit + ':' + scope.appPrefix + row.file]);
    const normalized = Buffer.from(row.data.toString('utf8').replaceAll('\r\n', '\n'));
    check(committed.equals(row.data) || (!row.data.includes(0) && committed.equals(normalized)), 'Committed bytes differ: ' + row.file);
    comparisons.push({ file: row.file, sourceSha256: hash(row.data), committedSha256: hash(committed), lineEndingNormalization: !committed.equals(row.data) });
  }
  const remoteLine = await text(['ls-remote', '--exit-code', 'origin', 'refs/heads/' + scope.branch]);
  check(remoteLine === commit + '\trefs/heads/' + scope.branch, 'Reviewed commit has not been pushed to the intended remote ref.');
  const tree = await text(['rev-parse', commit + '^{tree}']);
  check(/^[a-f0-9]{40}$/.test(tree), 'Commit tree is unavailable.');
  return { at: stamp(), commit, tree, remote: scope.remote, ref: 'refs/heads/' + scope.branch, remoteCommit: commit,
    remoteTreeBasis: 'same content-addressed commit; local commit tree verified', comparisons };
}
async function api(scope, endpoint, { method = 'GET', input, digest } = {}) {
  const args = [CLI, 'api', endpoint + (endpoint.includes('?') ? '&' : '?') + 'teamId=' + scope.team, '--scope', scope.scope, '--method', method, '--raw'];
  if (input) args.push('--input', input);
  if (digest) args.push('--header', 'Content-Type: application/octet-stream', '--header', 'x-vercel-digest: ' + digest);
  const result = await run(process.execPath, args, scope.repo);
  try { return result.length ? JSON.parse(result.toString('utf8')) : {}; }
  catch { throw new Error('CLI returned an unexpected response; no response details printed.'); }
}
function flatten(nodes, prefix = '', result = []) {
  check(Array.isArray(nodes), 'Unexpected deployment file tree.');
  for (const node of nodes) {
    const file = prefix ? prefix + '/' + node.name : node.name;
    if (node.children) flatten(node.children, file, result); else result.push({ file, sha: node.uid });
  }
  return result;
}
async function refs(scope, id) {
  check(/^dpl_[A-Za-z0-9]+$/.test(id), 'Invalid deployment ID.');
  return flatten(await api(scope, '/v6/deployments/' + id + '/files')).filter(x => x.file.startsWith('src/')).map(x => ({ file: x.file.slice(4), sha: x.sha }));
}
async function baselineLive(scope, body) {
  const project = await api(scope, '/v9/projects/' + scope.project);
  check(project.id === scope.project && project.targets?.production?.id === scope.baseline.id, 'Production moved beyond the historical pin; stop and reconcile.');
  for (const alias of scope.aliases) {
    const value = await api(scope, '/v4/aliases/' + alias);
    check(value.alias === alias && value.projectId === scope.project && value.deploymentId === scope.baseline.id, 'Production alias changed; stop and reconcile.');
  }
  exactRefs(body.files, await refs(scope, scope.baseline.id));
}
async function candidate(scope, prepared) {
  const receipt = JSON.parse(await read(prepared.stage, 'deployment-created.json'));
  check(receipt.sourceSetSha256 === prepared.sourceSetSha256 && receipt.manifestSha256 === prepared.manifestSha256, 'Candidate receipt binding differs.');
  check(/^dpl_[A-Za-z0-9]+$/.test(receipt.id), 'Candidate ID invalid.');
  const current = await api(scope, '/v13/deployments/' + receipt.id);
  check(current.url === receipt.url && /^[a-zA-Z0-9-]+\.vercel\.app$/.test(current.url), 'Candidate URL differs from the verified deployment.');
  check(current.projectId === scope.project && current.meta?.pendingReplySourceSet === prepared.sourceSetSha256 && current.meta?.pendingReplyManifest === prepared.manifestSha256 && current.meta?.pendingReplyGitCommit === receipt.gitCommit, 'Candidate identity changed.');
  if (current.readyState === 'READY') exactRefs(prepared.sources.map(x => ({ file: x.file, sha: x.sha1 })), await refs(scope, receipt.id));
  return { id: receipt.id, url: receipt.url, readyState: current.readyState, gitCommit: receipt.gitCommit };
}
async function deploy(scope, prepared, opts) {
  const proof = await gitProof(scope, prepared, opts['git-sha']);
  const passed = JSON.parse(await read(prepared.stage, 'build-passed.json'));
  check(passed.sourceSetSha256 === prepared.sourceSetSha256 && passed.sourcesUnchanged === 730, 'Exact candidate build proof required.');
  if (opts.promote) {
    const target = await candidate(scope, prepared);
    check(target.readyState === 'READY' && target.gitCommit === proof.commit, 'Ready candidate and current pushed commit must match.');
    check(opts['qa-receipt'] && /^[a-f0-9]{64}$/.test(opts['qa-sha256'] || ''), 'A reviewed visual QA receipt and SHA256 are required.');
    const bytes = await fs.readFile(opts['qa-receipt']); check(hash(bytes) === opts['qa-sha256'], 'QA receipt changed.');
    const qa = JSON.parse(bytes);
    check(qa.deploymentId === target.id && qa.sourceSetSha256 === prepared.sourceSetSha256 && qa.passed === true &&
      ['desktop', 'mobile', 'online-send', 'retry', 'reconnect', 'history-preserved', 'pending-reply', 'legacy-queue', 'history-before-continuation'].every(k => qa.checks?.[k] === true), 'Candidate visual and interaction acceptance is incomplete.');
    await baselineLive(scope, prepared.body);
    await exclusive(path.join(prepared.stage, 'promotion-attempt.json'), json({ at: stamp(), ...target, proof, qaSha256: hash(bytes) }));
    await run(process.execPath, [CLI, 'promote', target.id, '--scope', scope.scope, '--yes'], scope.repo);
    const live = { ...scope, baseline: { ...scope.baseline, id: target.id } };
    await baselineLive(live, { files: prepared.sources.map(x => ({ file: x.file, sha: x.sha1 })) });
    await exclusive(path.join(prepared.stage, 'promotion-completed.json'), json({ at: stamp(), ...target, sourceSetSha256: prepared.sourceSetSha256, proof, aliases: scope.aliases }));
    print({ state: 'PROMOTED_ALIASES_AND_730_SOURCE_REFS_VERIFIED', ...target }); return;
  }
  await absent(path.join(prepared.stage, 'submission-attempt.json'));
  await baselineLive(scope, prepared.body);
  const body = structuredClone(prepared.body), replacements = new Map(scope.overlays.map(x => [x.file, x.expected.sha1]));
  body.files = body.files.map(x => replacements.has(x.file) ? { ...x, sha: replacements.get(x.file) } : x);
  for (const row of scope.overlays.filter(x => x.previous === null)) body.files.push({ file: row.file, sha: row.expected.sha1 });
  body.autoAssignCustomDomains = false;
  body.meta = { ...body.meta, sourceDeployment: scope.baseline.id, pendingReplyManifest: prepared.manifestSha256, pendingReplySourceSet: prepared.sourceSetSha256, pendingReplyGitCommit: proof.commit };
  exactRefs(prepared.sources.map(x => ({ file: x.file, sha: x.sha1 })), body.files);
  check(same(body.projectSettings, prepared.body.projectSettings) && body.target === 'production', 'Inherited build settings changed.');
  await exclusive(path.join(prepared.stage, 'deployment-body.json'), json(body));
  for (const row of scope.overlays) {
    const data = prepared.payloads.get(row.file);
    check(Buffer.from(data.toString('utf8')).equals(data), 'CLI upload requires exact UTF-8 source bytes.');
    try { JSON.parse(data.toString('utf8')); throw new Error('JSON source would be transformed by CLI input handling.'); } catch (e) { if (!(e instanceof SyntaxError)) throw e; }
  }
  await exclusive(path.join(prepared.stage, 'submission-attempt.json'), json({ at: stamp(), proof, manifestSha256: prepared.manifestSha256, bodySha256: hash(json(body)) }));
  for (const row of scope.overlays) await api(scope, '/v2/files', { method: 'POST', input: path.join(prepared.stage, 'app', row.file), digest: row.expected.sha1 });
  await baselineLive(scope, prepared.body);
  const created = await api(scope, '/v13/deployments', { method: 'POST', input: path.join(prepared.stage, 'deployment-body.json') });
  check(created.projectId === scope.project && /^dpl_[A-Za-z0-9]+$/.test(created.id) && /^[a-zA-Z0-9-]+\.vercel\.app$/.test(created.url), 'Unexpected creation response; inspect submission before retrying.');
  await exclusive(path.join(prepared.stage, 'deployment-created.json'), json({ at: stamp(), id: created.id, url: created.url, gitCommit: proof.commit, manifestSha256: prepared.manifestSha256, sourceSetSha256: prepared.sourceSetSha256 }));
  print({ state: 'CANDIDATE_CREATED_DOMAINS_UNASSIGNED', id: created.id, url: created.url });
}
async function main() {
  const opts = options(), scopeBytes = await fs.readFile(path.join(HERE, 'scope.json'));
  const scope = JSON.parse(scopeBytes), scopeSha256 = hash(scopeBytes), frozen = await baseline(scope);
  if (opts.mode === 'prepare') return prepare(scope, scopeSha256, frozen);
  const prepared = await loadStage(scope, scopeSha256, frozen, opts);
  if (opts.mode === 'build') return build(scope, prepared);
  if (opts.mode === 'status') { print({ state: 'CANDIDATE_OBSERVATION', ...await candidate(scope, prepared) }); return; }
  return deploy(scope, prepared, opts);
}
main().catch(error => { process.stderr.write('Release stopped: ' + error.message + '\n'); process.exitCode = 1; });
