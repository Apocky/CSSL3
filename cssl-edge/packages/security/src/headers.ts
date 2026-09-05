export type HeaderRecord = Readonly<Record<string, string>>;

export function buildContentSecurityPolicy(options?: {
  liveKitUrl?: string;
  allowMedia?: boolean;
}): string {
  const connectSources = ["'self'", "https:", "wss:"];
  if (options?.liveKitUrl !== undefined) {
    const liveKitUrl = new URL(options.liveKitUrl);
    if (!["https:", "wss:"].includes(liveKitUrl.protocol)) {
      throw new TypeError("LiveKit URL must use https or wss");
    }
    connectSources.push(liveKitUrl.origin);
  }

  return [
    "default-src 'self'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
    "object-src 'none'",
    "script-src 'self' 'unsafe-inline'",
    "style-src 'self' 'unsafe-inline'",
    "img-src 'self' data: blob:",
    options?.allowMedia
      ? "media-src 'self' blob:"
      : "media-src 'none'",
    `connect-src ${connectSources.join(" ")}`,
    "font-src 'self'",
    "manifest-src 'self'",
    "worker-src 'self' blob:",
    "upgrade-insecure-requests",
  ].join("; ");
}

export function securityHeaders(options?: {
  liveKitUrl?: string;
  allowMedia?: boolean;
  publicSite?: boolean;
}): HeaderRecord {
  const mediaPermission = options?.allowMedia ? "(self)" : "()";
  return {
    "Content-Security-Policy": buildContentSecurityPolicy(options),
    "Cross-Origin-Opener-Policy": "same-origin",
    "Cross-Origin-Resource-Policy": "same-origin",
    "Origin-Agent-Cluster": "?1",
    "Permissions-Policy": [
      `camera=${mediaPermission}`,
      `microphone=${mediaPermission}`,
      "geolocation=()",
      "interest-cohort=()",
      "payment=()",
      "usb=()",
    ].join(", "),
    "Referrer-Policy": options?.publicSite
      ? "strict-origin-when-cross-origin"
      : "no-referrer",
    "Strict-Transport-Security":
      "max-age=63072000; includeSubDomains; preload",
    "X-Content-Type-Options": "nosniff",
    "X-Frame-Options": "DENY",
    "X-Permitted-Cross-Domain-Policies": "none",
  };
}

export const privateNoStoreHeaders: HeaderRecord = {
  "Cache-Control": "private, no-store, max-age=0, must-revalidate",
  Expires: "0",
  Pragma: "no-cache",
};

export function mergeHeaders(
  ...records: readonly HeaderRecord[]
): Record<string, string> {
  return Object.assign({}, ...records);
}
