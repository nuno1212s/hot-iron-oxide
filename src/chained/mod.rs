use crate::chained::messages::serialize::IronChainSer;
use crate::chained::protocol::ChainedHotStuffProtocol;
use crate::decision_tree::{DecisionHandler, DecisionNodeHeader, TQuorumCertificate};
use crate::req_aggr::ReqAggregator;
use crate::view::View;
use atlas_common::crypto::threshold_crypto::CombinedSignature;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{Decision, DecisionAD, DecisionMetadata, OPExecResult, OPPollResult, ProtocolMessage};
use atlas_core::timeouts::timeout::TimeoutModHandle;
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::sync::Arc;

mod messages;
mod protocol;
mod chained_decision_tree;

type IronChainResult<RQ> = OPExecResult<
    DecisionMetadata<RQ, IronChainSer<RQ>>,
    DecisionAD<RQ, IronChainSer<RQ>>,
    ProtocolMessage<RQ, IronChainSer<RQ>>,
    RQ,
>;

type IronChainDecision<RQ> = Decision<
    DecisionMetadata<RQ, IronChainSer<RQ>>,
    DecisionAD<RQ, IronChainSer<RQ>>,
    ProtocolMessage<RQ, IronChainSer<RQ>>,
    RQ,
>;

type IronChainPollResult<RQ> = OPPollResult<
    DecisionMetadata<RQ, IronChainSer<RQ>>,
    DecisionAD<RQ, IronChainSer<RQ>>,
    ProtocolMessage<RQ, IronChainSer<RQ>>,
    RQ,
>;

type ChainedDecisionHandler = DecisionHandler<ChainedQC>;

pub struct IronChain<RQ, NT, CR, RQA>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    CR: Send + Sync,
{
    node_id: NodeId,
    network_node: Arc<NT>,
    quorum_information: Arc<CR>,
    protocol: ChainedHotStuffProtocol<RQ, RQA>,
}

impl<RQ, NT, CR, RQA> IronChain<RQ, NT, CR, RQA>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    CR: Send + Sync,
{
    pub fn new(
        node_id: NodeId,
        network_node: Arc<NT>,
        rq_aggr: Arc<RQA>,
        timeouts: TimeoutModHandle,
        quorum_information: Arc<CR>,
        current_view: View,
    ) -> Self
    where
        RQA: ReqAggregator<RQ>,
    {
        let protocol = ChainedHotStuffProtocol::new(node_id, rq_aggr, timeouts, current_view);

        Self {
            node_id,
            network_node,
            quorum_information,
            protocol,
        }
    }
}

/// In chained hotstuff, there is only one QC type,
/// generic which will be used for all the steps of the protocol
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
pub struct ChainedQC {
    /// The sequence number of the QC
    seq_no: SeqNo,
    decision_node: DecisionNodeHeader,
    #[get = "pub"]
    signature: CombinedSignature,
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

impl TQuorumCertificate for ChainedQC {
    fn decision_node(&self) -> &DecisionNodeHeader {
        &self.decision_node
    }

    fn signature(&self) -> &CombinedSignature {
        &self.signature
    }
}
