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
use tracing::{debug, error, info, trace, warn};
use crate::chained::metrics::ConsensusMetric;

#[derive(Debug, PartialOrd, Ord, Eq, PartialEq)]
pub(super) enum ChainedDecisionState {
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
    NextLeaderMessages(ShareableMessage<IronChainMessage<D>>, ChainedQC),
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
    #[getset(get = "pub")]
    state: ChainedDecisionState,
    #[getset(get = "pub", set = "")]
    decision_node_digest: Option<Digest>,
    #[getset(get = "", set = "")]
    decision_qc: Option<ChainedQC>,
    msg_queue: ChainedHotStuffMsgQueue<RQ>,
    decision_log: DecisionLog,
    metrics: ConsensusMetric,
}

impl<RQ> ChainedDecision<RQ>
where
    RQ: SerMsg,
{
    pub fn new_first_view(view: View, node_id: NodeId) -> Self {
        if view.primary() == node_id {
            Self::new(view, node_id)
        } else {
            Self::new_internal(view, node_id, ChainedDecisionState::Init)
        }
    }

    fn new_internal(
        view: View,
        node_id: NodeId,
        state: ChainedDecisionState,
    ) -> ChainedDecision<RQ> {
        let (decision_log, metrics) = if node_id == view.primary() {
            (
                DecisionLog::Leader(NewViewStore::default()),
                ConsensusMetric::leader_decision(),
            )
        } else if node_id == view.next_view().primary() {
            (
                DecisionLog::NextLeader(VoteStore::default()),
                ConsensusMetric::next_leader_decision(),
            )
        } else {
            (DecisionLog::Replica, ConsensusMetric::replica_consensus_decision())
        };

        Self {
            view,
            node_id,
            state,
            decision_qc: None,
            decision_node_digest: None,
            msg_queue: ChainedHotStuffMsgQueue::default(),
            decision_log,
            metrics,
        }
    }

    pub fn new(view: View, node_id: NodeId) -> Self {
        Self::new_internal(view, node_id, ChainedDecisionState::Prepare(0, false))
    }

    pub fn advance_state(
        &mut self,
        state: ChainedDecisionState,
        decision_handler: &mut ChainedDecisionHandler,
    ) {
        debug!(
            "Checking if can advance state from {:?} to {:?} for view: {:?}-> {}",
            self.state,
            state,
            self.view,
            state > self.state
        );
        if state > self.state {
            self.state = state;

            match self.state {
                ChainedDecisionState::PreCommit if self.decision_qc.is_some() => {
                    info!("Advancing to PreCommit state for view: {:?}", self.view);

                    decision_handler.install_latest_prepare_qc(self.decision_qc.clone().unwrap());
                }
                ChainedDecisionState::Commit if self.decision_qc.is_some() => {
                    info!("Advancing to Commit state for view: {:?}", self.view);

                    decision_handler.install_latest_locked_qc(self.decision_qc.clone().unwrap());
                }
                ChainedDecisionState::Decide | ChainedDecisionState::Finalized => (),
                _ => {
                    error!(
                        "Advanced into non sensical state: {:?} for view: {:?} decision: {:?}",
                        self.state, self.view, self.decision_qc
                    );
                }
            }
        }
    }

    pub(super) fn poll<CR, CP, NT>(
        &mut self,
        crypto: &Arc<CR>,
        decision_handler: &ChainedDecisionHandler,
        network: &Arc<NT>,
    ) -> ChainedDecisionPollResult<RQ>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        return match &self.state {
            ChainedDecisionState::Init  => {
                self.generate_and_send_new_view_message::<CR, CP, NT>(
                    decision_handler.latest_locked_qc_ref().cloned(),
                    network,
                    crypto,
                );

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
        debug!(
            "Queued message: {:?} in state: {:?} for view: {:?}",
            message.message().message(),
            self.state,
            self.view
        );

        self.msg_queue.queue_message(message);
    }

    pub(super) fn process_message<CR, CP, NT>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        decision_handler: &mut ChainedDecisionHandler,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        pending_decision_nodes: &PendingDecisionNodes<RQ>,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> Result<ChainedDecisionResult<RQ>, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        trace!(
            "Processing message: {:?} in state: {:?} for view: {:?}. Is leader: {}, Is next leader: {}",
            message, self.state, self.view, self.is_leader(), self.is_next_leader()
        );

        if self.is_leader()
            && matches!(
                message.message().message(),
                IronChainMessageType::NewView(_)
            )
        {
            self.process_message_leader::<CR, CP, NT>(rq_aggr, crypto, network, message)
        } else if self.is_next_leader()
            && matches!(message.message().message(), IronChainMessageType::Vote(_))
        {
            self.process_message_next_leader::<CR, CP, NT>(decision_handler, crypto, message)
        } else {
            Ok(self.process_message_replica::<CR, CP, NT>(
                decision_handler,
                crypto,
                network,
                pending_decision_nodes,
                message,
            ))
        }
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

        debug!(
            "Processing {:?} from {:?} - Received {}. Proposed {}, Quorum {}",
            message.message().message(),
            message.header().from(),
            received,
            proposed,
            self.view.quorum()
        );

        if *received >= self.view.quorum() && !*proposed {
            let decision_log_high_qc = self
                .decision_log
                .as_mut_leader()
                .get_high_qc()
                .map_err(|err| NewViewGenerateError::FailedToGenerateNewViewQC(err))?;

            *proposed = true;

            let decision_node_header = self.generate_and_send_proposal::<CR, NT>(
                rq_aggr,
                crypto,
                network,
                decision_log_high_qc.high_qc(),
            );

            Ok(ChainedDecisionResult::ProposalGenerated(
                decision_node_header,
                message,
            ))
        } else {
            Ok(ChainedDecisionResult::MessageProcessed(message))
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
                    warn!(
                        "Did not accept vote from {:?} for view: {:?}",
                        message.header().from(),
                        self.view
                    );
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

        debug!(
            "Processing {:?} as next leader from {:?} in state Prepare. Vote Count: {}, Quorum: {}",
            message.message().message(),
            message.header().from(),
            vote_count,
            self.view.quorum()
        );

        if vote_count >= self.view.quorum() && !self.decision_log.as_mut_next_leader().is_proposed()
        {
            self.decision_log.as_mut_next_leader().set_proposed(true);

            let qc = self
                .decision_log
                .as_mut_next_leader()
                .get_quorum_qc::<CR, CP>(&self.view, crypto.as_ref())?;

            decision_handler.install_latest_prepare_qc(qc.clone());

            debug!(
                "Next leader has received enough votes for QC: {:?} with count: {}",
                qc, vote_count
            );

            // Perform the generic as leader phase, as we are the leader of the next view
            let vote_details = self.decision_log.as_mut_next_leader().get_high_justify_qc();

            trace!("High justify QC: {:?}", vote_details);

            match (vote_details, decision_handler.latest_locked_qc_ref()) {
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

            Ok(ChainedDecisionResult::NextLeaderMessages(message, qc))
        } else {
            Ok(ChainedDecisionResult::MessageProcessed(message))
        }
    }

    pub(super) fn process_message_replica<CR, CP, NT>(
        &mut self,
        decision_handler: &mut ChainedDecisionHandler,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        pending_decision_nodes: &PendingDecisionNodes<RQ>,
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
            if !decision_handler.safe_node(proposal_message.proposal(), qc, pending_decision_nodes)
            {
                warn!(
                    "Proposal message from {:?} is not safe, ignoring it.",
                    message.header().from()
                );

                return ChainedDecisionResult::MessageIgnored;
            }

            debug!(
                "Proposal message from {:?} is safe, processing it.",
                message.header().from()
            );

            self.set_decision_qc(Some(qc.clone()));
        } else {
            debug!(
                "Proposal message from {:?} has no justification, processing it as root block.",
                message.header().from()
            );
        }

        let proposal = proposal_message.into_parts();

        self.decision_node_digest = Some(proposal.decision_header().current_block_digest());

        debug!(
            "Processing {:?} from {:?} in state {:?}. Decision Node Header: {:?}",
            message.message().message(),
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
                let vote_details = VoteDetails::new(decision_node_header, justify);

                let msg_signature = get_partial_signature_for_message::<CR, CP, VoteDetails>(
                    &*crypto,
                    view.sequence_number(),
                    &vote_details,
                );

                let vote_msg =
                    IronChainMessageType::Vote(VoteMessage::new(vote_details, msg_signature));

                let next_leader = view.next_view().primary();

                let _ = network.send(
                    IronChainMessage::new(view.sequence_number(), vote_msg),
                    next_leader,
                    true,
                );

                debug!("Sent vote message to next leader: {:?}", next_leader);
            }
        });

        ChainedDecisionResult::VoteGenerated(proposal, message)
    }

    pub(super) fn handle_previous_node_completed_leader<CR, NT>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        previous_high_qc: &ChainedQC,
    ) -> DecisionNodeHeader
    where
        CR: CryptoInformationProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        self.generate_and_send_proposal::<CR, NT>(
            rq_aggr,
            crypto,
            network,
            Some(previous_high_qc),
        )
    }

    fn generate_and_send_new_view_message<CR, CP, NT>(
        &self,
        previous: Option<ChainedQC>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
    ) where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        threadpool::execute({
            let crypto = crypto.clone();
            let network = network.clone();
            let seq_no = self.view.sequence_number();
            let primary = self.view.primary();

            move || {
                let signature = get_partial_signature_for_message::<CR, CP, Option<ChainedQC>>(
                    &*crypto, seq_no, &previous,
                );
                
                let new_view = if let Some(qc) = previous {
                    NewViewMessage::from_previous_qc(qc, signature)
                } else {
                    NewViewMessage::from_empty(signature)
                };

                debug!("Sending new view message: {:?}", new_view);

                let _ = network.send(
                    IronChainMessage::new(seq_no, IronChainMessageType::NewView(new_view)),
                    primary,
                    true,
                );
            }
        });
    }

    fn generate_and_send_proposal<CR, NT>(
        &mut self,
        rq_aggr: &Arc<RequestAggr<RQ>>,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        previous_high_qc: Option<&ChainedQC>,
    ) -> DecisionNodeHeader
    where
        CR: CryptoInformationProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    {
        let (requests, digest) = rq_aggr.take_pool_requests();

        let decision_node = if let Some(qc) = previous_high_qc {
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

        debug!(
            "Generated proposal message: {:?} for view: {:?}",
            hot_iron, self.view,
        );

        let _ = network.broadcast_signed(hot_iron, self.view.quorum_members().iter().cloned());

        decision_node_header
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
