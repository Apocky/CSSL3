// § T11-W11-SIGMA-CHAIN-FEDERATION : peer-sync skeleton (single-node-mode default)
// §§ stage-0 : Mode::SingleNode is canonical · LoA.exe writes its own ledger
// §§ next-wave : Mode::Federated · each peer maintains own ledger · Σ-mask-gated cross-anchoring
// §§ design-note : federation = Σ-mask-gated cross-anchoring of CHECKPOINT-ROOTS only ;
//     full ledger never shared cross-peer ; privacy-default per spec/14 axioms.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::chain::Checkpoint;

/// Operating mode for the chain · stage-0 default = SingleNode.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[derive(Serialize, Deserialize)]
pub enum Mode {
    /// Single-node : LoA.exe owns the ledger · no peer-sync · default.
    SingleNode,
    /// Federated : multi-peer cross-anchoring of checkpoint-roots · multi-peer-sync.
    /// §§ NEXT-WAVE : not yet implemented · skeleton only.
    Federated,
}

impl Default for Mode {
    fn default() -> Self {
        Self::SingleNode
    }
}

/// Identifier for a federation peer · 32-byte pubkey-fingerprint.
pub type PeerId = [u8; 32];

/// One peer's published checkpoint · what gets cross-anchored.
///
/// § cross-anchor : peer A pulls peer B's CheckpointPublication ;
///     verifies B's sig over B's checkpoint-root ;
///     emits FederationAnchor entry locally with B's root + B's checkpoint-seq.
/// §§ no full ledger ever flows cross-peer.
/// §§ serde-note : `peer_signature: [u8; 64]` exceeds serde's auto-impl ceiling ;
///     wire-format use `Federation::checkpoint_pub_canonical_bytes()` + manual sig append.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointPublication {
    pub peer_id: PeerId,
    pub epoch: u64,
    pub seq_no: u64,
    pub root: [u8; 32],
    /// Ed25519 sig from peer over (peer_id | epoch | seq_no | root)
    pub peer_signature: [u8; 64],
}

/// Federation manager · skeleton only in stage-0.
#[derive(Default)]
pub struct Federation {
    mode: Mode,
    known_peers: Vec<PeerId>,
    pulled_checkpoints: Vec<CheckpointPublication>,
}

impl Federation {
    /// § new federation in single-node-mode · default for stage-0.
    #[must_use]
    pub fn single_node() -> Self {
        Self::default()
    }

    /// § switch to federated-mode · stage-0 = no-op skeleton ; future-wave wires peer-sync.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// § current mode.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// § register a peer · must be Σ-mask-gated by caller (spec/14 § PRIVACY).
    pub fn add_peer(&mut self, peer: PeerId) {
        if !self.known_peers.contains(&peer) {
            self.known_peers.push(peer);
        }
    }

    /// § list of peer-IDs known to this node.
    #[must_use]
    pub fn known_peers(&self) -> &[PeerId] {
        &self.known_peers
    }

    /// § publish OUR checkpoint for peers to pull.
    /// §§ stage-0 : just packages the data ; transport layer is next-wave.
    #[must_use]
    pub fn publish_checkpoint(
        &self,
        peer_id: PeerId,
        cp: &Checkpoint,
        peer_signature: [u8; 64],
    ) -> CheckpointPublication {
        CheckpointPublication {
            peer_id,
            epoch: cp.epoch,
            seq_no: cp.seq_no,
            root: cp.root,
            peer_signature,
        }
    }

    /// § record a pulled checkpoint from a peer · stage-0 = in-memory only.
    /// §§ caller-must-verify peer_signature before calling this ;
    ///     this fn does not verify (separation-of-concerns).
    pub fn record_peer_checkpoint(&mut self, pub_: CheckpointPublication) {
        if !self.known_peers.contains(&pub_.peer_id) {
            self.known_peers.push(pub_.peer_id);
        }
        self.pulled_checkpoints.push(pub_);
    }

    /// § pulled checkpoints across all peers.
    #[must_use]
    pub fn pulled_checkpoints(&self) -> &[CheckpointPublication] {
        &self.pulled_checkpoints
    }

    /// § canonical-bytes for peer-signature verification · domain-separated.
    #[must_use]
    pub fn checkpoint_pub_canonical_bytes(pub_: &CheckpointPublication) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(80);
        bytes.extend_from_slice(b"cssl-substrate-sigma-chain/v0/peer-cp");
        bytes.extend_from_slice(&pub_.peer_id);
        bytes.extend_from_slice(&pub_.epoch.to_be_bytes());
        bytes.extend_from_slice(&pub_.seq_no.to_be_bytes());
        bytes.extend_from_slice(&pub_.root);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_single_node() {
        let f = Federation::single_node();
        assert_eq!(f.mode(), Mode::SingleNode);
        assert!(f.known_peers().is_empty());
    }

    #[test]
    fn add_peer_dedupes() {
        let mut f = Federation::single_node();
        let p = [1u8; 32];
        f.add_peer(p);
        f.add_peer(p);
        f.add_peer(p);
        assert_eq!(f.known_peers().len(), 1);
    }

    #[test]
    fn record_peer_checkpoint_registers_peer() {
        let mut f = Federation::single_node();
        let pub_ = CheckpointPublication {
            peer_id: [9u8; 32],
            epoch: 1,
            seq_no: 1024,
            root: [7u8; 32],
            peer_signature: [0u8; 64],
        };
        f.record_peer_checkpoint(pub_);
        assert_eq!(f.known_peers().len(), 1);
        assert_eq!(f.pulled_checkpoints().len(), 1);
    }
}
