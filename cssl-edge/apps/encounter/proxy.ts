import { securityHeaders } from "@apocky/security";
import { NextResponse } from "next/server";

export function proxy(): NextResponse {
  const response = NextResponse.next();
  const liveKitUrl = process.env.NEXT_PUBLIC_LIVEKIT_URL?.trim();
  for (const [name, value] of Object.entries(
    securityHeaders({
      allowMedia: true,
      ...(liveKitUrl ? { liveKitUrl } : {}),
    }),
  )) {
    response.headers.set(name, value);
  }
  response.headers.set(
    "Cache-Control",
    "private, no-store, max-age=0, must-revalidate",
  );
  return response;
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
