import { requirePrivateContext } from "@apocky/security/server";
import { notFound } from "next/navigation";

import { OperationsConsole } from "./operations-console";
import { readOpsSnapshot } from "@/lib/projection";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

export default async function OperationsPage() {
  try {
    const context = await requirePrivateContext();
    const snapshot = await readOpsSnapshot(context);
    return (
      <OperationsConsole
        initialSnapshot={snapshot}
        principalEmail={context.principal.email}
      />
    );
  } catch {
    notFound();
  }
}
