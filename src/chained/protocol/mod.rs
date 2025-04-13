mod decision;
mod log;
mod msg_queue;

use crate::chained::messages::IronChainMessage;
use crate::chained::protocol::decision::ChainedDecision;
use crate::chained::{ChainedDecisionHandler, IronChainPollResult, IronChainResult};
use crate::decision_tree::DecisionHandler;
use crate::req_aggr::ReqAggregator;
use crate::view::View;
use atlas_common::node_id::NodeId;
use atlas_common::serialization_helper::SerMsg;
use atlas_core::ordering_protocol::ShareableMessage;
use atlas_core::timeouts::timeout::TimeoutModHandle;
use std::collections::VecDeque;
use std::sync::Arc;

pub(crate) struct ChainedHotStuffProtocol<RQ, RQA> {
    node_id: NodeId,

    decision_handler: ChainedDecisionHandler,

    request_aggr: Arc<RQA>,

    timeouts: TimeoutModHandle,

    decision_list: VecDeque<ChainedDecision<RQ>>,

    curr_view: View,
}

impl<RQ, RQA> ChainedHotStuffProtocol<RQ, RQA>
where
    RQ: SerMsg,
    RQA: ReqAggregator<RQ>,
{
    const PROTOCOL_PHASES: u8 = 4;

    #[must_use]
    pub fn new(
        node_id: NodeId,
        rq_aggr: Arc<RQA>,
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
            curr_view,
        }
    }

    fn poll(&mut self) -> IronChainPollResult<RQ> {
        todo!()
    }

    fn process_message(
        &mut self,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> IronChainResult<RQ> {
        todo!()
    }
}
