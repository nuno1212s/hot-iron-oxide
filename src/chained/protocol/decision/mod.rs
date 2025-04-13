use crate::chained::messages::{ChainedQC, IronChainMessage, IronChainMessageType};
use crate::chained::protocol::log::{DecisionLog, NewViewGenerateError, NewViewStore};
use crate::chained::protocol::msg_queue::ChainedHotStuffMsgQueue;
use crate::chained::IronChainPollResult;
use crate::crypto::{CryptoInformationProvider, CryptoProvider};
use crate::protocol::req_aggr::{ReqAggregator, RequestAggr};
use crate::protocol::{DecisionNode, QC};
use crate::view::View;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::ordering_protocol::ShareableMessage;
use getset::{Getters, Setters};
use std::sync::Arc;

pub(super) enum ChainedDecisionState {
    Prepare(usize),
    PreCommit,
    Commit,
    Decide,
    Finalized,
}

pub(super) enum ChainedDecisionResult<D> {
    ProposalGenerated(
        DecisionNode<D>,
        ChainedQC,
        ShareableMessage<IronChainMessage<D>>,
    ),
    VoteGenerated(ChainedQC, ShareableMessage<IronChainMessage<D>>),
    MessageIgnored,
    MessageProcessed(ShareableMessage<IronChainMessage<D>>),
}

#[derive(Getters, Setters)]
pub(super) struct ChainedDecision<RQ> {
    #[get = "pub"]
    view: View,
    #[getset(get = "pub", set = "pub")]
    proposal: Option<DecisionNode<RQ>>,
    #[getset(get = "pub", set = "pub")]
    state: ChainedDecisionState,
    msg_queue: ChainedHotStuffMsgQueue<RQ>,
    decision_log: DecisionLog,
}

impl<RQ> ChainedDecision<RQ>
where
    RQ: SerMsg,
{
    pub fn new(view: View, node_id: NodeId) -> Self {
        let decision_log = if node_id == view.primary() {
            DecisionLog::Leader(NewViewStore::default())
        } else {
            DecisionLog::Replica
        };

        Self {
            view,
            proposal: None,
            state: ChainedDecisionState::Prepare(0),
            msg_queue: ChainedHotStuffMsgQueue::default(),
            decision_log,
        }
    }

    pub(super) fn poll(&mut self) -> IronChainPollResult<RQ> {
        if self.msg_queue.should_poll() {
            let message = self.msg_queue.pop_message();
            if let Some(message) = message {
                return IronChainPollResult::Exec(message);
            }
        }

        IronChainPollResult::ReceiveMsg
    }

    pub(super) fn process_message_leader<CR, CP>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        crypto: &Arc<CR>,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> Result<ChainedDecisionResult<RQ>, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let vote_msg = match message.message().message() {
            IronChainMessageType::Vote(vote_msg)
                if message.message().sequence_number() == self.sequence_number() =>
            {
                vote_msg.clone()
            }
            _ => return Ok(ChainedDecisionResult::MessageIgnored),
        };

        let ChainedDecisionState::Prepare(received) = &mut self.state else {
            return Ok(ChainedDecisionResult::MessageIgnored);
        };

        self.decision_log
            .as_mut_leader()
            .accept_new_view(message.header().from(), vote_msg);

        if *received >= self.view.quorum() {


            let (requests, digest) = rq_aggr.take_pool_requests();
            
            let decision_node = if let Some(qc) = self.decision_log.as_mut_leader().get_high_qc() {
                DecisionNode::create_leaf(qc.decision_node(), digest, requests)
            } else {
                DecisionNode::create_root(
                    &self.view,
                    digest,
                    requests,
                )
            };
            
            let qc = self
                .decision_log
                .as_mut_leader()
                .create_new_qc::<CR, CP>(crypto, decision_node.decision_header())?;

            self.state = ChainedDecisionState::PreCommit;
            
            Ok(ChainedDecisionResult::ProposalGenerated(decision_node, qc, message))
        } else {
            Ok(ChainedDecisionResult::MessageProcessed(message))
        }
    }

    pub(super) fn process_message(
        &mut self,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> ChainedDecisionResult<RQ> {
        todo!()
    }
}

impl<RQ> Orderable for ChainedDecision<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.view.sequence_number()
    }
}
