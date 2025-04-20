pub(super) mod serialize;

use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use crate::chained::chained_decision_tree::ChainedDecisionNode;
use crate::chained::ChainedQC;
use crate::decision_tree::{DecisionNode, DecisionNodeHeader};

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct IronChainMessage<D> {
    seq_no: SeqNo,
    message: IronChainMessageType<D>,
}

impl<D> IronChainMessage<D> {
    pub fn new(seq_no: SeqNo, message: IronChainMessageType<D>) -> Self {
        Self { seq_no, message }
    }
}

impl<RQ> Orderable for IronChainMessage<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.seq_no
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq)]
pub enum IronChainMessageType<D> {
    Proposal(ProposalMessage<D>),
    Vote(VoteMessage),
    NewView(NewViewMessage),
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct VoteMessage {
    pub node: DecisionNodeHeader,
    pub signature: PartialSignature,
}

impl VoteMessage {
    pub fn new( node: DecisionNodeHeader, signature: PartialSignature) -> Self {
        Self { node, signature }
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct NewViewMessage {
    pub qc: Option<ChainedQC>
}

impl NewViewMessage {
    
    pub fn from_previous_qc(qc: ChainedQC) -> Self {
        Self { qc: Some(qc) }
    }
    
    pub fn from_empty() -> Self {
        Self { qc: None }
    }
    
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct ProposalMessage<D> {
    pub(in super) proposal: ChainedDecisionNode<D>
}

impl<D> ProposalMessage<D> {
    pub fn new(proposal: ChainedDecisionNode<D>) -> Self {
        Self { proposal }
    }
}
