pub(super) mod serialize;

use crate::protocol::{DecisionNode, DecisionNodeHeader};
use atlas_common::crypto::threshold_crypto::{CombinedSignature, PartialSignature};
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
    vote: Option<ChainedQC>,
    signature: PartialSignature,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct ProposalMessage<D> {
    proposal: DecisionNode<D>,
    qc: ChainedQC,
}

/// In chained hotstuff, there is only one QC type,
/// generic which will be used for all the steps of the protocol
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
#[get = "pub"]
pub struct ChainedQC {
    /// The sequence number of the QC
    seq_no: SeqNo,
    decision_node: DecisionNodeHeader,
    signature: CombinedSignature,
}

impl<RQ> Orderable for IronChainMessage<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.seq_no
    }
}

impl ChainedQC {
    
    pub fn new(
        seq_no: SeqNo,
        decision_node: DecisionNodeHeader,
        signature: CombinedSignature,
    ) -> Self {
        ChainedQC {
            seq_no,
            decision_node,
            signature,
        }
    }
    
}

impl Orderable for ChainedQC {
    fn sequence_number(&self) -> SeqNo {
        self.seq_no
    }
}