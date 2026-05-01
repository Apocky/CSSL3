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

// =====================================================================
// § T11-W4-SUPABASE-SIGNALING · multiplayer signaling types
// Appended for migrations 0004 + 0005 + 0006.
// =====================================================================

// ---------------------------------------------------------------------
// Enums + literals (signaling)
// ---------------------------------------------------------------------
export type SignalingKind =
  | "offer"
  | "answer"
  | "ice"
  | "hello"
  | "ping"
  | "pong"
  | "bye"
  | "custom";

/**
 * Wildcard fan-out target — `*` broadcasts a signaling message to every
 * peer in the room. Always paired with `to_peer` in `SignalingMessageRow`.
 */
export type PeerAddress = string | "*";

// ---------------------------------------------------------------------
// public.multiplayer_rooms
// ---------------------------------------------------------------------
export interface MultiplayerRoomRow {
  id: Uuid;
  code: string;
  host_player_id: string;
  created_at: Timestamptz;
  expires_at: Timestamptz;
  max_peers: number;
  is_open: boolean;
  meta: Jsonb;
}

export type MultiplayerRoomInsert = Omit<
  MultiplayerRoomRow,
  "id" | "created_at" | "expires_at" | "max_peers" | "is_open" | "meta"
> & {
  id?: Uuid;
  created_at?: Timestamptz;
  expires_at?: Timestamptz;
  max_peers?: number;
  is_open?: boolean;
  meta?: Jsonb;
};

export type MultiplayerRoomUpdate = Partial<MultiplayerRoomInsert>;

/** Convenience alias matching the requested public surface. */
export type MultiplayerRoom = MultiplayerRoomRow;

// ---------------------------------------------------------------------
// public.room_peers
// ---------------------------------------------------------------------
export interface RoomPeerRow {
  id: Uuid;
  room_id: Uuid;
  player_id: string;
  display_name: string | null;
  joined_at: Timestamptz;
  last_seen_at: Timestamptz;
  is_host: boolean;
}

export type RoomPeerInsert = Omit<
  RoomPeerRow,
  "id" | "joined_at" | "last_seen_at" | "is_host"
> & {
  id?: Uuid;
  joined_at?: Timestamptz;
  last_seen_at?: Timestamptz;
  is_host?: boolean;
};

export type RoomPeerUpdate = Partial<RoomPeerInsert>;

/** Convenience alias matching the requested public surface. */
export type RoomPeer = RoomPeerRow;

// ---------------------------------------------------------------------
// public.signaling_messages
// ---------------------------------------------------------------------
export interface SignalingMessageRow {
  id: number;          // bigserial maps to number (53-bit safe; consider bigint for >2^53)
  room_id: Uuid;
  from_peer: string;
  to_peer: PeerAddress;
  kind: SignalingKind;
  payload: Jsonb;
  created_at: Timestamptz;
  delivered: boolean;
}

export type SignalingMessageInsert = Omit<
  SignalingMessageRow,
  "id" | "created_at" | "delivered"
> & {
  id?: number;
  created_at?: Timestamptz;
  delivered?: boolean;
};

export type SignalingMessageUpdate = Partial<
  Pick<SignalingMessageRow, "delivered">
>;

/** Convenience alias matching the requested public surface. */
export type SignalingMessage = SignalingMessageRow;

// ---------------------------------------------------------------------
// public.room_state_snapshots
// ---------------------------------------------------------------------
export interface RoomStateSnapshotRow {
  id: number;
  room_id: Uuid;
  seq: number;
  created_by: string;
  state: Jsonb;
  created_at: Timestamptz;
}

export type RoomStateSnapshotInsert = Omit<
  RoomStateSnapshotRow,
  "id" | "created_at"
> & {
  id?: number;
  created_at?: Timestamptz;
};

/** Convenience alias matching the requested public surface. */
export type RoomStateSnapshot = RoomStateSnapshotRow;

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
      multiplayer_rooms: {
        Row: MultiplayerRoomRow;
        Insert: MultiplayerRoomInsert;
        Update: MultiplayerRoomUpdate;
      };
      room_peers: {
        Row: RoomPeerRow;
        Insert: RoomPeerInsert;
        Update: RoomPeerUpdate;
      };
      signaling_messages: {
        Row: SignalingMessageRow;
        Insert: SignalingMessageInsert;
        Update: SignalingMessageUpdate;
      };
      room_state_snapshots: {
        Row: RoomStateSnapshotRow;
        Insert: RoomStateSnapshotInsert;
        Update: never; // snapshots are append-only
      };
      cocreative_bias_vectors: {
        Row: CocreativeBiasVectorRow;
        Insert: CocreativeBiasVectorInsert;
        Update: CocreativeBiasVectorUpdate;
      };
      cocreative_feedback_events: {
        Row: CocreativeFeedbackEventRow;
        Insert: CocreativeFeedbackEventInsert;
        Update: never; // feedback is append-only
      };
      cocreative_optimizer_snapshots: {
        Row: CocreativeOptimizerSnapshotRow;
        Insert: CocreativeOptimizerSnapshotInsert;
        Update: never; // snapshots are append-only
      };
      game_state_snapshots: {
        Row: GameStateSnapshotRow;
        Insert: GameStateSnapshotInsert;
        Update: never; // append-only · DELETE allowed for GDPR erasure but not UPDATE
      };
      game_session_index: {
        Row: GameSessionIndexRow;
        Insert: GameSessionIndexInsert;
        Update: GameSessionIndexUpdate;
      };
      sovereign_cap_audit: {
        Row: SovereignCapAuditRow;
        Insert: SovereignCapAuditInsert;
        Update: never; // INSERT-ONLY · transparency invariant
      };
    };
    Views: {
      sovereign_cap_audit_summary: {
        Row: SovereignCapAuditSummaryRow;
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
      gen_room_code: {
        Args: Record<string, never>;
        Returns: string;
      };
      cleanup_expired_rooms: {
        Args: Record<string, never>;
        Returns: number;
      };
      presence_touch: {
        Args: { p_room: Uuid; p_player: string };
        Returns: Timestamptz;
      };
      current_user_id: {
        Args: Record<string, never>;
        Returns: string | null;
      };
      update_bias_with_step: {
        Args: {
          p_bias_id: Uuid;
          p_new_theta: Jsonb;
          p_step_count: number;
          p_loss: number;
          p_grad: number;
        };
        Returns: Timestamptz;
      };
      latest_snapshot_for_player: {
        Args: { p_player_id: string };
        Returns: CocreativeOptimizerSnapshotRow[];
      };
      record_snapshot: {
        Args: {
          p_session: Uuid | null;
          p_player: string;
          p_scene: Jsonb;
          p_digest: string;
          p_url: string | null;
          p_history: Jsonb;
        };
        Returns: number; // bigserial id of new snapshot row
      };
      latest_snapshot: {
        Args: { p_session: Uuid };
        Returns: GameStateSnapshotRow[];
      };
      end_session: {
        Args: { p_session: Uuid };
        Returns: Timestamptz | null;
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

// =====================================================================
// § T11-W4-SUPABASE-SIGNALING · channel-name builders
// =====================================================================
/** Per-room realtime channel name (filter signaling_messages by room_id). */
export function roomChannelName(roomId: Uuid): string {
  return `room:${roomId}`;
}

/** Per-peer realtime channel name (subscribe to messages addressed to me). */
export function peerChannelName(roomId: Uuid, peerId: string): string {
  return `room:${roomId}:peer:${peerId}`;
}

// =====================================================================
// § T11-W5b-SUPABASE-COCREATIVE · cocreative cross-session learning types
// Appended for migrations 0007 + 0008 + 0009.
// =====================================================================

// ---------------------------------------------------------------------
// Enums + literals (cocreative)
// ---------------------------------------------------------------------
export type CocreativeFeedbackKind =
  | "thumbs_up"
  | "thumbs_down"
  | "scalar_score"
  | "comment";

/**
 * Bias-vector parameter array θ ∈ R^dim. Stored as `Jsonb` (a JSON array of
 * numbers). The host crate (`cssl-host-cocreative`) deserializes this into
 * a fixed-size f32 buffer of length `dim`.
 */
export type BiasTheta = number[];

// ---------------------------------------------------------------------
// public.cocreative_bias_vectors
// ---------------------------------------------------------------------
export interface CocreativeBiasVectorRow {
  id: Uuid;
  player_id: string;
  dim: number;
  theta: Jsonb; // BiasTheta serialized as Jsonb array
  lr: number;
  momentum_decay: number;
  step_count: number;
  last_loss: number | null;
  last_grad_l2: number | null;
  created_at: Timestamptz;
  updated_at: Timestamptz;
}

export type CocreativeBiasVectorInsert = Omit<
  CocreativeBiasVectorRow,
  | "id"
  | "lr"
  | "momentum_decay"
  | "step_count"
  | "last_loss"
  | "last_grad_l2"
  | "created_at"
  | "updated_at"
> & {
  id?: Uuid;
  lr?: number;
  momentum_decay?: number;
  step_count?: number;
  last_loss?: number | null;
  last_grad_l2?: number | null;
  created_at?: Timestamptz;
  updated_at?: Timestamptz;
};

export type CocreativeBiasVectorUpdate = Partial<CocreativeBiasVectorInsert>;

/** Convenience alias matching the requested public surface. */
export type BiasVector = CocreativeBiasVectorRow;

// ---------------------------------------------------------------------
// public.cocreative_feedback_events
// ---------------------------------------------------------------------
export interface CocreativeFeedbackEventRow {
  id: number; // bigserial
  player_id: string;
  bias_id: Uuid | null;
  kind: CocreativeFeedbackKind;
  target_label: string;
  scene_features: Jsonb;
  score: number | null;       // present only when kind === "scalar_score"
  comment_text: string | null; // present only when kind === "comment"
  recorded_at: Timestamptz;
}

export type CocreativeFeedbackEventInsert = Omit<
  CocreativeFeedbackEventRow,
  "id" | "recorded_at"
> & {
  id?: number;
  recorded_at?: Timestamptz;
};

export type CocreativeFeedbackEventUpdate = Partial<
  Pick<CocreativeFeedbackEventRow, "score" | "comment_text">
>;

/** Convenience alias matching the requested public surface. */
export type FeedbackEvent = CocreativeFeedbackEventRow;

// ---------------------------------------------------------------------
// public.cocreative_optimizer_snapshots
// ---------------------------------------------------------------------
export interface CocreativeOptimizerSnapshotRow {
  id: number; // bigserial
  bias_id: Uuid;
  seq: number;
  theta: Jsonb; // BiasTheta serialized as Jsonb array
  step_count: number;
  last_loss: number | null;
  created_at: Timestamptz;
}

export type CocreativeOptimizerSnapshotInsert = Omit<
  CocreativeOptimizerSnapshotRow,
  "id" | "created_at"
> & {
  id?: number;
  created_at?: Timestamptz;
};

export type CocreativeOptimizerSnapshotUpdate = never; // append-only

/** Convenience alias matching the requested public surface. */
export type OptimizerSnapshot = CocreativeOptimizerSnapshotRow;

// =====================================================================
// § T11-W5b-SUPABASE-COCREATIVE · channel-name builders
// =====================================================================
/**
 * Per-player realtime channel name. cssl-host-cocreative subscribes
 * to this channel to receive cross-device updates to its bias-vector
 * (e.g. when the player drives feedback on a second device).
 */
export function cocreativeChannelName(playerId: string): string {
  return `cocreative:${playerId}`;
}

// =====================================================================
// § T11-W5c-SUPABASE-GAMESTATE · cross-session game-state types
// Appended for migrations 0010 + 0011 + 0012 + 0013.
// =====================================================================

/**
 * One entry in `game_state_snapshots.companion_history`. The host crate
 * appends one of these on every sovereign-cap operation that occurred
 * in the session up to the snapshot's `created_at`.
 */
export interface CompanionHistoryEntry {
  ts: Timestamptz;
  sovereign_handle: string;
  op: CompanionOperation;
  params: Jsonb | null;
}

// ---------------------------------------------------------------------
// public.game_session_index
// ---------------------------------------------------------------------
export interface GameSessionIndexRow {
  session_id: Uuid;
  player_id: string;
  started_at: Timestamptz;
  ended_at: Timestamptz | null;   // NULL = active
  latest_seq: number;
  total_snapshots: number;
  meta: Jsonb;
}

export type GameSessionIndexInsert = Omit<
  GameSessionIndexRow,
  | "session_id"
  | "started_at"
  | "ended_at"
  | "latest_seq"
  | "total_snapshots"
  | "meta"
> & {
  session_id?: Uuid;
  started_at?: Timestamptz;
  ended_at?: Timestamptz | null;
  latest_seq?: number;
  total_snapshots?: number;
  meta?: Jsonb;
};

export type GameSessionIndexUpdate = Partial<
  Pick<GameSessionIndexRow,
       "ended_at" | "latest_seq" | "total_snapshots" | "meta">
>;

/** Convenience alias matching the requested public surface. */
export type GameSession = GameSessionIndexRow;

// ---------------------------------------------------------------------
// public.game_state_snapshots
// ---------------------------------------------------------------------
export interface GameStateSnapshotRow {
  id: number; // bigserial
  session_id: Uuid;
  player_id: string;
  seq: number;
  scene_graph: Jsonb;
  /** sha256-hex (64 lowercase hex chars) of the ω-field tensor at capture-time. */
  omega_field_digest: string;
  /** Optional pointer to the full ω-field bytes in storage; NULL = regenerable. */
  omega_field_url: string | null;
  /** JSONB array of CompanionHistoryEntry serialized as Jsonb. */
  companion_history: Jsonb;
  created_at: Timestamptz;
}

export type GameStateSnapshotInsert = Omit<
  GameStateSnapshotRow,
  "id" | "session_id" | "created_at" | "companion_history"
> & {
  id?: number;
  session_id?: Uuid;
  created_at?: Timestamptz;
  companion_history?: Jsonb;
};

/** Snapshots are append-only ; UPDATE is not permitted. */
export type GameStateSnapshotUpdate = never;

/** Convenience alias matching the requested public surface. */
export type GameStateSnapshot = GameStateSnapshotRow;

// ---------------------------------------------------------------------
// public.sovereign_cap_audit
// ---------------------------------------------------------------------
export type SovereignCapKind =
  | "rate_limit"
  | "external_io"
  | "content_filter"
  | "harm_check"
  | "consent_gate"
  | (string & {}); // open-ended

export type SovereignActionKind =
  | "companion.spawn"
  | "companion.modify"
  | "render.snapshot_png"
  | "mutate_world"
  | "force-skip-paywall"
  | (string & {}); // open-ended

export type SovereignCallerOrigin =
  | "mcp:companion"
  | "mcp:render.snapshot_png"
  | "cli:csslc"
  | "ide:lsp"
  | (string & {}); // open-ended

export interface SovereignCapAuditRow {
  id: number; // bigserial
  session_id: Uuid;
  player_id: string;
  ts: Timestamptz;
  action_kind: SovereignActionKind;
  cap_bypassed_kind: SovereignCapKind;
  reason: string;
  target_audit_event_id: string | null;
  caller_origin: SovereignCallerOrigin;
}

export type SovereignCapAuditInsert = Omit<
  SovereignCapAuditRow,
  "id" | "ts" | "target_audit_event_id"
> & {
  id?: number;
  ts?: Timestamptz;
  target_audit_event_id?: string | null;
};

/** INSERT-ONLY · transparency invariant. UPDATE/DELETE prohibited. */
export type SovereignCapAuditUpdate = never;

/** Convenience alias matching the requested public surface. */
export type SovereignCapAuditEvent = SovereignCapAuditRow;

// ---------------------------------------------------------------------
// public.sovereign_cap_audit_summary (VIEW)
// ---------------------------------------------------------------------
export interface SovereignCapAuditSummaryRow {
  player_id: string;
  action_kind: SovereignActionKind;
  uses: number;
  first_use: Timestamptz;
  last_use: Timestamptz;
}

// =====================================================================
// § T11-W5c-SUPABASE-GAMESTATE · channel-name builders
// =====================================================================
/**
 * Per-session realtime channel name. The DM-engine subscribes to this
 * channel to receive cross-device updates to game-state (e.g. when the
 * player has the same session active on a second device).
 */
export function gameSessionChannelName(sessionId: Uuid): string {
  return `gamestate:${sessionId}`;
}

/**
 * Per-player realtime channel for sovereign-cap audit events. The
 * transparency UI subscribes to this channel to surface new entries
 * in real time.
 */
export function sovereignAuditChannelName(playerId: string): string {
  return `sovereign-audit:${playerId}`;
}
