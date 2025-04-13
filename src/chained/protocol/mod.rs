mod decision;
mod msg_queue;
mod log;

use std::collections::VecDeque;
use std::sync::Arc;
use atlas_common::node_id::NodeId;
use atlas_common::serialization_helper::SerMsg;
use atlas_core::ordering_protocol::ShareableMessage;
use atlas_core::timeouts::timeout::TimeoutModHandle;
use crate::chained::{IronChainPollResult, IronChainResult};
use crate::chained::messages::IronChainMessage;
use crate::chained::protocol::decision::ChainedDecision;
use crate::protocol::DecisionHandler;
use crate::protocol::req_aggr::RequestAggr;
use crate::view::View;


pub(crate) struct ChainedHotStuffProtocol<RQ> {
    node_id: NodeId,
    
    decision_handler: DecisionHandler,
    
    request_aggr: Arc<RequestAggr<RQ>>,
    
    timeouts: TimeoutModHandle,
    
    decision_list: VecDeque<ChainedDecision<RQ>>,

    curr_view: View
}

impl<RQ> ChainedHotStuffProtocol<RQ> 
where RQ: SerMsg {
    
    const PROTOCOL_PHASES: u8 = 4;
    
    #[must_use] pub fn new(
        node_id: NodeId,
        rq_aggr: Arc<RequestAggr<RQ>>,
        timeouts: TimeoutModHandle,
        mut curr_view: View,
    ) -> Self {
        
        let mut decision_list = VecDeque::with_capacity(Self::PROTOCOL_PHASES as usize);
        
        for _ in 0..Self::PROTOCOL_PHASES {
            let next_view = curr_view.next_view();
            
            let chained_decision = ChainedDecision::new(curr_view, node_id);
            
            decision_list.push_back(chained_decision);
            
            curr_view = next_view;
        }
        
        Self {
            node_id,
            decision_handler: DecisionHandler::default(),
            request_aggr: rq_aggr,
            timeouts,
            decision_list,
            curr_view
        }
    }
    
    fn poll(&mut self) -> IronChainPollResult<RQ> {
        todo!()
    }
    
    fn process_message(&mut self, message: ShareableMessage<IronChainMessage<RQ>>) -> IronChainResult<RQ> {
        todo!()
    }
    
}