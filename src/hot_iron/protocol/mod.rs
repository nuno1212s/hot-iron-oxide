use crate::decision_tree::{DecisionHandler, DecisionNodeHeader, TQuorumCertificate};
use atlas_common::crypto::threshold_crypto::CombinedSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use getset::{CopyGetters, Getters};
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use strum::EnumCount;

pub(crate) mod decision;
pub(crate) mod hotstuff;
mod log;
mod msg_queue;
pub mod proof;

pub type HotIronDecisionHandler = DecisionHandler<QC>;

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, PartialEq, Eq, Hash, Copy, EnumCount)]
pub enum QCType {
    PrepareVote,
    PreCommitVote,
    CommitVote,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Getters, CopyGetters, Hash, PartialEq, Eq)]
/// A quorum certificate
pub struct QC {
    #[get_copy = "pub"]
    qc_type: QCType,
    #[get_copy = "pub"]
    view_seq: SeqNo,
    #[get_copy = "pub"]
    decision_node: DecisionNodeHeader,
    // The combined signature of the quorum which voted for this QC
    #[get = "pub"]
    signature: CombinedSignature,
}

impl QC {
    #[must_use]
    pub fn new(
        qc_type: QCType,
        view_seq: SeqNo,
        decision_node: DecisionNodeHeader,
        signature: CombinedSignature,
    ) -> Self {
        QC {
            qc_type,
            view_seq,
            decision_node,
            signature,
        }
    }
}

impl TQuorumCertificate for QC {
    fn decision_node(&self) -> &DecisionNodeHeader {
        &self.decision_node
    }

    fn signature(&self) -> &CombinedSignature {
        &self.signature
    }
}

impl Orderable for QC {
    fn sequence_number(&self) -> SeqNo {
        self.view_seq
    }
}

impl Debug for QC {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "QC: view_seq: {:?}, decision_node: {:?}",
            self.view_seq, self.decision_node
        )
    }
}
