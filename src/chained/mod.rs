use crate::chained::messages::serialize::IronChainSer;
use crate::chained::protocol::ChainedHotStuffProtocol;
use crate::protocol::req_aggr::RequestAggr;
use crate::view::View;
use atlas_common::node_id::NodeId;
use atlas_common::serialization_helper::SerMsg;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{
    DecisionAD, DecisionMetadata, OPExecResult, OPPollResult, ProtocolMessage,
};
use atlas_core::timeouts::timeout::TimeoutModHandle;
use std::sync::Arc;

mod messages;
mod protocol;

type IronChainResult<RQ> = OPExecResult<
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

pub struct IronChain<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    CR: Send + Sync,
{
    node_id: NodeId,
    network_node: Arc<NT>,
    quorum_information: Arc<CR>,
    protocol: ChainedHotStuffProtocol<RQ>,
}

impl<RQ, NT, CR> IronChain<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    CR: Send + Sync,
{
    pub fn new(
        node_id: NodeId,
        network_node: Arc<NT>,
        rq_aggr: Arc<RequestAggr<RQ>>,
        timeouts: TimeoutModHandle,
        quorum_information: Arc<CR>,
        current_view: View,
    ) -> Self {
        let protocol = ChainedHotStuffProtocol::new(node_id, rq_aggr, timeouts, current_view);

        Self {
            node_id,
            network_node,
            quorum_information,
            protocol,
        }
    }
}
