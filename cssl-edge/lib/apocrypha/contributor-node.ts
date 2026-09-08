/**
 * Public-safe release contract for the Apocrypha Mycelial Contributor Node.
 *
 * This module intentionally describes availability; it does not start a
 * worker, fetch an artifact, read secrets, or grant compute authority.  The
 * candidate state is fail-closed until every release gate is represented by
 * a signed artifact and an independently verified native runtime.
 */

export type ContributorNodeReleaseState = 'NOT_DEPLOYED' | 'NOT_DEPLOYABLE' | 'PARTIAL' | 'READY';
export type ContributorNodePlatformState = 'NOT_DEPLOYED' | 'READY';
export type ContributorNodeTarget =
  | 'windows-x64'
  | 'macos-arm64'
  | 'linux-x64'
  | 'android'
  | 'ios';

export interface ContributorNodeArtifact {
  readonly href: string;
  readonly filename: string;
  readonly sha256: string;
  readonly detached_signature: string;
  readonly signing_key_id: string;
  readonly bytes: number;
  readonly provenance: string;
}

export interface ContributorNodePlatform {
  readonly target: ContributorNodeTarget;
  readonly category: 'desktop' | 'mobile';
  readonly label: string;
  readonly state: ContributorNodePlatformState;
  readonly artifact: ContributorNodeArtifact | null;
  readonly runtime_scope: 'desktop_opt_in' | 'mobile_foreground_opt_in';
  readonly availability_note: string;
}

export interface ContributorNodeManifest {
  readonly schema_version: 'apocrypha.mycelial-contributor-node.v1';
  readonly product: 'Apocrypha Mycelial Contributor Node';
  readonly version: string;
  readonly release_state: ContributorNodeReleaseState;
  readonly release_gate: 'CLOSED' | 'OPEN';
  readonly summary: string;
  readonly contract: {
    readonly status: 'candidate_only' | 'native_release';
    readonly transport: 'local_only' | 'external_adapter_only' | 'signed_native_transport';
    readonly network: 'disabled' | 'signed_outbound_only';
    readonly inbound_listener: false;
    readonly production_eligible: boolean;
    readonly mobile_background_compute: false;
  };
  readonly platforms: readonly ContributorNodePlatform[];
  readonly execution: {
    readonly default_mode: 'paused';
    readonly opt_in_required: true;
    readonly auto_start: false;
    readonly privilege_escalation: false;
    readonly sandbox_required: true;
    readonly user_controls: readonly ['pause', 'revoke', 'uninstall'];
  };
  readonly resource_policy: {
    readonly status: 'proposed_default' | 'enforced';
    readonly cpu_percent_max: 25;
    readonly memory_mb_max: 2048;
    readonly disk_mb_max: 4096;
    readonly egress_mbps_max: 5;
    readonly schedule: 'user_selected_only';
    readonly mobile_requires_charging: true;
    readonly metered_network: 'blocked_by_default';
  };
  readonly privacy: {
    readonly raw_conversation: false;
    readonly raw_memory: false;
    readonly vault_payloads: false;
    readonly credentials: false;
    readonly public_capsules_only: true;
    readonly telemetry: 'disabled_by_default';
  };
  readonly controls: {
    readonly pause: 'immediate_local_stop';
    readonly revoke: 'sever_and_rotate_node_identity';
    readonly uninstall: 'remove_binary_retain_state';
    readonly purge: 'explicit_owner_confirmation';
  };
  readonly artifact_policy: {
    readonly sha256: 'required';
    readonly detached_signature: 'required';
    readonly pinned_signer: 'required';
    readonly reproducible_build: 'required';
    readonly malware_scan: 'required';
    readonly install_smoke: 'required';
    readonly rollback: 'required';
    readonly unsigned_candidate_download: 'blocked';
  };
}

const TARGETS: readonly ContributorNodeTarget[] = [
  'windows-x64',
  'macos-arm64',
  'linux-x64',
  'android',
  'ios',
];

/** Public metadata only; there is deliberately no executable artifact yet. */
export const CONTRIBUTOR_NODE_MANIFEST: ContributorNodeManifest = {
  schema_version: 'apocrypha.mycelial-contributor-node.v1',
  product: 'Apocrypha Mycelial Contributor Node',
  version: '0.1.0-candidate',
  release_state: 'NOT_DEPLOYABLE',
  release_gate: 'CLOSED',
  summary:
    'A bounded local contributor-node candidate exists, but its production transport and signed public artifact are not verified. No download is enabled.',
  contract: {
    status: 'candidate_only',
    transport: 'local_only',
    network: 'disabled',
    inbound_listener: false,
    production_eligible: false,
    mobile_background_compute: false,
  },
  platforms: [
    {
      target: 'windows-x64',
      category: 'desktop',
      label: 'Windows x64',
      state: 'NOT_DEPLOYED',
      artifact: null,
      runtime_scope: 'desktop_opt_in',
      availability_note: 'Signed native package and isolated install smoke are not yet released.',
    },
    {
      target: 'macos-arm64',
      category: 'desktop',
      label: 'macOS arm64',
      state: 'NOT_DEPLOYED',
      artifact: null,
      runtime_scope: 'desktop_opt_in',
      availability_note: 'Signed native package and isolated install smoke are not yet released.',
    },
    {
      target: 'linux-x64',
      category: 'desktop',
      label: 'Linux x64',
      state: 'NOT_DEPLOYED',
      artifact: null,
      runtime_scope: 'desktop_opt_in',
      availability_note: 'Signed native package and isolated install smoke are not yet released.',
    },
    {
      target: 'android',
      category: 'mobile',
      label: 'Android',
      state: 'NOT_DEPLOYED',
      artifact: null,
      runtime_scope: 'mobile_foreground_opt_in',
      availability_note: 'No contributor-node package; mobile background execution remains disabled.',
    },
    {
      target: 'ios',
      category: 'mobile',
      label: 'iPhone / iPad',
      state: 'NOT_DEPLOYED',
      artifact: null,
      runtime_scope: 'mobile_foreground_opt_in',
      availability_note: 'No contributor-node package; mobile background execution remains disabled.',
    },
  ],
  execution: {
    default_mode: 'paused',
    opt_in_required: true,
    auto_start: false,
    privilege_escalation: false,
    sandbox_required: true,
    user_controls: ['pause', 'revoke', 'uninstall'],
  },
  resource_policy: {
    status: 'proposed_default',
    cpu_percent_max: 25,
    memory_mb_max: 2048,
    disk_mb_max: 4096,
    egress_mbps_max: 5,
    schedule: 'user_selected_only',
    mobile_requires_charging: true,
    metered_network: 'blocked_by_default',
  },
  privacy: {
    raw_conversation: false,
    raw_memory: false,
    vault_payloads: false,
    credentials: false,
    public_capsules_only: true,
    telemetry: 'disabled_by_default',
  },
  controls: {
    pause: 'immediate_local_stop',
    revoke: 'sever_and_rotate_node_identity',
    uninstall: 'remove_binary_retain_state',
    purge: 'explicit_owner_confirmation',
  },
  artifact_policy: {
    sha256: 'required',
    detached_signature: 'required',
    pinned_signer: 'required',
    reproducible_build: 'required',
    malware_scan: 'required',
    install_smoke: 'required',
    rollback: 'required',
    unsigned_candidate_download: 'blocked',
  },
};

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function exactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const keys = [...expected].sort();
  return actual.length === keys.length && actual.every((key, index) => key === keys[index]);
}

function boundedText(value: unknown, maximum = 512): value is string {
  return typeof value === 'string' && value.length > 0 && value.length <= maximum;
}

function lowerSha256(value: unknown): value is string {
  return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
}

function artifact(value: unknown): value is ContributorNodeArtifact {
  const source = asRecord(value);
  if (
    !source ||
    !exactKeys(source, ['href', 'filename', 'sha256', 'detached_signature', 'signing_key_id', 'bytes', 'provenance']) ||
    typeof source.href !== 'string' ||
    !/^\/downloads\/apocrypha-node\/[A-Za-z0-9][A-Za-z0-9._-]*$/.test(source.href) ||
    typeof source.filename !== 'string' ||
    !/^[A-Za-z0-9][A-Za-z0-9._-]{0,180}$/.test(source.filename) ||
    !lowerSha256(source.sha256) ||
    !boundedText(source.detached_signature, 4096) ||
    !/^[A-Za-z0-9._:-]{3,128}$/.test(source.signing_key_id as string) ||
    !Number.isSafeInteger(source.bytes) ||
    Number(source.bytes) <= 0 ||
    Number(source.bytes) > 2_000_000_000 ||
    !boundedText(source.provenance, 512)
  ) return false;
  return true;
}

function parsePlatform(value: unknown): ContributorNodePlatform | null {
  const source = asRecord(value);
  if (
    !source ||
    !exactKeys(source, ['target', 'category', 'label', 'state', 'artifact', 'runtime_scope', 'availability_note']) ||
    typeof source.target !== 'string' ||
    !TARGETS.includes(source.target as ContributorNodeTarget) ||
    typeof source.category !== 'string' ||
    !['desktop', 'mobile'].includes(source.category) ||
    !boundedText(source.label, 128) ||
    typeof source.state !== 'string' ||
    !['NOT_DEPLOYED', 'READY'].includes(source.state) ||
    !boundedText(source.availability_note, 512) ||
    typeof source.runtime_scope !== 'string' ||
    !['desktop_opt_in', 'mobile_foreground_opt_in'].includes(source.runtime_scope)
  ) return null;
  const target = source.target as ContributorNodeTarget;
  const category = source.category as ContributorNodePlatform['category'];
  const runtimeScope = source.runtime_scope as ContributorNodePlatform['runtime_scope'];
  if (category === 'desktop' && !['windows-x64', 'macos-arm64', 'linux-x64'].includes(target)) return null;
  if (category === 'mobile' && !['android', 'ios'].includes(target)) return null;
  if (category === 'desktop' && runtimeScope !== 'desktop_opt_in') return null;
  if (category === 'mobile' && runtimeScope !== 'mobile_foreground_opt_in') return null;
  if (source.state === 'NOT_DEPLOYED' && source.artifact !== null) return null;
  if (source.state === 'READY' && !artifact(source.artifact)) return null;
  return source as unknown as ContributorNodePlatform;
}

/**
 * Validate a release manifest before a client could ever treat it as a
 * download.  The relation between state, artifact, signer, and native
 * transport is intentional: a hash or signature alone cannot promote a
 * candidate into a production release.
 */
export function parseContributorNodeManifest(value: unknown): ContributorNodeManifest | null {
  const source = asRecord(value);
  if (
    !source ||
    !exactKeys(source, [
      'schema_version',
      'product',
      'version',
      'release_state',
      'release_gate',
      'summary',
      'contract',
      'platforms',
      'execution',
      'resource_policy',
      'privacy',
      'controls',
      'artifact_policy',
    ]) ||
    source.schema_version !== 'apocrypha.mycelial-contributor-node.v1' ||
    source.product !== 'Apocrypha Mycelial Contributor Node' ||
    !/^[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?$/.test(String(source.version)) ||
    !boundedText(source.summary, 1024) ||
    !Array.isArray(source.platforms) ||
    source.platforms.length !== TARGETS.length
  ) return null;

  const contract = asRecord(source.contract);
  if (
    !contract ||
    !exactKeys(contract, ['status', 'transport', 'network', 'inbound_listener', 'production_eligible', 'mobile_background_compute']) ||
    !['candidate_only', 'native_release'].includes(String(contract.status)) ||
    !['local_only', 'external_adapter_only', 'signed_native_transport'].includes(String(contract.transport)) ||
    !['disabled', 'signed_outbound_only'].includes(String(contract.network)) ||
    contract.inbound_listener !== false ||
    typeof contract.production_eligible !== 'boolean' ||
    contract.mobile_background_compute !== false
  ) return null;

  const execution = asRecord(source.execution);
  if (
    !execution ||
    !exactKeys(execution, ['default_mode', 'opt_in_required', 'auto_start', 'privilege_escalation', 'sandbox_required', 'user_controls']) ||
    execution.default_mode !== 'paused' ||
    execution.opt_in_required !== true ||
    execution.auto_start !== false ||
    execution.privilege_escalation !== false ||
    execution.sandbox_required !== true ||
    !Array.isArray(execution.user_controls) ||
    execution.user_controls.length !== 3 ||
    execution.user_controls.some((item, index) => item !== (['pause', 'revoke', 'uninstall'] as const)[index])
  ) return null;

  const resource = asRecord(source.resource_policy);
  if (
    !resource ||
    !exactKeys(resource, [
      'status',
      'cpu_percent_max',
      'memory_mb_max',
      'disk_mb_max',
      'egress_mbps_max',
      'schedule',
      'mobile_requires_charging',
      'metered_network',
    ]) ||
    !['proposed_default', 'enforced'].includes(String(resource.status)) ||
    resource.cpu_percent_max !== 25 ||
    resource.memory_mb_max !== 2048 ||
    resource.disk_mb_max !== 4096 ||
    resource.egress_mbps_max !== 5 ||
    resource.schedule !== 'user_selected_only' ||
    resource.mobile_requires_charging !== true ||
    resource.metered_network !== 'blocked_by_default'
  ) return null;

  const privacy = asRecord(source.privacy);
  if (
    !privacy ||
    !exactKeys(privacy, ['raw_conversation', 'raw_memory', 'vault_payloads', 'credentials', 'public_capsules_only', 'telemetry']) ||
    privacy.raw_conversation !== false ||
    privacy.raw_memory !== false ||
    privacy.vault_payloads !== false ||
    privacy.credentials !== false ||
    privacy.public_capsules_only !== true ||
    privacy.telemetry !== 'disabled_by_default'
  ) return null;

  const controls = asRecord(source.controls);
  if (
    !controls ||
    !exactKeys(controls, ['pause', 'revoke', 'uninstall', 'purge']) ||
    controls.pause !== 'immediate_local_stop' ||
    controls.revoke !== 'sever_and_rotate_node_identity' ||
    controls.uninstall !== 'remove_binary_retain_state' ||
    controls.purge !== 'explicit_owner_confirmation'
  ) return null;

  const artifactPolicy = asRecord(source.artifact_policy);
  if (
    !artifactPolicy ||
    !exactKeys(artifactPolicy, [
      'sha256',
      'detached_signature',
      'pinned_signer',
      'reproducible_build',
      'malware_scan',
      'install_smoke',
      'rollback',
      'unsigned_candidate_download',
    ]) ||
    artifactPolicy.sha256 !== 'required' ||
    artifactPolicy.detached_signature !== 'required' ||
    artifactPolicy.pinned_signer !== 'required' ||
    artifactPolicy.reproducible_build !== 'required' ||
    artifactPolicy.malware_scan !== 'required' ||
    artifactPolicy.install_smoke !== 'required' ||
    artifactPolicy.rollback !== 'required' ||
    artifactPolicy.unsigned_candidate_download !== 'blocked'
  ) return null;

  const platforms = source.platforms.map(parsePlatform);
  if (platforms.some((item): item is null => item === null)) return null;
  const parsedPlatforms = platforms as ContributorNodePlatform[];
  const targets = parsedPlatforms.map((item) => item.target);
  if (new Set(targets).size !== TARGETS.length || TARGETS.some((target) => !targets.includes(target))) return null;
  const readyCount = parsedPlatforms.filter((item) => item.state === 'READY').length;
  const expectedState: ContributorNodeReleaseState =
    readyCount === 0
      ? source.release_state === 'NOT_DEPLOYABLE' ? 'NOT_DEPLOYABLE' : 'NOT_DEPLOYED'
      : readyCount === parsedPlatforms.length ? 'READY' : 'PARTIAL';
  if (source.release_state !== expectedState) return null;
  if (source.release_gate !== (readyCount === 0 ? 'CLOSED' : 'OPEN')) return null;
  if (readyCount === 0) {
    if (contract.status !== 'candidate_only'
      || !['local_only', 'external_adapter_only'].includes(String(contract.transport))
      || contract.production_eligible !== false
      || (source.release_state === 'NOT_DEPLOYABLE' && (contract.transport !== 'local_only' || contract.network !== 'disabled'))) return null;
    if (resource.status !== 'proposed_default') return null;
  } else {
    if (contract.status !== 'native_release' || contract.transport !== 'signed_native_transport' || contract.production_eligible !== true) return null;
    if (resource.status !== 'enforced') return null;
  }
  return source as unknown as ContributorNodeManifest;
}
