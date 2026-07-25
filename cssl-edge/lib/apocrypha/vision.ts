import { isOpaqueClientRequestId } from '@/lib/apocrypha/proxy';

const FORBIDDEN_RAW_KEYS = new Set([
  'content_b64',
  'frame_bytes',
  'raw_frame_bytes',
  'image_data',
  'data_uri',
]);

export const VISION_SOURCE_REF = 'public:apocky.com/vision';
export const VISION_PURPOSE = 'webcam_perception';

export function isVisionSessionRef(value: unknown): value is string {
  return isOpaqueClientRequestId(value);
}

export function isVisionConsentId(value: unknown): value is string {
  return isOpaqueClientRequestId(value);
}

export function isVisionControlEvent(value: unknown): value is 'pause' | 'resume' | 'revoke' | 'close' {
  return value === 'pause' || value === 'resume' || value === 'revoke' || value === 'close';
}

export function visionPayloadIsMetadataOnly(value: unknown): boolean {
  if (!value || typeof value !== 'object') return true;
  if (Array.isArray(value)) return value.every(visionPayloadIsMetadataOnly);
  for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
    if (FORBIDDEN_RAW_KEYS.has(key)) return false;
    if (!visionPayloadIsMetadataOnly(child)) return false;
  }
  return true;
}
