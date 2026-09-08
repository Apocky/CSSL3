/**
 * Public-safe identity for the governed Obsidian Vault memory organ.
 *
 * The vault is a local projection, not a hosted content source.  This module
 * carries only release metadata and policy flags; note bodies, restricted
 * transcript payloads, credentials, and local paths never enter a deployment.
 */
export const APOCRYPHA_VAULT_MEMORY_ORGAN = Object.freeze({
  schema_version: 'apocrypha.obsidian-vault.v1',
  status: 'projection_ready_adapter_open',
  contract_sha256: '1b32bc75731e40819c23021434fdf31045c745d4be9c97c41329e7f3edd49438',
  vault_commit: 'b18e3e9ec49f6fde86427d7c26de527e9f0f692e',
  vault_tree_sha256: 'cf77acc8e3479003211173d4cd6ca787e3b7b736',
  graph_schema: 'aethergraph.v4',
  graph_manifest_sha256: '9390efec04be419d784c977e81830f45a8a205d83ba18e422110562954b93368',
  readable_map: Object.freeze({
    nodes: 1_837,
    node_paths_verified: 1_837,
    direct_readable_links: 39,
    dead_links: 0,
  }),
  privacy: Object.freeze({
    payloads_embedded: false,
    restricted_pointer_only: true,
    private_paths_exposed: false,
  }),
  runtime: Object.freeze({
    adapter_bound: false,
    missing_vault_degrades_typed: false,
    idempotence_verified: false,
    rollback_verified: false,
  }),
} as const);

export type ApocryphaVaultMemoryOrgan = typeof APOCRYPHA_VAULT_MEMORY_ORGAN;
