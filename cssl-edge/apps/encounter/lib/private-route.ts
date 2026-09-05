import {
  privateErrorResponse,
  privateNoStoreHeaders,
  requirePrivateContext,
  securityHeaders,
  type PrivateContext,
} from "@apocky/security/server";

export type PrivateRouteWork = (
  context: PrivateContext,
) => Promise<Response> | Response;

export interface PrivateRouteDependencies {
  authorize: () => Promise<PrivateContext>;
  accessError: (error: unknown) => Response;
}

const defaultDependencies: PrivateRouteDependencies = {
  authorize: requirePrivateContext,
  accessError: privateErrorResponse,
};

export function privateJson(
  body: unknown,
  init: { status?: number; allowMedia?: boolean } = {},
): Response {
  return Response.json(body, {
    status: init.status ?? 200,
    headers: {
      ...securityHeaders({ allowMedia: init.allowMedia ?? false }),
      ...privateNoStoreHeaders,
    },
  });
}

export async function runPrivateRoute(
  work: PrivateRouteWork,
  dependencies: PrivateRouteDependencies = defaultDependencies,
): Promise<Response> {
  let context: PrivateContext;
  try {
    context = await dependencies.authorize();
  } catch (error) {
    return dependencies.accessError(error);
  }

  try {
    return await work(context);
  } catch {
    return privateJson(
      {
        ok: false,
        error: {
          code: "private_operation_unavailable",
          message: "The private operation failed closed.",
        },
      },
      { status: 503 },
    );
  }
}

export async function readBoundedJson(
  request: Request,
  maximumBytes = 128 * 1024,
): Promise<unknown> {
  const declaredLength = Number(request.headers.get("content-length") ?? "0");
  if (
    !Number.isFinite(declaredLength) ||
    declaredLength < 0 ||
    declaredLength > maximumBytes
  ) {
    throw new Error("Request body exceeds the private command limit.");
  }
  const text = await request.text();
  if (new TextEncoder().encode(text).byteLength > maximumBytes) {
    throw new Error("Request body exceeds the private command limit.");
  }
  return JSON.parse(text);
}
