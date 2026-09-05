import { z } from "zod";

export const ExplicitConfirmationUnsignedSchema = z
  .object({
    action: z.string().trim().min(3).max(120),
    target: z.string().trim().min(1).max(500),
    nonce: z.string().min(24).max(256),
    confirmedAt: z.string().datetime({ offset: true }),
  })
  .strict();

export const ExplicitConfirmationSchema =
  ExplicitConfirmationUnsignedSchema.extend({
    digest: z.string().regex(/^sha256:[a-f0-9]{64}$/),
  }).strict();

export type ExplicitConfirmationUnsigned = z.infer<
  typeof ExplicitConfirmationUnsignedSchema
>;
export type ExplicitConfirmation = z.infer<
  typeof ExplicitConfirmationSchema
>;
