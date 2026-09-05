"use client";

import { createExplicitConfirmation } from "@apocky/security/client";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";

import {
  buildConfirmationPhrase,
  OPS_ACTION_LABELS,
} from "@/lib/confirmation";
import type {
  OpsAllowedAction,
  OpsSnapshot,
} from "@/lib/projection";

interface OperationsConsoleProps {
  initialSnapshot: OpsSnapshot;
  principalEmail: string;
}

function shortDigest(value: string): string {
  return value.length > 24
    ? `${value.slice(0, 15)}…${value.slice(-8)}`
    : value;
}

function formatTime(value: string | null): string {
  return value === null ? "—" : new Date(value).toLocaleString();
}

async function readSnapshotResponse(response: Response): Promise<OpsSnapshot> {
  const payload = (await response.json()) as {
    ok?: boolean;
    snapshot?: OpsSnapshot;
    error?: { message?: string };
  };
  if (!response.ok || payload.ok !== true || payload.snapshot === undefined) {
    throw new Error(payload.error?.message ?? "Evidence refresh failed.");
  }
  return payload.snapshot;
}

export function OperationsConsole({
  initialSnapshot,
  principalEmail,
}: OperationsConsoleProps) {
  const [snapshot, setSnapshot] = useState(initialSnapshot);
  const [selectedAction, setSelectedAction] =
    useState<OpsAllowedAction | null>(null);
  const [phrase, setPhrase] = useState("");
  const [signedReceipt, setSignedReceipt] = useState("");
  const [upstreamReceiptDigest, setUpstreamReceiptDigest] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const expectedPhrase = useMemo(
    () =>
      selectedAction === null
        ? ""
        : buildConfirmationPhrase(
            selectedAction.action,
            selectedAction.target,
            selectedAction.expectedDigest,
            selectedAction.action === "complete_retention_withdrawal"
              ? upstreamReceiptDigest
              : undefined,
          ),
    [selectedAction, upstreamReceiptDigest],
  );

  const refresh = useCallback(async () => {
    try {
      const response = await fetch("/api/snapshot", {
        cache: "no-store",
        credentials: "same-origin",
      });
      setSnapshot(await readSnapshotResponse(response));
      setError(null);
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Evidence refresh failed.",
      );
    }
  }, []);

  useEffect(() => {
    if (selectedAction !== null) return;
    const interval = window.setInterval(() => void refresh(), 10_000);
    return () => window.clearInterval(interval);
  }, [refresh, selectedAction]);

  const executeAction = useCallback(async () => {
    if (selectedAction === null || phrase !== expectedPhrase) return;
    setBusy(true);
    setError(null);
    try {
      let encounterReceipt: unknown;
      if (
        selectedAction.action === "end_encounter" ||
        selectedAction.action === "revoke_encounter_grant"
      ) {
        encounterReceipt = JSON.parse(signedReceipt) as unknown;
      }
      const confirmation = await createExplicitConfirmation({
        action: selectedAction.action,
        target: selectedAction.target,
        nonce: crypto.randomUUID(),
        confirmedAt: new Date().toISOString(),
      });
      const response = await fetch("/api/actions", {
        method: "POST",
        cache: "no-store",
        credentials: "same-origin",
        headers: {
          "content-type": "application/json",
          "x-apocky-confirmation-digest": confirmation.digest,
        },
        body: JSON.stringify({
          ...selectedAction,
          phrase,
          confirmation,
          ...(encounterReceipt === undefined ? {} : { encounterReceipt }),
          ...(selectedAction.action === "complete_retention_withdrawal"
            ? { upstreamReceiptDigest }
            : {}),
        }),
      });
      const payload = (await response.json()) as {
        ok?: boolean;
        error?: { message?: string };
      };
      if (!response.ok || payload.ok !== true) {
        throw new Error(
          payload.error?.message ?? "The protected action failed.",
        );
      }
      setSelectedAction(null);
      setPhrase("");
      setSignedReceipt("");
      setUpstreamReceiptDigest("");
      setNotice(
        `${OPS_ACTION_LABELS[selectedAction.action]} completed with an audit receipt.`,
      );
      await refresh();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "The action failed closed.",
      );
    } finally {
      setBusy(false);
    }
  }, [
    expectedPhrase,
    phrase,
    refresh,
    selectedAction,
    signedReceipt,
    upstreamReceiptDigest,
  ]);

  return (
    <main className="ops-shell">
      <header className="ops-header">
        <div>
          <p className="ops-kicker">Protected · unlisted · evidence only</p>
          <h1>Operations</h1>
        </div>
        <div className="ops-principal">
          <strong>Verified owner</strong>
          <br />
          {principalEmail}
          <br />
          Evidence refreshed {formatTime(snapshot.generatedAt)}
        </div>
      </header>

      <div className="ops-toolbar">
        <p>
          This console reads bounded provenance surfaces. It does not infer
          health from missing evidence and has no deployment mutation.
        </p>
        <button type="button" onClick={() => void refresh()}>
          Refresh evidence
        </button>
      </div>

      {error ? (
        <p className="ops-status ops-error" role="alert">
          {error}
        </p>
      ) : null}
      {notice ? (
        <p className="ops-status" role="status">
          {notice}
        </p>
      ) : null}

      <div className="ops-grid">
        <section className="ops-card">
          <p className="ops-kicker">Runtime</p>
          <h2>Verified gates</h2>
          <dl className="ops-list">
            <div className="ops-row">
              <dt>Cloudflare Access</dt>
              <dd>{snapshot.runtime.cloudflareAccess}</dd>
            </div>
            <div className="ops-row">
              <dt>Application session</dt>
              <dd>{snapshot.runtime.applicationSession}</dd>
            </div>
            <div className="ops-row">
              <dt>Owner allowlist</dt>
              <dd>{snapshot.runtime.ownerAllowlist}</dd>
            </div>
            <div className="ops-row">
              <dt>LiveKit configuration</dt>
              <dd>
                {snapshot.runtime.liveKitConfigured
                  ? "present"
                  : "missing · media fails closed"}
              </dd>
            </div>
          </dl>
        </section>

        <section className="ops-card">
          <p className="ops-kicker">Authority</p>
          <h2>Keys &amp; manifests</h2>
          {snapshot.authority.participantKeys.length === 0 &&
          snapshot.authority.manifests.length === 0 ? (
            <p className="ops-empty">No authority evidence returned.</p>
          ) : (
            <ul className="ops-list">
              {snapshot.authority.participantKeys.map((key) => (
                <li key={key.keyId}>
                  <strong>
                    {key.role} · {key.principal}
                  </strong>
                  <br />
                  <span className="ops-mono">{key.keyId}</span>
                  <br />
                  {key.revokedAt
                    ? `revoked ${formatTime(key.revokedAt)}`
                    : `issued ${formatTime(key.issuedAt)}`}
                </li>
              ))}
              {snapshot.authority.manifests.map((manifest) => (
                <li key={manifest.id}>
                  <strong>{manifest.kind} manifest</strong>
                  <br />
                  <span className="ops-mono" title={manifest.digest}>
                    {shortDigest(manifest.digest)}
                  </span>
                  <br />
                  {manifest.revokedAt
                    ? `revoked ${formatTime(manifest.revokedAt)}`
                    : `active · ${manifest.authorPrincipal}`}
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="ops-card" data-span="wide">
          <p className="ops-kicker">Sessions</p>
          <h2>Encounter state</h2>
          {snapshot.sessions.length === 0 ? (
            <p className="ops-empty">No encounter session rows returned.</p>
          ) : (
            <div className="ops-session-grid">
              {snapshot.sessions.map((session) => (
                <article className="ops-session" key={session.id}>
                  <div>
                    <p>
                      <strong>{session.state.replaceAll("_", " ")}</strong>
                    </p>
                    <p className="ops-mono">{session.id}</p>
                    <dl className="ops-list">
                      <div className="ops-row">
                        <dt>Grant</dt>
                        <dd title={session.grantDigest}>
                          {shortDigest(session.grantDigest)}
                        </dd>
                      </div>
                      <div className="ops-row">
                        <dt>Started</dt>
                        <dd>{formatTime(session.startedAt)}</dd>
                      </div>
                      <div className="ops-row">
                        <dt>Ended</dt>
                        <dd>{formatTime(session.endedAt)}</dd>
                      </div>
                    </dl>
                  </div>
                  {session.allowedActions.length > 0 ? (
                    <div className="ops-actions">
                      {session.allowedActions.map((action) => (
                        <button
                          type="button"
                          data-kind="danger"
                          key={action.action}
                          onClick={() => {
                            setSelectedAction(action);
                            setPhrase("");
                            setSignedReceipt("");
                            setUpstreamReceiptDigest("");
                            setNotice(null);
                            setError(null);
                          }}
                        >
                          {OPS_ACTION_LABELS[action.action]}
                        </button>
                      ))}
                    </div>
                  ) : (
                    <p className="ops-empty">No current action is authorized.</p>
                  )}
                </article>
              ))}
            </div>
          )}
        </section>

        <section className="ops-card">
          <p className="ops-kicker">Consent</p>
          <h2>Signed receipt rows</h2>
          {snapshot.consent.length === 0 ? (
            <p className="ops-empty">No consent evidence returned.</p>
          ) : (
            <ul className="ops-list">
              {snapshot.consent.slice(0, 30).map((row, index) => (
                <li
                  key={`${row.receiptDigest}:${row.modality}:${index}`}
                >
                  <strong>
                    {row.modality} · {row.state}
                  </strong>
                  <br />
                  {row.participant}
                  <br />
                  <span className="ops-mono" title={row.receiptDigest}>
                    {shortDigest(row.receiptDigest)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="ops-card">
          <p className="ops-kicker">Retention</p>
          <h2>Mutual artifacts</h2>
          {snapshot.retention.decisions.length === 0 ? (
            <p className="ops-empty">No retention decision returned.</p>
          ) : (
            <ul className="ops-list">
              {snapshot.retention.decisions.map((decision) => (
                <li key={decision.id}>
                  <strong>
                    {decision.artifactCount} retained artifact
                    {decision.artifactCount === 1 ? "" : "s"}
                  </strong>
                  <br />
                  {decision.acknowledgementCount}/2 acknowledgements ·{" "}
                  {decision.artifactClasses.join(", ") || "no classes"}
                  <br />
                  <span className="ops-mono" title={decision.decisionDigest}>
                    {shortDigest(decision.decisionDigest)}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {snapshot.retention.withdrawals.length > 0 ? (
            <ul className="ops-list">
              {snapshot.retention.withdrawals.map((withdrawal) => (
                <li key={withdrawal.id}>
                  <strong>
                    withdrawal · {withdrawal.workflowState}
                  </strong>
                  <br />
                  {withdrawal.deletedArtifactCount} local artifact(s) ·{" "}
                  {withdrawal.artifactClasses.join(", ")}
                  {withdrawal.allowedAction ? (
                    <>
                      <br />
                      <button
                        type="button"
                        data-kind="danger"
                        onClick={() => {
                          setSelectedAction(withdrawal.allowedAction);
                          setPhrase("");
                          setSignedReceipt("");
                          setUpstreamReceiptDigest("");
                        }}
                      >
                        {
                          OPS_ACTION_LABELS[
                            withdrawal.allowedAction.action
                          ]
                        }
                      </button>
                    </>
                  ) : null}
                </li>
              ))}
            </ul>
          ) : null}
        </section>

        <section className="ops-card">
          <p className="ops-kicker">Deployment</p>
          <h2>Recorded provenance</h2>
          {snapshot.deployment.length === 0 ? (
            <p className="ops-empty">
              No deployment records returned. That is unverified, not healthy.
            </p>
          ) : (
            <ul className="ops-list">
              {snapshot.deployment.map((record) => (
                <li key={record.id}>
                  <strong>
                    {record.surface} · {record.environment} · {record.state}
                  </strong>
                  <br />
                  {record.buildIdentity}
                  <br />
                  <span className="ops-mono">{record.commitSha}</span>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="ops-card">
          <p className="ops-kicker">Security</p>
          <h2>Immutable audit trail</h2>
          {snapshot.security.length === 0 ? (
            <p className="ops-empty">
              No audit receipts returned. That is unverified, not clean.
            </p>
          ) : (
            <ul className="ops-list">
              {snapshot.security.slice(0, 40).map((receipt) => (
                <li key={receipt.id}>
                  <strong>
                    {receipt.outcome} · {receipt.action}
                  </strong>
                  <br />
                  {receipt.target}
                  <br />
                  <span className="ops-mono" title={receipt.receiptDigest}>
                    {shortDigest(receipt.receiptDigest)}
                  </span>{" "}
                  · {formatTime(receipt.createdAt)}
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>

      {selectedAction ? (
        <div className="ops-dialog-backdrop" role="presentation">
          <section
            className="ops-confirmation ops-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="ops-confirm-title"
          >
            <p className="ops-kicker">Consequential action</p>
            <h2 id="ops-confirm-title">
              {OPS_ACTION_LABELS[selectedAction.action]}
            </h2>
            <p>
              Live target: <code>{selectedAction.target}</code>
            </p>
            {selectedAction.action === "end_encounter" ||
            selectedAction.action === "revoke_encounter_grant" ? (
              <label className="ops-stack">
                <span>Participant-signed encounter receipt JSON</span>
                <textarea
                  rows={6}
                  value={signedReceipt}
                  onChange={(event) =>
                    setSignedReceipt(event.currentTarget.value)
                  }
                  spellCheck={false}
                />
              </label>
            ) : null}
            {selectedAction.action ===
            "complete_retention_withdrawal" ? (
              <label className="ops-stack">
                <span>Upstream withdrawal receipt digest</span>
                <input
                  value={upstreamReceiptDigest}
                  onChange={(event) =>
                    setUpstreamReceiptDigest(event.currentTarget.value)
                  }
                  spellCheck={false}
                />
              </label>
            ) : null}
            <p>
              Evidence digest:{" "}
              <code>{selectedAction.expectedDigest}</code>
            </p>
            <label className="ops-stack">
              <span>Type this exact phrase:</span>
              <code className="ops-confirm-phrase">{expectedPhrase}</code>
              <input
                autoFocus
                autoComplete="off"
                spellCheck={false}
                value={phrase}
                onChange={(event) => setPhrase(event.currentTarget.value)}
              />
            </label>
            <div className="ops-dialog-actions">
              <button
                type="button"
                onClick={() => {
                  setSelectedAction(null);
                  setPhrase("");
                  setSignedReceipt("");
                  setUpstreamReceiptDigest("");
                }}
                disabled={busy}
              >
                Cancel
              </button>
              <button
                type="button"
                data-kind="danger"
                disabled={
                  busy ||
                  phrase !== expectedPhrase ||
                  (["end_encounter", "revoke_encounter_grant"].includes(
                    selectedAction.action,
                  ) &&
                    signedReceipt.trim() === "") ||
                  (selectedAction.action ===
                    "complete_retention_withdrawal" &&
                    !/^sha256:[a-f0-9]{64}$/.test(upstreamReceiptDigest))
                }
                onClick={() => void executeAction()}
              >
                Confirm exact action
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </main>
  );
}
