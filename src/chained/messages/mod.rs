pub mod serialize;

use crate::chained::chained_decision_tree::ChainedDecisionNode;
use crate::chained::ChainedQC;
use crate::decision_tree::{DecisionNode, DecisionNodeHeader};
use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};

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
    pub vote_details: VoteDetails,
    pub signature: PartialSignature,
}

impl VoteMessage {
    pub fn new(vote_details: VoteDetails, signature: PartialSignature) -> Self {
        Self {
            vote_details,
            signature,
        }
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
pub struct VoteDetails {
    #[get = "pub"]
    pub node: DecisionNodeHeader,
    pub justify: Option<ChainedQC>,
}

impl VoteDetails {
    pub fn new(node: DecisionNodeHeader, justify: Option<ChainedQC>) -> Self {
        Self { node, justify }
    }
    
    pub fn justify(&self) -> Option<&ChainedQC> {
        self.justify.as_ref()
    }
    
}

impl Orderable for VoteDetails {
    fn sequence_number(&self) -> SeqNo {
        self.node.sequence_number()
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct NewViewMessage {
    pub qc: Option<ChainedQC>,
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
    pub(super) proposal: ChainedDecisionNode<D>,
}

impl<D> ProposalMessage<D> {
    pub fn new(proposal: ChainedDecisionNode<D>) -> Self {
        Self { proposal }
    }
}
