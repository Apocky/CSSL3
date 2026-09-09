import {
  createPrivateKey,
  createPublicKey,
  sign as cryptoSign,
  type KeyObject,
} from 'node:crypto';

import type { ContributorAtomicRouteController } from './contributor-http';
import {
  CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA,
  CONTRIBUTOR_ENROLLMENT_SCHEMA,
  CONTRIBUTOR_LEASE_DISPATCH_SCHEMA,
  CONTRIBUTOR_RESULT_RECEIPT_SCHEMA,
  CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA,
  ContributorTransportError,
  contributorPayloadHash,
  parseContributorLeasePollRequest,
  parseContributorResultSubmission,
  publicKeyFromSpkiB64,
  verifyContributorLeaseIssueRequest,
  verifyContributorLeasePollRequest,
  verifyEnrollmentRequest,
  verifyRevokeRequest,
  verifyResultSubmission,
  type ContributorNodeRecord,
  type EnrollmentReceipt,
  type EnrollmentReceiptPayload,
  type EnrollmentRequest,
  type LeaseDispatch,
  type LeaseDispatchPayload,
  type LeaseIssueRequest,
  type LeasePollRequest,
  type ResultReceipt,
  type ResultReceiptPayload,
  type ResultSubmission,
  type RevokeReceipt,
  type RevokeReceiptPayload,
  type RevokeRequest,
} from './contributor-transport';
import {
  createSupabaseContributorTransportStore,
  type ContributorTransportAtomicEnrollmentInput,
  type ContributorTransportAtomicLeaseInput,
  type ContributorTransportAtomicResultInput,
  type ContributorTransportAtomicRevokeInput,
  type SupabaseContributorTransportStore,
} from './contributor-transport-supabase';
import {
  canonicalJson,
  signLease,
  type LeaseInput,
} from '../../contributor-node/src/runtime';

/**
 * Production controller binding for the operation-level Supabase RPC seam.
 *
 * This module is intentionally inert unless all explicitly-owned boundaries
 * are present: rate limiting is supplied by the route, controller signing is
 * supplied by dedicated controller variables, and persistence is the
 * server-only Supabase service-role adapter.  It never creates, stores, or
 * derives a credential and never selects the in-memory store.
 */

export const CONTRIBUTOR_CONTROLLER_ENV = {
  keyId: 'APOCRYPHA_CONTRIBUTOR_CONTROLLER_KEY_ID',
  privateKeyPem: 'APOCRYPHA_CONTRIBUTOR_CONTROLLER_PRIVATE_KEY_PEM',
  operatorKeyId: 'APOCRYPHA_CONTRIBUTOR_OPERATOR_KEY_ID',
  operatorPublicKeyPem: 'APOCRYPHA_CONTRIBUTOR_OPERATOR_PUBLIC_KEY_PEM',
} as const;

export interface ProductionContributorAtomicControllerOptions {
  /** The route must provide a real external/global limiter before binding. */
  readonly rateLimiterConfigured: boolean;
  /** Test seam; production resolution uses the server-only Supabase factory. */
  readonly store?: AtomicContributorStore;
  readonly now?: () => number;
}

/** Narrow structural store surface used by preparation and atomic operations. */
export type AtomicContributorStore = Pick<
  SupabaseContributorTransportStore,
  | 'atomicRpcCapable'
  | 'transactional'
  | 'atomicEnroll'
  | 'atomicIssueLease'
  | 'atomicAcceptResult'
  | 'atomicRevoke'
  | 'getNode'
  | 'getLeaseByDispatch'
>;

const IDENTIFIER = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const KEY_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{2,127}$/;

function envKeyId(name: string): string | null {
  const value = process.env[name];
  return value && KEY_ID.test(value) ? value : null;
}

function envPrivateKey(name: string): KeyObject | null {
  const pem = process.env[name];
  if (!pem || pem.length > 16_384) return null;
  try {
    const key = createPrivateKey(pem);
    return key.type === 'private' && key.asymmetricKeyType === 'ed25519' ? key : null;
  } catch {
    return null;
  }
}

function envPublicKey(name: string): KeyObject | null {
  const pem = process.env[name];
  if (!pem || pem.length > 16_384) return null;
  try {
    const key = createPublicKey(pem);
    return key.type === 'public' && key.asymmetricKeyType === 'ed25519' ? key : null;
  } catch {
    return null;
  }
}

function controllerSignature(payload: unknown, privateKey: KeyObject): string {
  try {
    return cryptoSign(null, Buffer.from(canonicalJson(payload), 'utf8'), privateKey).toString('base64url');
  } catch {
    throw new ContributorTransportError('TRANSPORT_SIGNATURE_INVALID', 'controller envelope signing failed');
  }
}

function enrollmentPayload(request: EnrollmentRequest) {
  return {
    schema_version: CONTRIBUTOR_ENROLLMENT_SCHEMA,
    request_id: request.request_id,
    node_id: request.node_id,
    node_key_id: request.node_key_id,
    node_public_key_spki_b64: request.node_public_key_spki_b64,
    platform: request.platform,
    capabilities: [...request.capabilities],
    consent_revision: request.consent_revision,
    issued_at: request.issued_at,
    expires_at: request.expires_at,
  };
}
function leaseRequestPayload(request: LeaseIssueRequest) {
  return {
    schema_version: request.schema_version,
    request_id: request.request_id,
    idempotency_key: request.idempotency_key,
    node_id: request.node_id,
    issued_at: request.issued_at,
    expires_at: request.expires_at,
    attempt: request.attempt,
    task: request.task,
  };
}

function leasePollRequestPayload(request: LeasePollRequest) {
  return {
    schema_version: request.schema_version,
    request_id: request.request_id,
    idempotency_key: request.idempotency_key,
    node_id: request.node_id,
    node_key_id: request.node_key_id,
    issued_at: request.issued_at,
    expires_at: request.expires_at,
    attempt: request.attempt,
    task: request.task,
  };
}

function resultSubmissionPayload(submission: ResultSubmission) {
  return {
    schema_version: submission.schema_version,
    dispatch_id: submission.dispatch_id,
    request_id: submission.request_id,
    idempotency_key: submission.idempotency_key,
    node_id: submission.node_id,
    result: submission.result,
  };
}

function revokePayload(request: RevokeRequest) {
  return {
    schema_version: request.schema_version,
    request_id: request.request_id,
    node_id: request.node_id,
    reason: request.reason,
    issued_at: request.issued_at,
    expires_at: request.expires_at,
    operator_key_id: request.operator_key_id,
  };
}

function activeNodeFromEnrollment(
  request: EnrollmentRequest,
  existing: ContributorNodeRecord | null,
  revision: number,
  enrolledAt: number,
): ContributorNodeRecord {
  return {
    node_id: request.node_id,
    node_key_id: request.node_key_id,
    node_public_key_spki_b64: request.node_public_key_spki_b64,
    platform: request.platform,
    capabilities: [...request.capabilities],
    status: 'active',
    revision,
    enrolled_at: existing?.enrolled_at ?? enrolledAt,
    revoked_at: null,
    revoke_reason: null,
  };
}

function requireExistingNode(
  value: ContributorNodeRecord | null,
): ContributorNodeRecord {
  if (!value) throw new ContributorTransportError('TRANSPORT_NODE_NOT_ENROLLED');
  return value;
}

function requireNodeIdentity(
  node: ContributorNodeRecord,
  submission: ResultSubmission,
  dispatch: LeaseDispatch,
): void {
  if (node.status === 'revoked') throw new ContributorTransportError('TRANSPORT_NODE_REVOKED');
  if (dispatch.node_id !== submission.node_id
    || dispatch.request_id !== submission.request_id
    || dispatch.idempotency_key !== submission.idempotency_key
    || submission.result.lease_id !== dispatch.lease.lease_id
    || submission.result.attempt !== dispatch.lease.attempt) {
    throw new ContributorTransportError('TRANSPORT_RESULT_INVALID');
  }
  if (submission.result.started_at + 30_000 < dispatch.lease.issued_at
    || submission.result.finished_at > dispatch.lease.expires_at + 30_000) {
    throw new ContributorTransportError('TRANSPORT_RESULT_INVALID');
  }
  if (node.node_id !== submission.node_id || !IDENTIFIER.test(node.node_id)) {
    throw new ContributorTransportError('TRANSPORT_NODE_KEY_MISMATCH');
  }
}

function createPrepare(
  store: AtomicContributorStore,
  controllerKeyId: string,
  controllerPrivateKey: KeyObject,
  operatorKeyId: string | null,
  operatorPublicKey: KeyObject | null,
  now: () => number,
): ContributorAtomicRouteController['prepare'] {
  return {
    async enrollment(value: unknown): Promise<ContributorTransportAtomicEnrollmentInput> {
      const request = verifyEnrollmentRequest(value, now());
      const requestHash = contributorPayloadHash(enrollmentPayload(request));
      const existing = await store.getNode(request.node_id);
      if (existing?.status === 'revoked') throw new ContributorTransportError('TRANSPORT_NODE_REVOKED');
      if (existing && (existing.node_key_id !== request.node_key_id
        || existing.node_public_key_spki_b64 !== request.node_public_key_spki_b64)) {
        throw new ContributorTransportError('TRANSPORT_NODE_KEY_MISMATCH');
      }
      const revision = (existing?.revision ?? 0) + 1;
      const issuedAt = now();
      const receiptPayload: EnrollmentReceiptPayload = {
        schema_version: CONTRIBUTOR_ENROLLMENT_RECEIPT_SCHEMA,
        request_id: request.request_id,
        enrollment_id: `enr-${requestHash.slice(0, 48)}`,
        node_id: request.node_id,
        node_key_id: request.node_key_id,
        controller_key_id: controllerKeyId,
        status: 'active',
        revision,
        request_hash: requestHash,
        issued_at: issuedAt,
        expires_at: request.expires_at,
      };
      const receipt: EnrollmentReceipt = {
        ...receiptPayload,
        signature_b64: controllerSignature(receiptPayload, controllerPrivateKey),
      };
      return {
        requestId: request.request_id,
        requestHash,
        node: activeNodeFromEnrollment(request, existing, revision, issuedAt),
        receipt,
      };
    },

    async lease(value: unknown): Promise<ContributorTransportAtomicLeaseInput> {
      const request = verifyContributorLeaseIssueRequest(value, now());
      const requestHash = contributorPayloadHash(leaseRequestPayload(request));
      const leaseId = `lease-${requestHash.slice(0, 56)}`;
      let lease;
      try {
        lease = signLease({
          schema_version: 'apocrypha.contributor.lease.v1',
          key_id: controllerKeyId,
          lease_id: leaseId,
          node_id: request.node_id,
          issued_at: request.issued_at,
          expires_at: request.expires_at,
          attempt: request.attempt,
          task: request.task,
        } satisfies LeaseInput, controllerPrivateKey);
      } catch {
        throw new ContributorTransportError('TRANSPORT_LEASE_INVALID', 'controller lease signing failed');
      }
      const dispatchPayload: LeaseDispatchPayload = {
        schema_version: CONTRIBUTOR_LEASE_DISPATCH_SCHEMA,
        dispatch_id: `dispatch-${requestHash.slice(0, 48)}`,
        request_id: request.request_id,
        idempotency_key: request.idempotency_key,
        node_id: request.node_id,
        lease,
      };
      const dispatch: LeaseDispatch = {
        ...dispatchPayload,
        signature_b64: controllerSignature(dispatchPayload, controllerPrivateKey),
      };
      return {
        nodeId: request.node_id,
        idempotencyKey: request.idempotency_key,
        requestHash,
        dispatch,
      };
    },

    async poll(value: unknown): Promise<ContributorTransportAtomicLeaseInput> {
      const pollRequest = parseContributorLeasePollRequest(value);
      const node = requireExistingNode(await store.getNode(pollRequest.node_id));
      if (node.status === 'revoked') throw new ContributorTransportError('TRANSPORT_NODE_REVOKED');
      if (node.node_key_id !== pollRequest.node_key_id) {
        throw new ContributorTransportError('TRANSPORT_NODE_KEY_MISMATCH');
      }
      verifyContributorLeasePollRequest(
        pollRequest,
        publicKeyFromSpkiB64(node.node_public_key_spki_b64),
        node.node_id,
        node.node_key_id,
        now(),
      );
      const request: LeaseIssueRequest = {
        schema_version: 'apocrypha.contributor.lease-request.v1',
        request_id: pollRequest.request_id,
        idempotency_key: pollRequest.idempotency_key,
        node_id: pollRequest.node_id,
        issued_at: pollRequest.issued_at,
        expires_at: pollRequest.expires_at,
        attempt: pollRequest.attempt,
        task: pollRequest.task,
      };
      verifyContributorLeaseIssueRequest(request, now());
      const requestHash = contributorPayloadHash(leasePollRequestPayload(pollRequest));
      const leaseId = `lease-${requestHash.slice(0, 56)}`;
      let lease;
      try {
        lease = signLease({
          schema_version: 'apocrypha.contributor.lease.v1',
          key_id: controllerKeyId,
          lease_id: leaseId,
          node_id: request.node_id,
          issued_at: request.issued_at,
          expires_at: request.expires_at,
          attempt: request.attempt,
          task: request.task,
        } satisfies LeaseInput, controllerPrivateKey);
      } catch {
        throw new ContributorTransportError('TRANSPORT_LEASE_INVALID', 'controller lease signing failed');
      }
      const dispatchPayload: LeaseDispatchPayload = {
        schema_version: CONTRIBUTOR_LEASE_DISPATCH_SCHEMA,
        dispatch_id: `dispatch-${requestHash.slice(0, 48)}`,
        request_id: request.request_id,
        idempotency_key: request.idempotency_key,
        node_id: request.node_id,
        lease,
      };
      const dispatch: LeaseDispatch = {
        ...dispatchPayload,
        signature_b64: controllerSignature(dispatchPayload, controllerPrivateKey),
      };
      return {
        nodeId: request.node_id,
        idempotencyKey: request.idempotency_key,
        requestHash,
        dispatch,
      };
    },

    async result(value: unknown): Promise<ContributorTransportAtomicResultInput> {
      const submission = parseContributorResultSubmission(value);
      const node = requireExistingNode(await store.getNode(submission.node_id));
      const leaseReplay = await store.getLeaseByDispatch(submission.dispatch_id);
      if (!leaseReplay) throw new ContributorTransportError('TRANSPORT_LEASE_UNKNOWN');
      const dispatch = leaseReplay.dispatch;
      requireNodeIdentity(node, submission, dispatch);
      verifyResultSubmission(
        submission,
        publicKeyFromSpkiB64(node.node_public_key_spki_b64),
        node.node_id,
        node.node_key_id,
      );
      const resultHash = contributorPayloadHash(submission.result);
      const submissionHash = contributorPayloadHash(resultSubmissionPayload(submission));
      const receiptPayload: ResultReceiptPayload = {
        schema_version: CONTRIBUTOR_RESULT_RECEIPT_SCHEMA,
        dispatch_id: submission.dispatch_id,
        request_id: submission.request_id,
        idempotency_key: submission.idempotency_key,
        node_id: submission.node_id,
        lease_id: submission.result.lease_id,
        result_hash: resultHash,
        status: 'accepted',
        accepted_at: now(),
      };
      const receipt: ResultReceipt = {
        ...receiptPayload,
        signature_b64: controllerSignature(receiptPayload, controllerPrivateKey),
      };
      return {
        nodeId: submission.node_id,
        dispatchId: submission.dispatch_id,
        submissionHash,
        receipt,
      };
    },

    async revoke(value: unknown): Promise<ContributorTransportAtomicRevokeInput> {
      if (!operatorKeyId || !operatorPublicKey) {
        throw new ContributorTransportError('TRANSPORT_REVOCATION_UNCONFIGURED');
      }
      const request = verifyRevokeRequest(value, operatorPublicKey, operatorKeyId, now());
      const requestHash = contributorPayloadHash(revokePayload(request));
      const existing = requireExistingNode(await store.getNode(request.node_id));
      const alreadyRevoked = existing.status === 'revoked';
      const revision = existing.revision + (alreadyRevoked ? 0 : 1);
      const receiptPayload: RevokeReceiptPayload = {
        schema_version: CONTRIBUTOR_REVOKE_RECEIPT_SCHEMA,
        request_id: request.request_id,
        node_id: request.node_id,
        controller_key_id: controllerKeyId,
        status: alreadyRevoked ? 'already_revoked' : 'revoked',
        revision,
        reason: request.reason,
        revoked_at: existing.revoked_at ?? now(),
      };
      const receipt: RevokeReceipt = {
        ...receiptPayload,
        signature_b64: controllerSignature(receiptPayload, controllerPrivateKey),
      };
      return {
        requestId: request.request_id,
        nodeId: request.node_id,
        requestHash,
        reason: request.reason,
        receipt,
      };
    },
  };
}

/**
 * Resolve the default controller only when the caller has supplied the
 * external limiter and all dedicated controller/operator/persistence seams
 * are valid.  Missing configuration returns null and leaves routes closed.
 */
export function createProductionContributorAtomicController(
  options: ProductionContributorAtomicControllerOptions,
): ContributorAtomicRouteController | null {
  if (!options.rateLimiterConfigured) return null;
  const controllerKeyId = envKeyId(CONTRIBUTOR_CONTROLLER_ENV.keyId);
  const controllerPrivateKey = envPrivateKey(CONTRIBUTOR_CONTROLLER_ENV.privateKeyPem);
  if (!controllerKeyId || !controllerPrivateKey) return null;

  let store = options.store;
  if (!store) {
    const availability = createSupabaseContributorTransportStore();
    if (!availability.ok) return null;
    store = availability.store;
  }
  if (!store.atomicRpcCapable) return null;

  const operatorKeyId = envKeyId(CONTRIBUTOR_CONTROLLER_ENV.operatorKeyId);
  const operatorPublicKey = envPublicKey(CONTRIBUTOR_CONTROLLER_ENV.operatorPublicKeyPem);
  const clock = options.now ?? (() => Date.now());
  const operations = {
    atomicEnroll: (input: ContributorTransportAtomicEnrollmentInput) => store!.atomicEnroll(input),
    atomicIssueLease: (input: ContributorTransportAtomicLeaseInput) => store!.atomicIssueLease(input),
    atomicAcceptResult: (input: ContributorTransportAtomicResultInput) => store!.atomicAcceptResult(input),
    atomicRevoke: (input: ContributorTransportAtomicRevokeInput) => store!.atomicRevoke(input),
  };
  return {
    atomicRpcCapable: true,
    genericTransactionCapable: store.transactional,
    controllerSigningConfigured: true,
    operatorConfigured: Boolean(operatorKeyId && operatorPublicKey),
    operations,
    prepare: createPrepare(
      store,
      controllerKeyId,
      controllerPrivateKey,
      operatorKeyId,
      operatorPublicKey,
      clock,
    ),
  };
}
