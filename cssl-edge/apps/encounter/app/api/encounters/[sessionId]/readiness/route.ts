import {
  handleReadiness,
  handleSetReadiness,
} from "@/lib/route-handlers";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

interface Context {
  params: Promise<{ sessionId: string }>;
}

export async function GET(_request: Request, context: Context) {
  return handleReadiness((await context.params).sessionId);
}

export async function POST(request: Request, context: Context) {
  return handleSetReadiness(request, (await context.params).sessionId);
}
