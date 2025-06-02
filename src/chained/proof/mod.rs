use crate::chained::chained_decision_tree::ChainedDecisionNode;
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::IronChainMessage;
use crate::chained::ChainedQC;
use atlas_common::collections::HashMap;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::StoredMessage;
use atlas_core::ordering_protocol::networking::serialize::OrderProtocolProof;
use atlas_core::ordering_protocol::ShareableConsensusMessage;
use serde::{Deserialize, Serialize};
use strum::{EnumCount, EnumIter};

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, PartialEq, Eq, Hash, Copy, EnumCount, EnumIter)]
pub enum ProofQCType {
    PrePrepare,
    Prepare,
    Commit,
    Decide,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub struct ChainedProof<RQ> {
    decision_node: ChainedDecisionNode<RQ>,
    proposal_message: StoredMessage<IronChainMessage<RQ>>,
    qcs: HashMap<ProofQCType, ChainedQC>,
}

impl<RQ> ChainedProof<RQ> {
    pub fn new(
        decision_node: ChainedDecisionNode<RQ>,
        proposal_message: StoredMessage<IronChainMessage<RQ>>,
        qcs: HashMap<ProofQCType, ChainedQC>,
    ) -> Self {
        ChainedProof {
            decision_node,
            proposal_message,
            qcs,
        }
    }

    pub fn decision_node(&self) -> &ChainedDecisionNode<RQ> {
        &self.decision_node
    }

    pub fn qcs(&self) -> &HashMap<ProofQCType, ChainedQC> {
        &self.qcs
    }

    pub fn qc(&self, qc_type: ProofQCType) -> Option<&ChainedQC> {
        self.qcs.get(&qc_type)
    }
    
    pub fn proposal_message(&self) -> &StoredMessage<IronChainMessage<RQ>> {
        &self.proposal_message
    }
}

impl<RQ> Orderable for ChainedProof<RQ>
where
    RQ: SerMsg,
{
    fn sequence_number(&self) -> SeqNo {
        self.decision_node.sequence_number()
    }
}

impl<RQ> OrderProtocolProof for ChainedProof<RQ>
where
    RQ: SerMsg,
{
    fn contained_messages(&self) -> usize {
        self.decision_node
            .decision_header()
            .contained_client_commands()
    }
}

impl<RQ> PartialEq for ChainedProof<RQ>
where
    RQ: SerMsg,
{
    fn eq(&self, other: &Self) -> bool {
        *self.decision_node.decision_header() == *other.decision_node.decision_header()
            && self.qcs == other.qcs
    }
}

impl<RQ> Eq for ChainedProof<RQ> where RQ: SerMsg {}
