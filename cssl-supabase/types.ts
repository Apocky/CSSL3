// =====================================================================
// § T11-WAVE3-SUPABASE · types.ts
// TypeScript types for the cssl-supabase schema.
// Used by cssl-edge / Vercel functions / browser clients.
//
// Manually authored to match migrations/0001_initial.sql.
// To regenerate from a live Supabase project use:
//   npx supabase gen types typescript --project-id <ref> > types.generated.ts
// =====================================================================

// ---------------------------------------------------------------------
// Branded scalar types
// ---------------------------------------------------------------------
export type Uuid = string & { readonly __brand: "uuid" };
export type Timestamptz = string & { readonly __brand: "timestamptz" };
export type Jsonb =
  | string
  | number
  | boolean
  | null
  | { [k: string]: Jsonb }
  | Jsonb[];

// ---------------------------------------------------------------------
// Enums + literals
// ---------------------------------------------------------------------
export type AssetFormat = "glb" | "gltf" | "obj" | "fbx" | "usdz" | "ply" | "stl";

export type AssetSource =
  | "sketchfab"
  | "polyhaven"
  | "kenney"
  | "stanford-3d-scan"
  | "nasa-3d"
  | "khronos-glTF-samples"
  | "public-domain"
  | (string & {}); // open-ended : accept future sources

export type LicenseId =
  | "CC0"
  | "CC-BY-4.0"
  | "CC-BY-SA-4.0"
  | "CC-BY-NC-4.0"
  | "public-domain"
  | "All-Rights-Reserved"
  | (string & {}); // open-ended

export type CompanionOperation =
  | "spawn"
  | "modify"
  | "query"
  | "destroy"
  | "imbue"
  | "summon"
  | (string & {}); // open-ended

// ---------------------------------------------------------------------
// public.assets
// ---------------------------------------------------------------------
export interface AssetRow {
  id: Uuid;
  source: AssetSource;
  source_id: string;
  name: string;
  license: LicenseId;
  attribution: string | null;
  format: AssetFormat;
  storage_url: string | null;
  upstream_url: string;
  metadata: Jsonb | null;
  bytes: number | null;
  created_at: Timestamptz;
  indexed_at: Timestamptz | null;
}

export type AssetInsert = Omit<AssetRow, "id" | "created_at" | "indexed_at"> & {
  id?: Uuid;
  created_at?: Timestamptz;
  indexed_at?: Timestamptz | null;
};

export type AssetUpdate = Partial<AssetInsert>;

// ---------------------------------------------------------------------
// public.scenes
// ---------------------------------------------------------------------
export interface SceneRow {
  id: Uuid;
  user_id: Uuid;
  name: string;
  description: string | null;
  seed_text: string | null;
  scene_graph: Jsonb;
  is_public: boolean;
  play_count: number;
  created_at: Timestamptz;
  updated_at: Timestamptz;
}

export type SceneInsert = Omit<
  SceneRow,
  "id" | "created_at" | "updated_at" | "play_count"
> & {
  id?: Uuid;
  is_public?: boolean;
  play_count?: number;
  created_at?: Timestamptz;
  updated_at?: Timestamptz;
};

export type SceneUpdate = Partial<SceneInsert>;

// ---------------------------------------------------------------------
// public.history
// ---------------------------------------------------------------------
export interface HistoryRow {
  id: Uuid;
  user_id: Uuid | null;        // nullable : anonymous opt-in
  seed_text: string;
  scene_graph: Jsonb | null;
  success: boolean | null;
  user_rating: number | null;  // 1..5
  created_at: Timestamptz;
}

export type HistoryInsert = Omit<HistoryRow, "id" | "created_at"> & {
  id?: Uuid;
  created_at?: Timestamptz;
};

// ---------------------------------------------------------------------
// public.companion_logs
// ---------------------------------------------------------------------
export interface CompanionLogRow {
  id: Uuid;
  user_id: Uuid;
  sovereign_handle: string;
  operation: CompanionOperation;
  params: Jsonb | null;
  accepted: boolean;
  refusal_reason: string | null;
  created_at: Timestamptz;
}

export type CompanionLogInsert = Omit<CompanionLogRow, "id" | "created_at"> & {
  id?: Uuid;
  created_at?: Timestamptz;
};

// ---------------------------------------------------------------------
// Supabase generated-style root type
// ---------------------------------------------------------------------
export interface Database {
  public: {
    Tables: {
      assets: {
        Row: AssetRow;
        Insert: AssetInsert;
        Update: AssetUpdate;
      };
      scenes: {
        Row: SceneRow;
        Insert: SceneInsert;
        Update: SceneUpdate;
      };
      history: {
        Row: HistoryRow;
        Insert: HistoryInsert;
        Update: Partial<HistoryInsert>;
      };
      companion_logs: {
        Row: CompanionLogRow;
        Insert: CompanionLogInsert;
        Update: never; // RLS denies UPDATE
      };
    };
    Functions: {
      scene_record_play: {
        Args: { p_scene_id: Uuid };
        Returns: number;
      };
      companion_log_append: {
        Args: {
          p_sovereign_handle: string;
          p_operation: CompanionOperation;
          p_params: Jsonb | null;
          p_accepted: boolean;
          p_refusal_reason?: string | null;
        };
        Returns: Uuid;
      };
    };
    Enums: Record<string, never>;
  };
}

// ---------------------------------------------------------------------
// Storage bucket identifiers (for path-builder helpers)
// ---------------------------------------------------------------------
export type BucketId = "assets" | "screenshots" | "audio";

export const BUCKET_LIMITS: Record<BucketId, { bytes: number; public: boolean }> = {
  assets: { bytes: 50 * 1024 * 1024, public: true },
  screenshots: { bytes: 10 * 1024 * 1024, public: false },
  audio: { bytes: 10 * 1024 * 1024, public: false },
};

export function assetPath(source: AssetSource, sourceId: string, ext: string): string {
  return `${source}/${sourceId}.${ext}`;
}

export function screenshotPath(userId: Uuid, sceneId: Uuid, ext: "png" | "jpg" | "webp" | "avif"): string {
  return `${userId}/${sceneId}.${ext}`;
}

export function audioPath(userId: Uuid, recordingId: Uuid, ext: "wav" | "mp3" | "ogg" | "webm" | "flac"): string {
  return `${userId}/${recordingId}.${ext}`;
}
