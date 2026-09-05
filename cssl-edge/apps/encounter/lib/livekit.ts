import { createHmac } from "node:crypto";

import { AccessToken } from "livekit-server-sdk";

import type { EncounterAuthorityBundle } from "@apocky/contracts/server";

const JOIN_TOKEN_TTL_SECONDS = 60;

interface LiveKitConfiguration {
  apiKey: string;
  apiSecret: string;
  serverUrl: string;
  e2eeMasterKey: Buffer;
}

export interface EncounterJoinCredential {
  serverUrl: string;
  token: string;
  e2eeKey: string;
  expiresInSeconds: number;
}

function requiredConfiguration(): LiveKitConfiguration {
  const apiKey = process.env.LIVEKIT_API_KEY?.trim();
  const apiSecret = process.env.LIVEKIT_API_SECRET?.trim();
  const serverUrl = process.env.NEXT_PUBLIC_LIVEKIT_URL?.trim();
  const encodedMasterKey = process.env.LIVEKIT_E2EE_MASTER_KEY?.trim();
  if (!apiKey || !apiSecret || !serverUrl || !encodedMasterKey) {
    throw new Error("LiveKit private configuration is unavailable.");
  }
  const parsedUrl = new URL(serverUrl);
  if (!["wss:", "https:"].includes(parsedUrl.protocol)) {
    throw new Error("LiveKit must use WSS or HTTPS.");
  }
  const e2eeMasterKey = Buffer.from(encodedMasterKey, "base64url");
  if (e2eeMasterKey.byteLength < 32) {
    throw new Error("LiveKit E2EE master key is too short.");
  }
  return {
    apiKey,
    apiSecret,
    serverUrl: parsedUrl.toString(),
    e2eeMasterKey,
  };
}

function roomName(sessionId: string): string {
  return `encounter-${sessionId}`;
}

function deriveRoomKey(
  masterKey: Buffer,
  sessionId: string,
  grantNonce: string,
): string {
  return createHmac("sha256", masterKey)
    .update("APOCKY-ENCOUNTER-E2EE-v1\0", "utf8")
    .update(sessionId, "utf8")
    .update("\0", "utf8")
    .update(grantNonce, "utf8")
    .digest("base64url");
}

export async function mintOwnerJoinCredential(
  authority: EncounterAuthorityBundle,
): Promise<EncounterJoinCredential> {
  const configuration = requiredConfiguration();
  const accessToken = new AccessToken(
    configuration.apiKey,
    configuration.apiSecret,
    {
      identity: authority.ownerIdentity.principal,
      name: authority.ownerIdentity.displayName,
      ttl: JOIN_TOKEN_TTL_SECONDS,
      attributes: {
        "apocky.role": "owner",
        "apocky.grant": authority.grantDigest,
      },
    },
  );
  accessToken.addGrant({
    roomJoin: true,
    room: roomName(authority.grant.sessionId),
    canPublish: true,
    canSubscribe: true,
    canPublishData: true,
    canUpdateOwnMetadata: false,
  });
  return {
    serverUrl: configuration.serverUrl,
    token: await accessToken.toJwt(),
    e2eeKey: deriveRoomKey(
      configuration.e2eeMasterKey,
      authority.grant.sessionId,
      authority.grant.nonce,
    ),
    expiresInSeconds: JOIN_TOKEN_TTL_SECONDS,
  };
}
