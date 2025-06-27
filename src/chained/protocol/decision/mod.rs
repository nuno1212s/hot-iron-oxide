use crate::chained::chained_decision_tree::{ChainedDecisionNode, PendingDecisionNodes};
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::{
    IronChainMessage, IronChainMessageType, NewViewMessage, ProposalMessage, VoteDetails,
    VoteMessage,
};
use crate::chained::protocol::log::{DecisionLog, NewViewGenerateError, NewViewStore, VoteStore};
use crate::chained::protocol::msg_queue::ChainedHotStuffMsgQueue;
use crate::chained::{ChainedDecisionHandler, ChainedQC};
use crate::crypto::{get_partial_signature_for_message, CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionNodeHeader, TQuorumCertificate};
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
use tracing::{debug, error, info};

#[derive(Debug)]
pub(super) enum ChainedDecisionState {
    WaitingForPreviousQC,
    Init,
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
    MessageQueued,
    MessageProcessed(ShareableMessage<IronChainMessage<D>>),
    NextLeaderMessages(ShareableMessage<IronChainMessage<D>>),
}

pub(super) enum ChainedDecisionPollResult<D> {
    Exec(ShareableMessage<IronChainMessage<D>>),
    ReceiveMsg,
    Decided,
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
    pub fn new_first_view(view: View, node_id: NodeId) -> Self {
        Self::new_internal(view, node_id, ChainedDecisionState::Init)
    }

    fn new_internal(
        view: View,
        node_id: NodeId,
        state: ChainedDecisionState,
    ) -> ChainedDecision<RQ> {
        let decision_log = if node_id == view.primary() {
            DecisionLog::Leader(NewViewStore::default())
        } else if node_id == view.next_view().primary() {
            DecisionLog::NextLeader(VoteStore::default())
        } else {
            DecisionLog::Replica
        };

        Self {
            view,
            node_id,
            state,
            decision_node_digest: None,
            msg_queue: ChainedHotStuffMsgQueue::default(),
            decision_log,
        }
    }

    pub fn new(view: View, node_id: NodeId) -> Self {
        Self::new_internal(view, node_id, ChainedDecisionState::Prepare(0, false))
    }

    pub(super) fn poll<RQA, NT>(
        &mut self,
        request_aggr: &RQA,
        decision_handler: &ChainedDecisionHandler,
        network: &Arc<NT>,
    ) -> ChainedDecisionPollResult<RQ>
    where
        RQA: ReqAggregator<RQ>,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        return match &self.state {
            ChainedDecisionState::Init if !self.is_leader() => {
                let generic_qc = decision_handler.latest_prepare_qc_ref().cloned();

                let new_view = if let Some(qc) = generic_qc {
                    NewViewMessage::from_previous_qc(qc)
                } else {
                    NewViewMessage::from_empty()
                };

                info!("Sending new view message: {:?}", new_view);
                let _ = network.send(
                    IronChainMessage::new(
                        self.sequence_number(),
                        IronChainMessageType::NewView(new_view),
                    ),
                    self.view.primary(),
                    true,
                );

                self.state = ChainedDecisionState::Prepare(0, false);

                ChainedDecisionPollResult::ReceiveMsg
            }
            ChainedDecisionState::Init if self.is_leader() => {
                match decision_handler.latest_prepare_qc_ref().cloned() {
                    None => {
                        
                    }
                    Some(latest_qc) => {
                        info!(
                            "Leader is initializing with latest prepare QC: {:?}",
                            latest_qc
                        );

                        
                    }
                }

                self.state = ChainedDecisionState::Prepare(0, false);

                ChainedDecisionPollResult::ReceiveMsg
            }
            ChainedDecisionState::Finalized => ChainedDecisionPollResult::Decided,
            _ => {
                if self.msg_queue.should_poll() {
                    let message = self.msg_queue.pop_message();

                    if let Some(message) = message {
                        return ChainedDecisionPollResult::Exec(message);
                    }
                }

                ChainedDecisionPollResult::ReceiveMsg
            }
        };

        ChainedDecisionPollResult::ReceiveMsg
    }

    fn is_leader(&self) -> bool {
        *self.node_id() == self.view.primary()
    }

    fn is_next_leader(&self) -> bool {
        *self.node_id() == self.view.next_view().primary()
    }

    pub(super) fn queue_message(&mut self, message: ShareableMessage<IronChainMessage<RQ>>) {
        self.msg_queue.queue_message(message);
    }

    pub(super) fn process_message<CR, CP, NT>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        decision_handler: &mut ChainedDecisionHandler,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> Result<ChainedDecisionResult<RQ>, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        info!(
            "Processing message: {:?} in state: {:?} for view: {:?}. Is leader: {}, Is next leader: {}",
            message, self.state, self.view, self.is_leader(), self.is_next_leader()
        );

        if self.is_leader()
            && matches!(
                message.message().message(),
                IronChainMessageType::NewView(_)
            )
        {
            info!(
                "Processing NewView message from {:?} as leader in state {:?}",
                message.header().from(), self.state
            );
            self.process_message_leader::<CR, CP, NT>(rq_aggr, crypto, network, message)
        } else if self.is_next_leader()
            && matches!(message.message().message(), IronChainMessageType::Vote(_))
        {
            info!("Processing Vote message from {:?} as next leader in state {:?}",
                message.header().from(), self.state
            );
            self.process_message_next_leader::<CR, CP, NT>(decision_handler, crypto, message)
        } else {
            info!(
                "Processing message as replica from {:?} in state: {:?}",
                message.header().from(),
                self.state
            );
            
            Ok(self.process_message_replica::<CR, CP, NT>(
                decision_handler,
                crypto,
                network,
                message,
            ))
        }
    }

    /// Processes a message as the next leader (n + 1) for view n in the decision-making process.
    pub(super) fn process_message_next_leader<CR, CP, NT>(
        &mut self,
        decision_handler: &mut ChainedDecisionHandler,
        crypto: &Arc<CR>,
        message: ShareableMessage<IronChainMessage<RQ>>,
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
            _ => {
                error!(
                    "Received unexpected message in next leader state: {:?}",
                    message
                );
                unreachable!()
            }
        };

        if vote_count >= self.view.quorum() {
            let qc = self
                .decision_log
                .as_mut_next_leader()
                .get_quorum_qc::<CR, CP>(&self.view, crypto.as_ref())?;

            decision_handler.install_latest_prepare_qc(qc.clone());
            
            debug!(
                "Next leader has received enough votes for QC: {:?} with count: {}",
                qc, vote_count
            );
        }

        info!(
            "Processing Vote message as next leader from {:?} in state Prepare. Vote Count: {}, Quorum: {}",
            message.header().from(),
            vote_count,
            self.view.quorum()
        );

        // Perform the generic as leader phase, as we are the leader of the next view
        let qc = self.decision_log.as_mut_next_leader().get_high_justify_qc();
        
        debug!("High justify QC: {:?}", qc);

        match (qc, decision_handler.latest_locked_qc_ref()) {
            (Some(qc), Some(latest_locked_qc))
                if qc.justify().map_or(SeqNo::ZERO, Orderable::sequence_number)
                    > latest_locked_qc.sequence_number() =>
            {
                if qc.justify().is_some() {
                    decision_handler.install_latest_prepare_qc(qc.justify().cloned().unwrap());
                }
            }
            _ => {}
        }

        Ok(ChainedDecisionResult::NextLeaderMessages(message))
    }

    /// This function processes a message as a leader in the decision-making process.
    ///
    /// This will only get executed for NewView messages that are sent to the leader of the
    /// current view.
    /// All other messages are sent with sequence number of view n to the leader of view n+1.
    /// To see the processing of those messages, see [`Self::process_message_next_leader()`].
    pub(super) fn process_message_leader<CR, CP, NT>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        message: ShareableMessage<IronChainMessage<RQ>>,
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
            self.queue_message(message);

            return Ok(ChainedDecisionResult::MessageQueued);
        };

        *received += 1;

        info!(
            "Processing NewView message from {:?} in state Prepare. {}. Proposed {}, Quorum {}",
            message.header().from(),
            received,
            proposed,
            self.view.quorum()
        );

        if *received >= self.view.quorum() && !*proposed {
            *proposed = true;

            let decision_node_header =
                self.generate_and_send_proposal::<CR, CP, NT>(rq_aggr, crypto, network.clone())?;

            Ok(ChainedDecisionResult::ProposalGenerated(
                decision_node_header,
                message,
            ))
        } else {
            Ok(ChainedDecisionResult::MessageProcessed(message))
        }
    }

    pub(super) fn process_message_replica<CR, CP, NT>(
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
                debug!(
                    "Proposal message from {:?} is not safe, ignoring it.",
                    message.header().from()
                );
                return ChainedDecisionResult::MessageIgnored;
            }

            decision_handler.install_latest_prepare_qc(qc.clone());
        }

        let proposal = proposal_message.into_parts();

        self.decision_node_digest = Some(proposal.decision_header().current_block_digest());

        self.state = ChainedDecisionState::PreCommit;

        info!(
            "Processing proposal message from {:?} in state {:?}. Decision Node Header: {:?}",
            message.header().from(),
            self.state,
            proposal.decision_header()
        );

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

                info!("Sent vote message to next leader: {:?}", next_leader);
            }
        });

        ChainedDecisionResult::VoteGenerated(proposal, message)
    }

    fn generate_and_send_proposal<CR, CP, NT>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        crypto: &Arc<CR>,
        network: Arc<NT>,
    ) -> Result<DecisionNodeHeader, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
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

        let decision_node_header = *decision_node.decision_header();

        let proposal = ProposalMessage::new(decision_node);

        let hot_iron = IronChainMessage::new(
            self.sequence_number(),
            IronChainMessageType::Proposal(proposal),
        );

        let _ = network.broadcast_signed(hot_iron, self.view.quorum_members().iter().cloned());

        Ok(decision_node_header)
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
