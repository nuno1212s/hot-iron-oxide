use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::{IronChainMessage, IronChainMessageType, ProposalMessage, VoteMessage};
use crate::chained::protocol::log::{DecisionLog, NewViewGenerateError, NewViewStore};
use crate::chained::protocol::msg_queue::ChainedHotStuffMsgQueue;
use crate::chained::{ChainedDecisionHandler, ChainedQC, IronChainPollResult};
use crate::crypto::{get_partial_signature_for_message, CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionNode, DecisionNodeHeader};
use crate::metric::SIGNATURE_VOTE_LATENCY_ID;
use crate::view::View;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_common::threadpool;
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::ShareableMessage;
use atlas_metrics::metrics::metric_duration;
use getset::{Getters, Setters};
use std::sync::Arc;
use std::time::Instant;
use crate::req_aggr::{ReqAggregator, RequestAggr};

pub(super) enum ChainedDecisionState {
    Prepare(usize, bool),
    PreCommit,
    Commit,
    Decide,
    Finalized,
}

pub(super) enum ChainedDecisionResult<D> {
    ProposalGenerated(
        DecisionNodeHeader,
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
            state: ChainedDecisionState::Prepare(0, false),
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

    pub(super) fn process_message_leader<CR, CP, NT>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        crypto: &Arc<CR>,
        message: ShareableMessage<IronChainMessage<RQ>>,
        network: Arc<NT>,
    ) -> Result<ChainedDecisionResult<RQ>, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        let vote_msg = match message.message().message() {
            IronChainMessageType::Vote(vote_msg)
                if message.message().sequence_number() == self.sequence_number() =>
            {
                vote_msg.clone()
            }
            _ => return Ok(ChainedDecisionResult::MessageIgnored),
        };

        let ChainedDecisionState::Prepare(received, proposed) = &mut self.state else {
            return Ok(ChainedDecisionResult::MessageIgnored);
        };

        self.decision_log
            .as_mut_leader()
            .accept_new_view(message.header().from(), vote_msg);

        if *received >= self.view.quorum() && !*proposed {
            let (requests, digest) = rq_aggr.take_pool_requests();

            let decision_node = if let Some(qc) = self.decision_log.as_mut_leader().get_high_qc() {
                DecisionNode::create_leaf(qc.decision_node(), digest, requests)
            } else {
                DecisionNode::create_root(&self.view, digest, requests)
            };

            let qc = self
                .decision_log
                .as_mut_leader()
                .create_new_qc::<CR, CP>(crypto, decision_node.decision_header())?;

            *proposed = true;
            
            let decision_node_header = *decision_node.decision_header();
            
            let proposal = ProposalMessage::new(decision_node, qc.clone());
            
            let hot_iron = IronChainMessage::new(self.sequence_number(), IronChainMessageType::Proposal(proposal));
            
            let _ = network.broadcast_signed(hot_iron, self.view.quorum_members().iter().cloned());
            
            Ok(ChainedDecisionResult::ProposalGenerated(
                decision_node_header,
                qc,
                message,
            ))
        } else {
            Ok(ChainedDecisionResult::MessageProcessed(message))
        }
    }

    pub(super) fn process_message<CR, CP, NT>(
        &mut self,
        decision_handler: &mut ChainedDecisionHandler,
        crypto: &Arc<CR>,
        message: ShareableMessage<IronChainMessage<RQ>>,
        network: &Arc<NT>,
    ) -> ChainedDecisionResult<RQ>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        let proposal_message = match message.message().message() {
            IronChainMessageType::Proposal(proposal_msg)
                if message.message().sequence_number() == self.sequence_number() =>
            {
                proposal_msg.clone()
            }
            _ => return ChainedDecisionResult::MessageIgnored,
        };

        if !decision_handler.safe_node(proposal_message.proposal(), proposal_message.qc()) {
            return ChainedDecisionResult::MessageIgnored;
        }
        
        decision_handler.install_latest_prepare_qc(proposal_message.qc().clone());

        self.state = ChainedDecisionState::PreCommit;

        threadpool::execute({
            let network = network.clone();

            let crypto = crypto.clone();

            let view = self.view.clone();
            
            let qc = proposal_message.qc().clone();

            move || {
                // Send the message signing processing to the thread pool

                let start_time = Instant::now();
                
                let msg_signature = get_partial_signature_for_message::<CR, CP, ChainedQC>(
                    &*crypto,
                    view.sequence_number(),
                    &qc,
                );

                metric_duration(SIGNATURE_VOTE_LATENCY_ID, start_time.elapsed());

                let vote_msg = IronChainMessageType::Vote(VoteMessage::new(Some(qc), msg_signature));

                let _ = network.send(
                    IronChainMessage::new(view.sequence_number(), vote_msg),
                    view.primary(),
                    true,
                );
            }
        });
        
        ChainedDecisionResult::VoteGenerated(proposal_message.qc().clone(), message)
    }
}

impl<RQ> Orderable for ChainedDecision<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.view.sequence_number()
    }
}
