import type {
  UnderstandingAcknowledgement,
  UnderstandingVersion,
} from "./types";

export type UnderstandingOutcome =
  | "pending"
  | "needs_repair"
  | "disagreed"
  | "mutually_understood";

export interface UnderstandingEvaluation {
  outcome: UnderstandingOutcome;
  versionDigest: string;
  acknowledgements: readonly UnderstandingAcknowledgement[];
}

export function evaluateUnderstanding(
  version: UnderstandingVersion,
  participantPrincipals: readonly [string, string],
  acknowledgements: readonly UnderstandingAcknowledgement[],
): UnderstandingEvaluation {
  const relevant = acknowledgements.filter(
    (acknowledgement) =>
      acknowledgement.sessionId === version.sessionId &&
      acknowledgement.versionDigest === version.canonicalDigest &&
      participantPrincipals.includes(acknowledgement.participant),
  );

  const latestByParticipant = new Map<string, UnderstandingAcknowledgement>();
  for (const acknowledgement of relevant) {
    const previous = latestByParticipant.get(acknowledgement.participant);
    if (
      previous === undefined ||
      acknowledgement.acknowledgedAt > previous.acknowledgedAt
    ) {
      latestByParticipant.set(
        acknowledgement.participant,
        acknowledgement,
      );
    }
  }

  const latest = participantPrincipals
    .map((principal) => latestByParticipant.get(principal))
    .filter(
      (
        acknowledgement,
      ): acknowledgement is UnderstandingAcknowledgement =>
        acknowledgement !== undefined,
    );

  let outcome: UnderstandingOutcome = "pending";
  if (latest.some(({ status }) => status === "disagree")) {
    outcome = "disagreed";
  } else if (latest.some(({ status }) => status === "needs_repair")) {
    outcome = "needs_repair";
  } else if (
    latest.length === 2 &&
    latest.every(({ status }) => status === "understood")
  ) {
    outcome = "mutually_understood";
  }

  return {
    outcome,
    versionDigest: version.canonicalDigest,
    acknowledgements: latest,
  };
}

export function correctionRequiresNewVersion(
  current: UnderstandingVersion,
  next: UnderstandingVersion,
): boolean {
  return (
    current.sessionId === next.sessionId &&
    next.version === current.version + 1 &&
    next.versionId !== current.versionId &&
    next.canonicalDigest !== current.canonicalDigest
  );
}
