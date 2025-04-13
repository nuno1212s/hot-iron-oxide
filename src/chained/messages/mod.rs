pub(super) mod serialize;

use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use crate::chained::ChainedQC;
use crate::decision_tree::DecisionNode;

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct IronChainMessage<D> {
    seq_no: SeqNo,
    message: IronChainMessageType<D>,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq)]
pub enum IronChainMessageType<D> {
    Proposal(ProposalMessage<D>),
    Vote(VoteMessage),
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct VoteMessage {
    pub vote: Option<ChainedQC>,
    pub signature: PartialSignature,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct ProposalMessage<D> {
    proposal: DecisionNode<D>,
    qc: ChainedQC,
}

impl<D> IronChainMessage<D> {
    pub fn new(seq_no: SeqNo, message: IronChainMessageType<D>) -> Self {
        Self { seq_no, message }
    }
}

impl VoteMessage {
    pub fn new(vote: Option<ChainedQC>, signature: PartialSignature) -> Self {
        Self { vote, signature }
    }
}

impl<D> ProposalMessage<D> {
    pub fn new(proposal: DecisionNode<D>, qc: ChainedQC) -> Self {
        Self { proposal, qc }
    }
}

impl<RQ> Orderable for IronChainMessage<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.seq_no
    }
}

