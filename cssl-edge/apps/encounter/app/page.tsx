import { requirePrivateContext } from "@apocky/security/server";
import { notFound } from "next/navigation";

import { EncounterExperience } from "./encounter-experience";
import { readCurrentEncounter } from "@/lib/encounters";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

export default async function EncounterPage() {
  let initialEncounter = null;
  let initialError: string | null = null;
  try {
    const context = await requirePrivateContext();
    try {
      initialEncounter = await readCurrentEncounter(context);
    } catch {
      initialError =
        "The encounter exists, but its current authority could not be verified.";
    }
  } catch {
    notFound();
  }

  return (
    <EncounterExperience
      initialEncounter={initialEncounter}
      initialError={initialError}
    />
  );
}
