//! Delivery, ordering, fork detection, and repair facades around MLS state.
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Delivery errors.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum DeliveryError {
    #[error("divergent tree hash at epoch {0}")]
    DivergentTree(u64),
}

/// Compact state summary exchanged during catch-up.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochSummary {
    pub epoch: u64,
    pub tree_hash: [u8; 32],
    pub confirmation_tag: [u8; 32],
}

/// Compare two summaries and return whether repair is required.
#[must_use]
pub fn needs_repair(local: &EpochSummary, remote: &EpochSummary) -> bool {
    local.epoch == remote.epoch
        && (local.tree_hash != remote.tree_hash
            || local.confirmation_tag != remote.confirmation_tag)
}

/// Repair action: rejoin first, then re-propose valid app events only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RepairAction {
    None,
    RejoinAndReproposal { coordinator_leaf: u32 },
}

/// Select a deterministic repair coordinator from last common accepted leaf indexes.
#[must_use]
pub fn select_repair_action(diverged: bool, leaves: &[u32]) -> RepairAction {
    if !diverged {
        return RepairAction::None;
    }
    RepairAction::RejoinAndReproposal {
        coordinator_leaf: leaves.iter().copied().max().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_same_epoch_divergence() {
        let a = EpochSummary {
            epoch: 2,
            tree_hash: [1; 32],
            confirmation_tag: [2; 32],
        };
        let b = EpochSummary {
            epoch: 2,
            tree_hash: [9; 32],
            confirmation_tag: [2; 32],
        };
        assert!(needs_repair(&a, &b));
        assert_eq!(
            select_repair_action(true, &[1, 7, 3]),
            RepairAction::RejoinAndReproposal {
                coordinator_leaf: 7
            }
        );
    }
}
