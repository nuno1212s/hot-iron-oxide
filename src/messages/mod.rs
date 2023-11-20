use atlas_common::crypto::signature::Signature;
use atlas_common::ordering::{Orderable, SeqNo};

/// A quorum certificate
pub struct QC {
    qc_type: QCType,
    view_seq: SeqNo,

}

pub enum QCType {

}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub struct HotStuffMessage {
    curr_view: SeqNo,
    message: HotStuffOrderProtocolMessage
}

pub enum HotStuffOrderProtocolMessage {
    NewView(Option<QC>),
    Prepare(QC),
    PreCommit(QC),
    Commit(Signature),
    Decide(QC)
}

impl HotStuffMessage {

    pub fn kind(&self) -> &HotStuffOrderProtocolMessage {
        &self.message
    }

}

impl Orderable for HotStuffMessage {
    fn sequence_number(&self) -> SeqNo {
        self.curr_view
    }
}