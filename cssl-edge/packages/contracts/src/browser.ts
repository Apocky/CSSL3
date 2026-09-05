import { canonicalJson } from "./canonical";

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join(
    "",
  );
}

export async function sha256HexBrowser(
  value: string | Uint8Array,
): Promise<string> {
  const bytes =
    typeof value === "string" ? new TextEncoder().encode(value) : value;
  const stableBytes = Uint8Array.from(bytes).buffer;
  const digest = await globalThis.crypto.subtle.digest(
    "SHA-256",
    stableBytes,
  );
  return toHex(new Uint8Array(digest));
}

export async function digestCanonicalBrowser(
  value: unknown,
): Promise<`sha256:${string}`> {
  return `sha256:${await sha256HexBrowser(canonicalJson(value))}`;
}
