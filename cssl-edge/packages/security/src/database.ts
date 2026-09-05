export type Json =
  | boolean
  | null
  | number
  | string
  | Json[]
  | { [key: string]: Json | undefined };

type OwnerRow = {
  owner_id: string;
};

export interface Database {
  public: {
    Tables: {
      private_owner_profiles: {
        Row: {
          user_id: string;
          email: string;
          created_at: string;
        };
        Insert: {
          user_id: string;
          email: string;
          created_at?: string;
        };
        Update: {};
        Relationships: [];
      };
      participant_keys: {
        Row: OwnerRow & {
          key_id: string;
          principal: string;
          role: "owner" | "apocrypha";
          public_key_jwk: Json;
          issued_at: string;
          revoked_at: string | null;
          created_at: string;
        };
        Insert: OwnerRow & {
          key_id: string;
          principal: string;
          role: "owner" | "apocrypha";
          public_key_jwk: Json;
          issued_at: string;
          revoked_at?: string | null;
          created_at?: string;
        };
        Update: Partial<{
          revoked_at: string | null;
        }>;
        Relationships: [];
      };
      authority_manifests: {
        Row: OwnerRow & {
          id: string;
          kind: "presence" | "voice";
          digest: string;
          manifest: Json;
          author_principal: string;
          issued_at: string;
          revoked_at: string | null;
          created_at: string;
        };
        Insert: OwnerRow & {
          id?: string;
          kind: "presence" | "voice";
          digest: string;
          manifest: Json;
          author_principal: string;
          issued_at: string;
          revoked_at?: string | null;
          created_at?: string;
        };
        Update: Partial<
          OwnerRow & {
            revoked_at: string | null;
            manifest: Json;
          }
        >;
        Relationships: [];
      };
      encounter_sessions: {
        Row: OwnerRow & {
          id: string;
          state:
            | "lobby"
            | "active"
            | "understanding"
            | "ended_unresolved"
            | "mutually_understood"
            | "revoked";
          grant_digest: string;
          grant_nonce_digest: string;
          grant: Json;
          voice_manifest_digest: string;
          presence_manifest_digest: string;
          retention_policy: Json;
          understanding_version_digest: string | null;
          created_at: string;
          started_at: string | null;
          ended_at: string | null;
        };
        Insert: OwnerRow & {
          id: string;
          state?: "lobby";
          grant_digest: string;
          grant_nonce_digest: string;
          grant: Json;
          voice_manifest_digest: string;
          presence_manifest_digest: string;
          retention_policy: Json;
          understanding_version_digest?: string | null;
          created_at?: string;
          started_at?: string | null;
          ended_at?: string | null;
        };
        Update: Partial<{
          state:
            | "lobby"
            | "active"
            | "understanding"
            | "ended_unresolved"
            | "mutually_understood"
            | "revoked";
          started_at: string | null;
          ended_at: string | null;
          understanding_version_digest: string | null;
        }>;
        Relationships: [];
      };
      encounter_consents: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          participant_principal: string;
          modality: "audio" | "video" | "captions" | "text";
          state: "granted" | "revoked";
          receipt_digest: string;
          receipt: Json;
          created_at: string;
        };
        Insert: OwnerRow & {
          id?: string;
          session_id: string;
          participant_principal: string;
          modality: "audio" | "video" | "captions" | "text";
          state: "granted" | "revoked";
          receipt_digest: string;
          receipt: Json;
          created_at?: string;
        };
        Update: {};
        Relationships: [];
      };
      encounter_readiness: {
        Row: OwnerRow & {
          session_id: string;
          participant_principal: string;
          ready: boolean;
          modalities: string[];
          updated_at: string;
        };
        Insert: OwnerRow & {
          session_id: string;
          participant_principal: string;
          ready: boolean;
          modalities: string[];
          updated_at?: string;
        };
        Update: Partial<{
          ready: boolean;
          modalities: string[];
          updated_at: string;
        }>;
        Relationships: [];
      };
      encounter_join_tokens: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          participant_principal: string;
          token_digest: string;
          issued_at: string;
          expires_at: string;
          revoked_at: string | null;
        };
        Insert: OwnerRow & {
          id: string;
          session_id: string;
          participant_principal: string;
          token_digest: string;
          issued_at: string;
          expires_at: string;
          revoked_at?: string | null;
        };
        Update: Partial<{
          revoked_at: string | null;
        }>;
        Relationships: [];
      };
      understanding_versions: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          version: number;
          canonical_digest: string;
          content: Json;
          created_by: string;
          created_at: string;
        };
        Insert: OwnerRow & {
          id: string;
          session_id: string;
          version: number;
          canonical_digest: string;
          content: Json;
          created_by: string;
          created_at: string;
        };
        Update: {};
        Relationships: [];
      };
      understanding_acknowledgements: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          participant_principal: string;
          version_digest: string;
          status: "understood" | "needs_repair" | "disagree";
          correction: string | null;
          signature: Json;
          acknowledged_at: string;
        };
        Insert: OwnerRow & {
          id: string;
          session_id: string;
          participant_principal: string;
          version_digest: string;
          status: "understood" | "needs_repair" | "disagree";
          correction?: string | null;
          signature: Json;
          acknowledged_at: string;
        };
        Update: {};
        Relationships: [];
      };
      retention_decisions: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          decision_digest: string;
          artifact_classes: string[];
          expires_at: string | null;
          withdrawal_terms: string;
          decision: Json;
          created_at: string;
        };
        Insert: OwnerRow & {
          id: string;
          session_id: string;
          decision_digest: string;
          artifact_classes: string[];
          expires_at?: string | null;
          withdrawal_terms: string;
          decision: Json;
          created_at?: string;
        };
        Update: {};
        Relationships: [];
      };
      retention_acknowledgements: {
        Row: OwnerRow & {
          id: string;
          decision_id: string;
          participant_principal: string;
          decision_digest: string;
          signature: Json;
          acknowledged_at: string;
        };
        Insert: OwnerRow & {
          id?: string;
          decision_id: string;
          participant_principal: string;
          decision_digest: string;
          signature: Json;
          acknowledged_at: string;
        };
        Update: {};
        Relationships: [];
      };
      retained_encounter_artifacts: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          retention_decision_id: string;
          artifact_class: "transcript" | "understanding" | "memory-effects";
          content_digest: string;
          content: Json;
          created_at: string;
        };
        Insert: OwnerRow & {
          id?: string;
          session_id: string;
          retention_decision_id: string;
          artifact_class: "transcript" | "understanding" | "memory-effects";
          content_digest: string;
          content: Json;
          created_at?: string;
        };
        Update: {};
        Relationships: [];
      };
      retention_withdrawals: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          artifact_classes: Array<
            "transcript" | "understanding" | "memory-effects"
          >;
          artifact_digests: string[];
          deleted_artifact_count: number;
          workflow_state: "completed" | "pending_upstream" | "failed";
          upstream_receipt_digest: string | null;
          requested_at: string;
          completed_at: string | null;
        };
        Insert: OwnerRow & {
          id?: string;
          session_id: string;
          artifact_classes: Array<
            "transcript" | "understanding" | "memory-effects"
          >;
          artifact_digests: string[];
          deleted_artifact_count: number;
          workflow_state: "completed" | "pending_upstream" | "failed";
          upstream_receipt_digest?: string | null;
          requested_at?: string;
          completed_at?: string | null;
        };
        Update: {
          workflow_state?: "completed" | "pending_upstream" | "failed";
          upstream_receipt_digest?: string | null;
          completed_at?: string | null;
        };
        Relationships: [];
      };
      encounter_receipts: {
        Row: OwnerRow & {
          id: string;
          session_id: string;
          receipt_digest: string;
          receipt: Json;
          end_state: "ended_unresolved" | "mutually_understood" | "revoked";
          created_at: string;
        };
        Insert: OwnerRow & {
          id: string;
          session_id: string;
          receipt_digest: string;
          receipt: Json;
          end_state: "ended_unresolved" | "mutually_understood" | "revoked";
          created_at?: string;
        };
        Update: {};
        Relationships: [];
      };
      security_audit_receipts: {
        Row: OwnerRow & {
          id: string;
          action: string;
          target: string;
          outcome: "allowed" | "denied" | "completed" | "failed";
          rollback: Json | null;
          metadata: Json;
          receipt_digest: string;
          created_at: string;
        };
        Insert: OwnerRow & {
          id?: string;
          action: string;
          target: string;
          outcome: "allowed" | "denied" | "completed" | "failed";
          rollback?: Json | null;
          metadata: Json;
          receipt_digest: string;
          created_at?: string;
        };
        Update: {};
        Relationships: [];
      };
      deployment_records: {
        Row: OwnerRow & {
          id: string;
          surface: "site" | "encounter" | "ops" | "media";
          environment: "staging" | "production";
          commit_sha: string;
          build_identity: string;
          state: "preview" | "canary" | "promoted" | "rolled_back" | "failed";
          rollback_target: string | null;
          provenance: Json;
          created_at: string;
        };
        Insert: OwnerRow & {
          id?: string;
          surface: "site" | "encounter" | "ops" | "media";
          environment: "staging" | "production";
          commit_sha: string;
          build_identity: string;
          state: "preview" | "canary" | "promoted" | "rolled_back" | "failed";
          rollback_target?: string | null;
          provenance: Json;
          created_at?: string;
        };
        Update: {};
        Relationships: [];
      };
    };
    Views: {};
    Functions: {
      finalize_encounter: {
        Args: {
          p_session_id: string;
          p_end_state:
            | "ended_unresolved"
            | "mutually_understood"
            | "revoked";
          p_receipt_id: string;
          p_receipt_digest: string;
          p_receipt: Json;
          p_ended_at: string;
          p_audit_metadata?: Json;
        };
        Returns: Json;
      };
      withdraw_retained_history: {
        Args: {
          p_session_id: string;
          p_artifact_classes: Array<
            "transcript" | "understanding" | "memory-effects"
          >;
          p_audit_metadata?: Json;
        };
        Returns: Json;
      };
      complete_retention_withdrawal: {
        Args: {
          p_withdrawal_id: string;
          p_upstream_receipt_digest: string;
          p_audit_metadata?: Json;
        };
        Returns: Json;
      };
    };
    Enums: {};
    CompositeTypes: {};
  };
}
