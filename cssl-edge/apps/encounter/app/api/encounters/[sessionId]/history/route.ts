import {
  handleDeleteHistory,
  handleHistory,
} from "@/lib/route-handlers";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

interface Context {
  params: Promise<{ sessionId: string }>;
}

export async function GET(_request: Request, context: Context) {
  return handleHistory((await context.params).sessionId);
}

export async function DELETE(request: Request, context: Context) {
  return handleDeleteHistory(request, (await context.params).sessionId);
}
