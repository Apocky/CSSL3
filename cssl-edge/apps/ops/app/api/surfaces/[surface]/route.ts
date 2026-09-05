import { handleSurface } from "@/lib/route-handlers";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

interface Context {
  params: Promise<{ surface: string }>;
}

export async function GET(_request: Request, context: Context) {
  return handleSurface((await context.params).surface);
}
