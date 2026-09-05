import {
  correctionRequiresNewVersion,
  evaluateUnderstanding,
  type UnderstandingAcknowledgement,
  type UnderstandingOutcome,
  type UnderstandingVersion,
} from "@apocky/contracts";

export interface UnderstandingState {
  outcome: UnderstandingOutcome;
  versionDigest: string;
  acknowledgements: readonly UnderstandingAcknowledgement[];
}

export function deriveUnderstandingState(
  version: UnderstandingVersion,
  participants: readonly [string, string],
  acknowledgements: readonly UnderstandingAcknowledgement[],
): UnderstandingState {
  return evaluateUnderstanding(version, participants, acknowledgements);
}

export function assertCorrectionTransition(
  current: UnderstandingVersion,
  corrected: UnderstandingVersion,
): void {
  if (!correctionRequiresNewVersion(current, corrected)) {
    throw new Error(
      "A correction must create the next version with a new identifier and digest.",
    );
  }
}

export function outcomeOnExit(
  state: UnderstandingState | null,
): "mutually_understood" | "ended_unresolved" {
  return state?.outcome === "mutually_understood"
    ? "mutually_understood"
    : "ended_unresolved";
}
