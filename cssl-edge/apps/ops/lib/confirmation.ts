import { EncounterReceiptSchema } from "@apocky/contracts";
import { z } from "zod";

export const OpsActionSchema = z.enum([
  "end_encounter",
  "revoke_encounter_grant",
  "delete_retained_history",
  "complete_retention_withdrawal",
]);

export type OpsAction = z.infer<typeof OpsActionSchema>;

export const OPS_ACTION_LABELS: Readonly<Record<OpsAction, string>> = {
  end_encounter: "END ENCOUNTER",
  revoke_encounter_grant: "REVOKE GRANT",
  delete_retained_history: "DELETE RETAINED HISTORY",
  complete_retention_withdrawal: "COMPLETE RETENTION WITHDRAWAL",
};

export function buildConfirmationPhrase(
  action: OpsAction,
  target: string,
  expectedDigest: string,
  effectDigest?: string,
): string {
  return `CONFIRM ${OPS_ACTION_LABELS[action]} ${target} ${expectedDigest}${
    effectDigest ? ` ${effectDigest}` : ""
  }`;
}

export const OpsCommandSchema = z
  .object({
    action: OpsActionSchema,
    target: z.string().uuid(),
    expectedDigest: z.string().regex(/^sha256:[a-f0-9]{64}$/),
    phrase: z.string().max(512),
    confirmation: z.unknown(),
    encounterReceipt: EncounterReceiptSchema.optional(),
    upstreamReceiptDigest: z
      .string()
      .regex(/^sha256:[a-f0-9]{64}$/)
      .optional(),
  })
  .strict()
  .superRefine((command, context) => {
    if (
      command.phrase !==
      buildConfirmationPhrase(
        command.action,
        command.target,
        command.expectedDigest,
        command.upstreamReceiptDigest,
      )
    ) {
      context.addIssue({
        code: "custom",
        message: "Confirmation does not exactly bind the action, target, and digest.",
        path: ["phrase"],
      });
    }
    if (
      ["end_encounter", "revoke_encounter_grant"].includes(
        command.action,
      ) &&
      command.encounterReceipt === undefined
    ) {
      context.addIssue({
        code: "custom",
        message: "Encounter finalization requires a signed receipt.",
        path: ["encounterReceipt"],
      });
    }
    if (
      command.action === "end_encounter" &&
      command.encounterReceipt?.endState === "revoked"
    ) {
      context.addIssue({
        code: "custom",
        message: "Revocation must use the revoke-grant action.",
        path: ["encounterReceipt", "endState"],
      });
    }
    if (
      command.action === "revoke_encounter_grant" &&
      command.encounterReceipt?.endState !== "revoked"
    ) {
      context.addIssue({
        code: "custom",
        message: "Grant revocation requires a revoked end receipt.",
        path: ["encounterReceipt", "endState"],
      });
    }
    if (
      (command.action === "complete_retention_withdrawal") !==
      (command.upstreamReceiptDigest !== undefined)
    ) {
      context.addIssue({
        code: "custom",
        message:
          "Only retention completion accepts an upstream receipt digest.",
        path: ["upstreamReceiptDigest"],
      });
    }
    if (
      !["end_encounter", "revoke_encounter_grant"].includes(
        command.action,
      ) &&
      command.encounterReceipt !== undefined
    ) {
      context.addIssue({
        code: "custom",
        message: "This action does not accept an encounter receipt.",
        path: ["encounterReceipt"],
      });
    }
  });

export type OpsCommand = z.infer<typeof OpsCommandSchema>;
