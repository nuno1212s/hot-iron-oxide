pub(super) mod serialize;

use crate::chained::chained_decision_tree::ChainedDecisionNode;
use crate::chained::ChainedQC;
use crate::decision_tree::DecisionNodeHeader;
use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

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

    pub fn into_parts(self) -> (SeqNo, IronChainMessageType<D>) {
        (self.seq_no, self.message)
    }
}

impl<RQ> Orderable for IronChainMessage<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.seq_no
    }
}

impl<RQ> Debug for IronChainMessage<RQ> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "IronChainMessage(seq_no: {:?}, message: {:?})",
            self.seq_no, self.message
        )
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq)]
pub enum IronChainMessageType<D> {
    Proposal(ProposalMessage<D>),
    Vote(VoteMessage),
    NewView(NewViewMessage),
}

impl<D> Debug for IronChainMessageType<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            IronChainMessageType::Proposal(proposal) => write!(f, "Proposal({:?})", proposal),
            IronChainMessageType::Vote(vote) => write!(f, "Vote({:?})", vote),
            IronChainMessageType::NewView(new_view) => write!(f, "NewView({:?})", new_view),
        }
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Debug, Getters)]
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
#[derive(Clone, Hash, Eq, PartialEq, Debug, Getters)]
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

/// Represents a new view message in the IronChain protocol.
///
/// The signature should be of the sequence number of the view that this message is for.
/// As well as the (optional) QC that this view is based on.
///
/// This message is sent to the leader of the view to force a new view to be created in cases
/// of timeouts or bootstrap.
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Debug, Getters)]
#[get = "pub"]
pub struct NewViewMessage {
    qc: Option<ChainedQC>,
    signature: PartialSignature,
}

impl NewViewMessage {
    pub fn from_previous_qc(qc: ChainedQC, signature: PartialSignature) -> Self {
        Self {
            qc: Some(qc),
            signature,
        }
    }

    pub fn from_empty(signature: PartialSignature) -> Self {
        Self {
            qc: None,
            signature,
        }
    }

    pub fn into_parts(self) -> (Option<ChainedQC>, PartialSignature) {
        (self.qc, self.signature)
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct ProposalMessage<D> {
    proposal: ChainedDecisionNode<D>,
}

impl<D> ProposalMessage<D> {
    pub fn new(proposal: ChainedDecisionNode<D>) -> Self {
        Self { proposal }
    }

    pub fn into_parts(self) -> ChainedDecisionNode<D> {
        self.proposal
    }
}

impl<D> Orderable for ProposalMessage<D> {
    fn sequence_number(&self) -> SeqNo {
        self.proposal.sequence_number()
    }
}

impl<D> Debug for ProposalMessage<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProposalMessage(proposal: {:?})", self.proposal)
    }
}
