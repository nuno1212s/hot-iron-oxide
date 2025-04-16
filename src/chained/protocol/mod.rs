mod decision;
mod log;
mod msg_queue;

use crate::chained::chained_decision_tree::PendingDecisionNodes;
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::IronChainMessage;
use crate::chained::protocol::decision::{ChainedDecision, ChainedDecisionResult};
use crate::chained::{ChainedDecisionHandler, IronChainPollResult, IronChainResult};
use crate::crypto::{CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionHandler, TQuorumCertificate};
use crate::req_aggr::ReqAggregator;
use crate::view::View;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::Orderable;
use atlas_common::serialization_helper::SerMsg;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::ShareableMessage;
use atlas_core::timeouts::timeout::TimeoutModHandle;
use either::Either;
use std::collections::VecDeque;
use std::sync::Arc;

pub(crate) struct ChainedHotStuffProtocol<RQ, RQA> {
    node_id: NodeId,

    decision_handler: ChainedDecisionHandler,

    request_aggr: Arc<RQA>,

    timeouts: TimeoutModHandle,

    decision_list: VecDeque<ChainedDecision<RQ>>,
    
    pending_nodes: PendingDecisionNodes<RQ>,

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
            pending_nodes: PendingDecisionNodes::default(),
            curr_view,
        }
    }

    fn poll(&mut self) -> IronChainPollResult<RQ> {
        todo!()
    }

    fn process_message<CR, CP, NT>(
        &mut self,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> IronChainResult<RQ> where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    {
        let decision_index = match message.sequence_number().index(self.curr_view.sequence_number()) {
            Either::Right(index) => index,
            Either::Left(_) => {
                return IronChainResult::MessageDropped
            }
        };

        let Some(decision) = self.decision_list.get_mut(decision_index) else {
            //TODO: Queue message

            return IronChainResult::MessageQueued;
        };

        let result = decision.process_message::<CR, CP, NT>(&mut self.decision_handler, crypto, network, message);

        match result {
            ChainedDecisionResult::ProposalGenerated(_, _, _) => {
            }
            ChainedDecisionResult::VoteGenerated(node, qc, message) => {
                
                if let Some(node) = node {
                    self.pending_nodes.insert(node);
                }
                
                let b_st = qc.decision_node();
                let b_1 = qc.decision_node().previous_block();
                
            }
            ChainedDecisionResult::MessageProcessed(_) => {}
            ChainedDecisionResult::MessageIgnored => {
                return IronChainResult::MessageDropped;
            }
        }
        
        todo!()
    }
}
