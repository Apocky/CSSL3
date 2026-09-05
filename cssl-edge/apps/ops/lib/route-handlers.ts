import { requireMutationGuard } from "@apocky/security/server";

import { executeOpsAction } from "./actions";
import { OpsCommandSchema } from "./confirmation";
import {
  privateJson,
  readBoundedJson,
  runPrivateRoute,
} from "./private-route";
import { readOpsSnapshot, readOpsSurface } from "./projection";
import { isOpsReadSurface } from "./routes";

export async function handleSnapshot(): Promise<Response> {
  return runPrivateRoute(async (context) =>
    privateJson({
      ok: true,
      snapshot: await readOpsSnapshot(context),
    }),
  );
}

export async function handleSurface(
  surfaceInput: string,
): Promise<Response> {
  return runPrivateRoute(async (context) => {
    if (!isOpsReadSurface(surfaceInput)) {
      return privateJson(
        {
          ok: false,
          error: {
            code: "unknown_ops_surface",
            message: "The requested evidence surface is not allowlisted.",
          },
        },
        { status: 404 },
      );
    }
    return privateJson({
      ok: true,
      surface: surfaceInput,
      evidence: await readOpsSurface(context, surfaceInput),
    });
  });
}

export async function handleAction(request: Request): Promise<Response> {
  return runPrivateRoute(async (context) => {
    const parsed = OpsCommandSchema.safeParse(await readBoundedJson(request));
    if (!parsed.success) {
      return privateJson(
        {
          ok: false,
          error: {
            code: "invalid_ops_command",
            message:
              "The action, target, evidence digest, or typed phrase did not match.",
          },
        },
        { status: 400 },
      );
    }
    const command = parsed.data;
    requireMutationGuard(context, request, {
      action: command.action,
      target: command.target,
      confirmation: command.confirmation,
    });
    const snapshot = await readOpsSnapshot(context);
    const authorized = snapshot.sessions
      .flatMap(({ allowedActions }) => allowedActions)
      .concat(
        snapshot.retention.withdrawals.flatMap(({ allowedAction }) =>
          allowedAction === null ? [] : [allowedAction],
        ),
      )
      .some(
        (candidate) =>
          candidate.action === command.action &&
          candidate.target === command.target &&
          candidate.expectedDigest === command.expectedDigest,
      );
    if (!authorized) {
      return privateJson(
        {
          ok: false,
          error: {
            code: "stale_ops_evidence",
            message:
              "The live action target no longer matches the confirmed evidence digest.",
          },
        },
        { status: 409 },
      );
    }

    return privateJson({
      ok: true,
      result: await executeOpsAction(context, command),
    });
  });
}
