use crate::chained::chained_decision_tree::ChainedDecisionNode;
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::{
    IronChainMessage, IronChainMessageType, ProposalMessage, VoteDetails, VoteMessage,
};
use crate::chained::protocol::log::{DecisionLog, NewViewGenerateError, NewViewStore};
use crate::chained::protocol::msg_queue::ChainedHotStuffMsgQueue;
use crate::chained::{ChainedDecisionHandler, ChainedQC, IronChainPollResult};
use crate::crypto::{get_partial_signature_for_message, CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionNode, DecisionNodeHeader, TQuorumCertificate};
use crate::metric::SIGNATURE_VOTE_LATENCY_ID;
use crate::req_aggr::{ReqAggregator, RequestAggr};
use crate::view::View;
use atlas_common::crypto::hash::Digest;
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
use thiserror::Error;

pub(super) enum ChainedDecisionState {
    Prepare(usize, bool),
    PreCommit,
    Commit,
    Decide,
    Finalized,
}

pub(super) enum ChainedDecisionResult<D> {
    ProposalGenerated(DecisionNodeHeader, ShareableMessage<IronChainMessage<D>>),
    VoteGenerated(
        ChainedDecisionNode<D>,
        ShareableMessage<IronChainMessage<D>>,
    ),
    MessageIgnored,
    MessageProcessed(ShareableMessage<IronChainMessage<D>>),
}

#[derive(Getters, Setters)]
pub(super) struct ChainedDecision<RQ> {
    #[get = "pub"]
    view: View,
    #[get = "pub"]
    node_id: NodeId,
    #[getset(get = "pub", set = "pub")]
    state: ChainedDecisionState,
    #[getset(get = "pub", set = "")]
    decision_node_digest: Option<Digest>,
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
            node_id,
            state: ChainedDecisionState::Prepare(0, false),
            decision_node_digest: None,
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

    fn is_next_leader(&self) -> bool {
        *self.node_id() == self.view.next_view().primary()
    }

    pub(super) fn process_message_next_leader<CR, CP, NT>(
        &mut self,
        decision_handler: &mut ChainedDecisionHandler,
        crypto: &Arc<CR>,
        message: ShareableMessage<IronChainMessage<RQ>>,
        network: Arc<NT>,
    ) -> Result<ChainedDecisionResult<RQ>, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        let vote_count = match message.message().message() {
            IronChainMessageType::Vote(vote_msg)
                if message.message().sequence_number() == self.sequence_number()
                    && self.is_next_leader() =>
            {
                if let Some(votes) = self
                    .decision_log
                    .as_mut_next_leader()
                    .accept_vote(message.header().from(), vote_msg.clone())
                {
                    votes
                } else {
                    return Ok(ChainedDecisionResult::MessageIgnored);
                }
            }
            _ => unreachable!(),
        };

        if vote_count >= self.view.quorum() {
            let qc = self
                .decision_log
                .as_mut_next_leader()
                .get_quorum_qc::<CR, CP>(&self.view, crypto.as_ref())?;

            decision_handler.install_latest_prepare_qc(qc.clone());
        }

        let qc = self.decision_log.as_mut_next_leader().get_high_justify_qc();

        match (qc, decision_handler.latest_locked_qc()) {
            (Some(qc), Some(latest_locked_qc))
                if qc
                    .justify()
                    .map_or(SeqNo::ZERO, Orderable::sequence_number)
                    > latest_locked_qc.sequence_number() => {
                
                if qc.justify().is_some() {
                    decision_handler.install_latest_prepare_qc(qc.justify().cloned().unwrap());
                }
            }
            _ => {}
        }

        Ok(ChainedDecisionResult::MessageProcessed(message))
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
        match message.message().message() {
            IronChainMessageType::NewView(new_view)
                if message.message().sequence_number() == self.sequence_number() =>
            {
                if !self
                    .decision_log
                    .as_mut_leader()
                    .accept_new_view(message.header().from(), new_view.clone())
                {
                    return Ok(ChainedDecisionResult::MessageIgnored);
                }
            }
            _ => return Ok(ChainedDecisionResult::MessageIgnored),
        };

        let ChainedDecisionState::Prepare(received, proposed) = &mut self.state else {
            return Ok(ChainedDecisionResult::MessageIgnored);
        };

        *received += 1;

        if *received >= self.view.quorum() && !*proposed {
            let (requests, digest) = rq_aggr.take_pool_requests();

            let decision_node = if let Some(qc) = self.decision_log.as_mut_leader().get_high_qc() {
                ChainedDecisionNode::create_leaf(
                    self.view.sequence_number(),
                    qc.decision_node(),
                    digest,
                    requests,
                    qc.clone(),
                )
            } else {
                ChainedDecisionNode::create_root(&self.view, digest, requests)
            };

            *proposed = true;

            let decision_node_header = *decision_node.decision_header();

            let proposal = ProposalMessage::new(decision_node);

            let hot_iron = IronChainMessage::new(
                self.sequence_number(),
                IronChainMessageType::Proposal(proposal),
            );

            let _ = network.broadcast_signed(hot_iron, self.view.quorum_members().iter().cloned());

            Ok(ChainedDecisionResult::ProposalGenerated(
                decision_node_header,
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
        network: &Arc<NT>,
        message: ShareableMessage<IronChainMessage<RQ>>,
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

        if let Some(qc) = proposal_message.proposal().justify() {
            if !decision_handler.safe_node(proposal_message.proposal(), qc) {
                return ChainedDecisionResult::MessageIgnored;
            }

            decision_handler.install_latest_prepare_qc(qc.clone());
        }

        let ProposalMessage { proposal } = proposal_message;

        self.decision_node_digest = Some(proposal.decision_header().current_block_digest());

        self.state = ChainedDecisionState::PreCommit;

        threadpool::execute({
            let network = network.clone();

            let crypto = crypto.clone();

            let view = self.view.clone();

            let decision_node_header = *proposal.decision_header();

            let justify = proposal.justify().clone();

            move || {
                // Send the message signing processing to the thread pool

                let start_time = Instant::now();

                let vote_details = VoteDetails::new(decision_node_header, justify);

                let msg_signature = get_partial_signature_for_message::<CR, CP, VoteDetails>(
                    &*crypto,
                    view.sequence_number(),
                    &vote_details,
                );

                metric_duration(SIGNATURE_VOTE_LATENCY_ID, start_time.elapsed());

                let vote_msg =
                    IronChainMessageType::Vote(VoteMessage::new(vote_details, msg_signature));

                let next_leader = view.next_view().primary();

                let _ = network.send(
                    IronChainMessage::new(view.sequence_number(), vote_msg),
                    next_leader,
                    true,
                );
            }
        });

        ChainedDecisionResult::VoteGenerated(proposal, message)
    }

    pub(super) fn finalize(self) -> Result<Digest, FinalizeDecisionError> {
        self.decision_node_digest
            .ok_or(FinalizeDecisionError::FailedToFinalizeDecisionMissingDigest)
    }
}

impl<RQ> Orderable for ChainedDecision<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.view.sequence_number()
    }
}

#[derive(Clone, Debug, Error)]
pub enum FinalizeDecisionError {
    #[error("Failed to finalize decision, missing digest")]
    FailedToFinalizeDecisionMissingDigest,
}
