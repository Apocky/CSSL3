import { digestCanonicalBrowser } from "@apocky/contracts";

import {
  ExplicitConfirmationUnsignedSchema,
  type ExplicitConfirmation,
  type ExplicitConfirmationUnsigned,
} from "./confirmation";

export async function createExplicitConfirmation(
  input: ExplicitConfirmationUnsigned,
): Promise<ExplicitConfirmation> {
  const unsigned = ExplicitConfirmationUnsignedSchema.parse(input);
  return {
    ...unsigned,
    digest: await digestCanonicalBrowser(unsigned),
  };
}

export type {
  ExplicitConfirmation,
  ExplicitConfirmationUnsigned,
} from "./confirmation";
