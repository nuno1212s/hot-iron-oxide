pub mod serialize;

use atlas_common::crypto::signature::Signature;
use atlas_common::ordering::{Orderable, SeqNo};
use crate::decisions::DecisionNode;

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
/// A quorum certificate
pub struct QC<D> {
    qc_type: QCType,
    view_seq: SeqNo,
    decision_node: DecisionNode<D>,
}

impl<D> QC<D> {
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
    PrepareVote(DecisionNode<D>, Signature),
    PreCommit(QC<D>),
    PreCommitVote(DecisionNode<D>, Signature),
    Commit(QC<D>),
    CommitVote(DecisionNode<D>, Signature),
    Decide(QC<D>),
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