pub mod serialize;
mod msg_builder;

use atlas_common::crypto::threshold_crypto::{PartialSignature, CombinedSignature};
use atlas_common::ordering::{Orderable, SeqNo};
use crate::decisions::DecisionNode;

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
/// A quorum certificate
pub struct QC<D> {
    qc_type: QCType,
    view_seq: SeqNo,
    decision_node: DecisionNode<D>,
    // The combined signature of the quorum which voted for this QC
    signature: CombinedSignature,
}

impl<D> QC<D> {
    pub fn new(qc_type: QCType,
               view_seq: SeqNo,
               decision_node: DecisionNode<D>,
               signature: CombinedSignature) -> Self {
        QC {
            qc_type,
            view_seq,
            decision_node,
            signature,
        }
    }

    pub fn qc_type(&self) -> &QCType {
        &self.qc_type
    }
    pub fn view_seq(&self) -> SeqNo {
        self.view_seq
    }
    pub fn decision_node(&self) -> &DecisionNode<D> {
        &self.decision_node
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum QCType {
    PrepareVote,
    PreCommitVote,
    CommitVote,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub struct HotIronOxMsg<D> {
    curr_view: SeqNo,
    message: HotStuffOrderProtocolMessage<D>,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum HotStuffOrderProtocolMessage<D> {
    NewView(Option<QC<D>>),
    Prepare(DecisionNode<D>, Option<QC<D>>),
    PrepareVote(DecisionNode<D>, PartialSignature),
    PreCommit(QC<D>),
    PreCommitVote(DecisionNode<D>, PartialSignature),
    Commit(QC<D>),
    CommitVote(DecisionNode<D>, PartialSignature),
    Decide(QC<D>),
}


#[derive(Clone)]
pub enum VoteType {
    PrepareVote,
    PreCommitVote,
    CommitVote,
}

#[derive(Clone)]
pub struct VoteMessage<D> {
    vote_type: VoteType,
    decision_node: DecisionNode<D>,
    signature: PartialSignature,
}

pub enum ProposalType {
    Prepare,
    PreCommit,
    Commit,
    Decide,
}

pub struct ProposalMessage<D> {
    proposal_type: ProposalType,
    quorum_certificate: QC<D>,
}

pub enum HotIronOxMsgType<D> {
    Proposal(ProposalMessage<D>),
    Vote(VoteMessage<D>),
}

pub struct HotFeOxMsg<D> {
    curr_view: SeqNo,
    message: HotIronOxMsgType<D>,
}

impl<D> HotIronOxMsg<D> {
    pub fn message(view: SeqNo, message: HotStuffOrderProtocolMessage<D>) -> Self {
        HotIronOxMsg {
            curr_view: view,
            message,
        }
    }

    pub fn kind(&self) -> &HotStuffOrderProtocolMessage<D> {
        &self.message
    }

    pub fn into_kind(self) -> HotStuffOrderProtocolMessage<D> {
        self.message
    }
}

impl<D> Orderable for HotIronOxMsg<D> {
    fn sequence_number(&self) -> SeqNo {
        self.curr_view
    }
}

impl<D> Orderable for QC<D> {
    fn sequence_number(&self) -> SeqNo {
        self.view_seq
    }
}

