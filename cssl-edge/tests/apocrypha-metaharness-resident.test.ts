import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const script = resolve(
  process.cwd(),
  'scripts/apocrypha-memory-gateway/metaharness-register-resident-task.ps1',
);
const source = readFileSync(script, 'utf8');

assert.match(source, /New-ScheduledTaskAction -Execute \$taskExecutable -Argument \$taskArguments -WorkingDirectory \$root/u);
assert.doesNotMatch(source, /New-ScheduledTaskAction[^\r\n]+powershell/iu);
assert.doesNotMatch(source, /(?:Enable|Start|Register|Unregister)-ScheduledTask[^\r\n]+\$LegacyTaskName/iu);
assert.match(source, /bootstrap-metaharness-capability/u);
assert.match(source, /\[EnvironmentVariableTarget\]::User/u);
assert.match(source, /credential_printed = \$false/u);

const fixture = mkdtempSync(join(tmpdir(), 'metaharness-resident-plan-'));
try {
  const root = join(fixture, 'MetaHarness');
  const sourceRoot = join(root, 'src');
  const scripts = join(root, '.venv', 'Scripts');
  const packages = join(root, '.venv', 'Lib', 'site-packages');
  mkdirSync(sourceRoot, { recursive: true });
  mkdirSync(scripts, { recursive: true });
  mkdirSync(join(packages, 'mcp-1.28.1.dist-info'), { recursive: true });
  const basePython = join(fixture, 'python-home');
  mkdirSync(basePython, { recursive: true });
  writeFileSync(join(root, 'PRIME_DIRECTIVE.md'), 'fixture\n');
  writeFileSync(join(root, 'pyproject.toml'), '[project]\nname="meta-harness"\n');
  writeFileSync(join(scripts, 'meta-harness-mcp.exe'), 'fixture executable');
  writeFileSync(join(scripts, 'python.exe'), 'fixture executable');
  writeFileSync(join(basePython, 'python.exe'), 'fixture executable');
  writeFileSync(join(root, '.venv', 'pyvenv.cfg'), `home = ${basePython}\n`);
  writeFileSync(join(packages, '__editable__.meta_harness-0.1.0.pth'), sourceRoot);

  const federator = join(fixture, 'apocrypha-memory-federator.exe');
  const bundle = join(fixture, 'private', 'metaharness.v1.dpapi');
  const federation = join(fixture, 'federation.json');
  const envFile = join(fixture, '.env.local');
  const compilerRoot = join(process.env.WINDIR ?? 'C:\\Windows', 'Microsoft.NET');
  const compiler64 = join(compilerRoot, 'Framework64', 'v4.0.30319', 'csc.exe');
  const compiler32 = join(compilerRoot, 'Framework', 'v4.0.30319', 'csc.exe');
  const compiler = existsSync(compiler64) ? compiler64 : compiler32;
  const federatorSource = join(fixture, 'federator.cs');
  writeFileSync(federatorSource, [
    'using System;',
    'public static class Federator {',
    '  public static int Main(string[] args) {',
    '    if (args.Length == 2 && args[0] == "bootstrap-metaharness-capability" && args[1] == "--help") {',
    '      Console.WriteLine("Usage: bootstrap-metaharness-capability --output PATH --owner-id OWNER --endpoint URL");',
    '      return 0;',
    '    }',
    '    return 1;',
    '  }',
    '}',
  ].join('\n'));
  const compiled = spawnSync(compiler, ['/nologo', '/target:exe', `/out:${federator}`, federatorSource], {
    encoding: 'utf8', windowsHide: true,
  });
  assert.equal(compiled.status, 0, compiled.stdout + compiled.stderr);
  writeFileSync(federation, JSON.stringify({
    metaharness: {
      endpoint: 'http://127.0.0.1:8765/mcp',
      capability: {
        bundle_path: bundle,
        entropy_label: 'Apocrypha.MetaHarness.observer-capability.v1',
      },
    },
  }));
  const envBefore = [
    `APOCRYPHA_MEMORY_FEDERATOR_EXE=${federator}`,
    `APOCRYPHA_MEMORY_FEDERATOR_CONFIG=${federation}`,
    'APOCRYPHA_MEMORY_OWNER_ID=apocky',
    '',
  ].join('\n');
  writeFileSync(envFile, envBefore);

  const planned = spawnSync('powershell.exe', [
    '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'RemoteSigned',
    '-File', script,
    '-Operation', 'Plan',
    '-TaskName', 'Fixture-MetaHarness-Direct',
    '-MetaHarnessRoot', root,
    '-EnvFile', envFile,
  ], { encoding: 'utf8', windowsHide: true });
  assert.equal(planned.status, 0, planned.stdout + planned.stderr);
  const plan = JSON.parse(planned.stdout) as Record<string, any>;
  assert.equal(plan.schema, 'apocrypha.metaharness-resident.plan.v1');
  assert.equal(plan.legacy_untouched, true);
  assert.equal(plan.action.execute, join(scripts, 'python.exe'));
  assert.deepEqual(plan.action.arguments, ['-m', 'meta_harness.mcp_server']);
  assert.equal(plan.action.serialized_arguments, '-m meta_harness.mcp_server');
  assert.equal(plan.action.working_directory, root);
  assert.deepEqual(plan.trigger, ['user-logon']);
  assert.doesNotMatch(source, /RepetitionInterval/u);
  assert.equal(plan.observer.endpoint, 'http://127.0.0.1:8765/mcp');
  assert.equal(plan.observer.authority, 'none');
  assert.equal(plan.observer.execution_authorized, false);
  assert.equal(plan.credential.included_in_task, false);
  assert.equal(plan.credential.printed, false);
  assert.equal(plan.provenance.base_python, join(basePython, 'python.exe'));
  assert.equal(readFileSync(envFile, 'utf8'), envBefore, 'Plan mode changed the private environment file');
  assert.doesNotMatch(planned.stdout, /Bearer\s+[A-Za-z0-9_-]{43}/u);

  const legacy = spawnSync('powershell.exe', [
    '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'RemoteSigned',
    '-File', script,
    '-Operation', 'Plan',
    '-TaskName', 'Apocky-MetaHarness-MCP',
    '-MetaHarnessRoot', root,
    '-EnvFile', envFile,
  ], { encoding: 'utf8', windowsHide: true });
  assert.notEqual(legacy.status, 0, 'Plan mode admitted the disabled legacy task name');
} finally {
  rmSync(fixture, { recursive: true, force: true });
}

console.log('apocrypha-metaharness-resident.test: OK');
