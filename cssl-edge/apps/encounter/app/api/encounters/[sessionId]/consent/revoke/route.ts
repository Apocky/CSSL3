import { handleRevokeConsent } from "@/lib/route-handlers";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

interface Context {
  params: Promise<{ sessionId: string }>;
}

export async function POST(request: Request, context: Context) {
  return handleRevokeConsent(request, (await context.params).sessionId);
}
