mod test;

use crate::crypto::{get_partial_signature_for_message, CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionExtensionVerifier, DecisionNode, DecisionNodeHeader};
use crate::metric::{
    SIGNATURE_PROPOSAL_LATENCY_ID, SIGNATURE_VOTE_LATENCY_ID,
};
use crate::req_aggr::ReqAggregator;
use crate::view::View;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_common::{quiet_unwrap, threadpool};
use atlas_core::messages::{ClientRqInfo, SessionBased};
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{BatchedDecision, ProtocolConsensusDecision, ShareableMessage};
use atlas_metrics::metrics::metric_duration;
use derive_more::with_trait::Display;
use getset::{Getters, Setters};
use std::error::Error;
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tracing::{error, info, trace, warn};
use crate::hot_iron::metric::ConsensusDecisionMetric;
use crate::hot_iron::protocol::log::{DecisionLog, MsgDecisionLog, NewViewAcceptError, NewViewGenerateError, VoteAcceptError, VoteStoreError};
use crate::hot_iron::messages::{HotFeOxMsg, HotFeOxMsgType, ProposalMessage, ProposalType, ProposalTypes, VoteMessage, VoteType, VoteTypes};
use crate::hot_iron::protocol::msg_queue::HotStuffTBOQueue;
use crate::hot_iron::protocol::{HotIronDecisionHandler, QC};
use crate::hot_iron::messages::serialize::HotIronOxSer;

#[derive(Debug)]
pub enum DecisionState {
    Init,
    Prepare(bool, usize),
    PreCommit(bool, usize),
    Commit(bool, usize),
    Decide(bool, usize),
    Finally,
    NextView,
}

#[derive(Debug, Display)]
pub enum DecisionFinalizationResult {
    Finalized,
    NextView,
    NotFinal,
}

/// The decision for a given object
#[derive(Getters, Setters)]
pub struct HSDecision<D> {
    #[get = "pub"]
    view: View,
    node_id: NodeId,
    #[getset(get = "pub (super)", set = "pub (super)")]
    current_state: DecisionState,
    decision_queue: HotStuffTBOQueue<D>,
    msg_decision_log: MsgDecisionLog,
    #[getset(get = "pub (super)")]
    decision_log: DecisionLog<D>,
    #[get = "pub(crate)"]
    consensus_metric: ConsensusDecisionMetric,
}

/// The result of the poll operation on the decision
pub enum DecisionPollResult<D> {
    TryPropose,
    Recv,
    NextMessage(ShareableMessage<HotFeOxMsg<D>>),
    Decided,
}

/// The decision of the poll operation on the decision
pub enum DecisionResult<D> {
    DuplicateVote(NodeId),
    MessageIgnored,
    MessageQueued,
    DecisionProgressed(
        Option<QC>,
        Option<DecisionNodeHeader>,
        ShareableMessage<HotFeOxMsg<D>>,
    ),
    Decided(Option<QC>, ShareableMessage<HotFeOxMsg<D>>),
}

impl<RQ> HSDecision<RQ>
where
    RQ: SerMsg,
{
    pub fn new(view: View, node_id: NodeId) -> Self {
        let msg_decision_log = if view.primary() == node_id {
            MsgDecisionLog::Leader(Box::default())
        } else {
            MsgDecisionLog::Replica(Box::default())
        };

        let consensus_metric = if view.primary() == node_id {
            ConsensusDecisionMetric::leader()
        } else {
            ConsensusDecisionMetric::replica()
        };

        HSDecision {
            view,
            node_id,
            current_state: DecisionState::Init,
            decision_queue: HotStuffTBOQueue::default(),
            msg_decision_log,
            decision_log: DecisionLog::default(),
            consensus_metric,
        }
    }

    /// Poll this decision
    pub fn poll<NT, CR, CP>(
        &mut self,
        network: &Arc<NT>,
        dec_handler: &HotIronDecisionHandler,
        crypto_info: &Arc<CR>,
    ) -> DecisionPollResult<RQ>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        match self.current_state {
            DecisionState::Init => {
                threadpool::execute({
                    let network = network.clone();
                    let crypto_info = crypto_info.clone();
                    let latest_qc = dec_handler.latest_qc();
                    let view_leader = self.view.primary();
                    let seq = self.view.sequence_number();

                    move || {
                        let vote_type = VoteType::NewView(latest_qc);

                        let partial_sig = get_partial_signature_for_message::<CR, CP, VoteType>(
                            &*crypto_info,
                            seq,
                            &vote_type,
                        );

                        info!(
                            "Sending vote message {:?} to view leader {:?}",
                            vote_type, view_leader
                        );

                        quiet_unwrap!(network.send(
                            HotFeOxMsg::new(
                                seq,
                                HotFeOxMsgType::Vote(VoteMessage::new(vote_type, partial_sig))
                            ),
                            view_leader,
                            true
                        ));
                    }
                });

                self.consensus_metric
                    .as_replica()
                    .register_vote_sent(VoteTypes::NewView);
                self.current_state = DecisionState::Prepare(false, 0);

                DecisionPollResult::Recv
            }
            DecisionState::Prepare(_, _)
            | DecisionState::PreCommit(_, _)
            | DecisionState::Commit(_, _)
            | DecisionState::Decide(_, _)
                if self.decision_queue.should_poll() =>
            {
                self.decision_queue
                    .get_message_for(self.is_leader(), &self.current_state)
                    .map_or(DecisionPollResult::Recv, |message| {
                        DecisionPollResult::NextMessage(message)
                    })
            }
            DecisionState::Finally => DecisionPollResult::Decided,
            _ => DecisionPollResult::Recv,
        }
    }

    /// Queue a message into this decision, so it can be polled later
    pub(super) fn queue(&mut self, message: ShareableMessage<HotFeOxMsg<RQ>>) {
        self.decision_queue.queue_message(message);
    }

    /// Are we the leader of the current view
    fn is_leader(&self) -> bool {
        self.view.primary() == self.node_id
    }

    fn leader(&self) -> NodeId {
        self.view.primary()
    }

    pub(super) fn can_be_finalized(&self) -> DecisionFinalizationResult {
        match self.current_state {
            DecisionState::Finally => DecisionFinalizationResult::Finalized,
            DecisionState::NextView => DecisionFinalizationResult::NextView,
            _ => DecisionFinalizationResult::NotFinal,
        }
    }

    pub fn next_view_received(&mut self) {
        self.current_state = DecisionState::NextView;
    }

    /// Process a given consensus message
    pub fn process_message<NT, CR, CP, RQA>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut HotIronDecisionHandler,
        crypto: &Arc<CR>,
        req_aggr: &Arc<RQA>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        RQA: ReqAggregator<RQ>,
        CP: CryptoProvider,
    {
        let is_leader = self.is_leader();

        trace!(
            decision = &(u32::from(self.sequence_number())),
            "Processing message {:?} with current state {:?}",
            message,
            self.current_state
        );

        if self.view.sequence_number() != message.sequence_number() {
            error!(
                decision = &(u32::from(self.sequence_number())),
                "Message sequence number {:?} does not match current view sequence number {:?}",
                message.sequence_number(),
                self.view.sequence_number()
            );

            return Ok(DecisionResult::MessageIgnored);
        }

        match &mut self.current_state {
            DecisionState::Init => {
                self.decision_queue.queue_message(message);

                Ok(DecisionResult::MessageQueued)
            }
            DecisionState::Prepare(_, _) if is_leader => match message.message().message() {
                HotFeOxMsgType::Proposal(_) => self.process_message_prepare::<NT, CR, CP>(
                    message,
                    network,
                    dec_handler,
                    crypto,
                ),
                HotFeOxMsgType::Vote(_) => self.process_message_prepare_leader::<NT, CR, CP, RQA>(
                    message, network, crypto, req_aggr,
                ),
            },
            DecisionState::Prepare(_, _) => {
                self.process_message_prepare::<NT, CR, CP>(message, network, dec_handler, crypto)
            }
            DecisionState::PreCommit(_, _) if is_leader => match &message.message().message() {
                HotFeOxMsgType::Proposal(_) => self.process_message_pre_commit::<NT, CR, CP>(
                    message,
                    network,
                    dec_handler,
                    crypto,
                ),
                HotFeOxMsgType::Vote(_) => self
                    .process_message_pre_commit_leader::<NT, CR, CP, RQA>(
                        message, network, crypto, req_aggr,
                    ),
            },
            DecisionState::PreCommit(_, _) => {
                self.process_message_pre_commit::<NT, CR, CP>(message, network, dec_handler, crypto)
            }
            DecisionState::Commit(_, _) if is_leader => match &message.message().message() {
                HotFeOxMsgType::Proposal(_) => {
                    self.process_message_commit::<NT, CR, CP>(message, network, dec_handler, crypto)
                }
                HotFeOxMsgType::Vote(_) => self.process_message_commit_leader::<NT, CR, CP, RQA>(
                    message, network, crypto, req_aggr,
                ),
            },
            DecisionState::Commit(_, _) => {
                self.process_message_commit::<NT, CR, CP>(message, network, dec_handler, crypto)
            }
            DecisionState::Decide(_, _) if is_leader => match &message.message().message() {
                HotFeOxMsgType::Vote(_) => self.process_message_decide_leader::<NT, CR, CP, RQA>(
                    message, network, crypto, req_aggr,
                ),
                HotFeOxMsgType::Proposal(_) => {
                    self.process_message_decide::<NT, CR, CP>(message, dec_handler, network, crypto)
                }
            },
            DecisionState::Decide(_, _) => {
                self.process_message_decide::<NT, CR, CP>(message, dec_handler, network, crypto)
            }
            DecisionState::Finally | DecisionState::NextView => Ok(DecisionResult::MessageIgnored),
        }
    }

    fn process_message_leader<NT, CR, CP, RQA>(
        &mut self,
        vote_message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
        req_aggr: &Arc<RQA>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        RQA: ReqAggregator<RQ>,
        CP: CryptoProvider,
    {
        let (vt_header, HotFeOxMsgType::Vote(vt_msg)) =
            (vote_message.header(), vote_message.message().message())
        else {
            unreachable!("Message received is not a vote message")
        };

        let (DecisionState::Prepare(proposal_sent, received_votes)
        | DecisionState::PreCommit(proposal_sent, received_votes)
        | DecisionState::Commit(proposal_sent, received_votes)
        | DecisionState::Decide(proposal_sent, received_votes)) = &mut self.current_state
        else {
            unreachable!("Leader decision state is not Prepare, PreCommit, Commit or Decide")
        };

        self.consensus_metric
            .as_leader()
            .register_vote_received(vt_msg.vote_type().into());

        let leader_log = self
            .msg_decision_log
            .as_mut_leader()
            .ok_or(DecisionError::NoLeaderMessageLog)?;

        *received_votes += 1;

        leader_log.accept_vote(vt_header.from(), vt_msg.clone())?;

        if *received_votes >= self.view.quorum() && !*proposal_sent {
            *proposal_sent = true;

            let proposal_type = if let VoteType::NewView(_) = vt_msg.vote_type() {
                let high_qc = leader_log.new_view_store().get_high_qc();

                let (pooled_request, digest) = req_aggr.take_pool_requests();

                let node = if let Some(qc) = high_qc {
                    DecisionNode::create_leaf(&qc.decision_node(), digest, pooled_request)
                } else {
                    DecisionNode::create_root(&self.view, digest, pooled_request)
                };

                let new_qc = leader_log
                    .new_view_store()
                    .create_new_qc::<_, CP>(&**crypto, node.decision_header())?;

                self.consensus_metric
                    .as_leader()
                    .register_proposal_sent(ProposalTypes::Decide);

                ProposalType::Prepare(node, new_qc)
            } else {
                let proposal_type = match vt_msg.vote_type() {
                    VoteType::PrepareVote(_) => ProposalTypes::PreCommit,
                    VoteType::PreCommitVote(_) => ProposalTypes::Commit,
                    VoteType::CommitVote(_) => ProposalTypes::Decide,
                    VoteType::NewView(_) => unreachable!(),
                };

                let qc = leader_log.generate_qc::<CR, CP>(&**crypto, &self.view, proposal_type)?;

                self.consensus_metric
                    .as_leader()
                    .register_proposal_sent(proposal_type);

                match proposal_type {
                    ProposalTypes::PreCommit => ProposalType::PreCommit(qc),
                    ProposalTypes::Commit => ProposalType::Commit(qc),
                    ProposalTypes::Decide => ProposalType::Decide(qc),
                    ProposalTypes::Prepare => unreachable!(),
                }
            };

            let proposal_message = ProposalMessage::new(proposal_type);

            let msg = HotFeOxMsgType::Proposal(proposal_message);

            let _ = network.broadcast(
                HotFeOxMsg::new(self.view.sequence_number(), msg),
                self.view.quorum_members().iter().copied(),
            );

            Ok(DecisionResult::DecisionProgressed(None, None, vote_message))
        } else if *proposal_sent {
            Ok(DecisionResult::MessageIgnored)
        } else {
            Ok(DecisionResult::DecisionProgressed(None, None, vote_message))
        }
    }

    fn process_message_replica<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        dec_handler: &mut HotIronDecisionHandler,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let (_, HotFeOxMsgType::Proposal(proposal)) =
            (message.header(), message.message().message())
        else {
            unreachable!("Message received is not a proposal message")
        };

        self.consensus_metric
            .as_replica()
            .register_proposal_received(proposal.proposal_type().into());

        let (vote_type, qc, node) = match &mut self.current_state {
            DecisionState::Prepare(_, _) => {
                let ProposalType::Prepare(node, qc) = proposal.proposal_type() else {
                    unreachable!()
                };
                
                let verifier = DecisionNodeExtensionVerifier;

                if !dec_handler.safe_node(node, qc, &verifier) {
                    warn!(
                        "Node {:?}, QC {:?} does not extend from {:?}",
                        node,
                        qc,
                        dec_handler.latest_qc()
                    );

                    return Err(DecisionError::InvalidDecisionNode);
                }

                self.decision_log.set_current_proposal(Some(node.clone()));

                dec_handler.install_latest_prepare_qc(qc.clone());

                self.current_state = DecisionState::PreCommit(false, 0);

                (
                    Some(VoteType::PrepareVote(*node.decision_header())),
                    qc.clone(),
                    Some(qc.decision_node()),
                )
            }
            DecisionState::PreCommit(_, _)
            | DecisionState::Commit(_, _)
            | DecisionState::Decide(_, _) => match proposal.proposal_type() {
                ProposalType::PreCommit(qc) => {
                    self.current_state = DecisionState::Commit(false, 0);

                    (
                        Some(VoteType::PreCommitVote(qc.decision_node())),
                        qc.clone(),
                        None,
                    )
                }
                ProposalType::Commit(qc) => {
                    self.current_state = DecisionState::Decide(false, 0);

                    (
                        Some(VoteType::CommitVote(qc.decision_node())),
                        qc.clone(),
                        None,
                    )
                }
                ProposalType::Decide(qc) => {
                    self.current_state = DecisionState::Finally;

                    (None, qc.clone(), None)
                }
                _ => unreachable!(),
            },
            _ => unreachable!("Attempting to process a proposal message in an invalid state"),
        };

        if let Some(vote_type) = vote_type {
            self.consensus_metric
                .as_replica()
                .register_vote_sent((&vote_type).into());

            threadpool::execute({
                let network = network.clone();

                let crypto = crypto.clone();

                let view = self.view.clone();

                move || {
                    // Send the message signing processing to the threadpool

                    let msg_signature = get_partial_signature_for_message::<CR, CP, VoteType>(
                        &*crypto,
                        view.sequence_number(),
                        &vote_type,
                    );

                    let vote_msg = HotFeOxMsgType::Vote(VoteMessage::new(vote_type, msg_signature));

                    let _ = network.send(
                        HotFeOxMsg::new(view.sequence_number(), vote_msg),
                        view.primary(),
                        true,
                    );
                }
            });

            Ok(DecisionResult::DecisionProgressed(
                Some(qc.clone()),
                node,
                message,
            ))
        } else {
            Ok(DecisionResult::Decided(Some(qc), message))
        }
    }

    fn process_message_prepare_leader<NT, CR, CP, RQA>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
        req_aggr: &Arc<RQA>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        RQA: ReqAggregator<RQ>,
        CP: CryptoProvider,
    {
        match message.message().message() {
            HotFeOxMsgType::Vote(vote_msg)
                if self.view.sequence_number() == message.sequence_number() =>
            {
                if let VoteType::NewView(_) = vote_msg.vote_type() {
                    self.process_message_leader::<NT, CR, CP, RQA>(
                        message, network, crypto, req_aggr,
                    )
                } else {
                    self.decision_queue.queue_message(message);

                    Ok(DecisionResult::MessageQueued)
                }
            }
            _ => {
                // Leaders create the proposals, they never receive them, so
                // All proposal messages are dropped or
                // The received message does not match our sequence number

                Ok(DecisionResult::MessageIgnored)
            }
        }
    }

    fn process_message_prepare<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut HotIronDecisionHandler,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        match message.message().clone().into() {
            HotFeOxMsgType::Proposal(proposal)
                if self.view.sequence_number() == message.message().sequence_number()
                    && message.header().from() == self.leader() =>
            {
                if let ProposalType::Prepare(_, _) = proposal.into() {
                    self.process_message_replica::<NT, CR, CP>(
                        message,
                        dec_handler,
                        network,
                        crypto,
                    )
                } else {
                    self.decision_queue.queue_message(message);

                    Ok(DecisionResult::MessageQueued)
                }
            }
            _ => {
                // A non leader can never receive vote messages, they can only receive
                // Proposals which they can vote on

                Ok(DecisionResult::MessageIgnored)
            }
        }
    }

    fn process_message_pre_commit_leader<NT, CR, CP, RQA>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
        rqa: &Arc<RQA>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        RQA: ReqAggregator<RQ>,
    {
        match message.message().message() {
            HotFeOxMsgType::Vote(vote_msg)
                if message.message().sequence_number() == self.view.sequence_number() =>
            {
                if let VoteType::PrepareVote(_) = vote_msg.vote_type() {
                    self.process_message_leader::<NT, CR, CP, RQA>(message, network, crypto, rqa)
                } else {
                    self.decision_queue.queue_message(message);

                    Ok(DecisionResult::MessageQueued)
                }
            }
            _ => {
                // Leaders do not receive proposals
                Ok(DecisionResult::MessageIgnored)
            }
        }
    }

    fn process_message_pre_commit<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut HotIronDecisionHandler,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        match message.message().message() {
            HotFeOxMsgType::Proposal(prop)
                if message.message().sequence_number() == self.view.sequence_number()
                    && message.header().from() == self.leader() =>
            {
                match prop.proposal_type() {
                    ProposalType::PreCommit(_) => self.process_message_replica::<NT, CR, CP>(
                        message,
                        dec_handler,
                        network,
                        crypto,
                    ),
                    ProposalType::Prepare(_, _) => Ok(DecisionResult::MessageIgnored),
                    ProposalType::Commit(_) | ProposalType::Decide(_) => {
                        self.decision_queue.queue_message(message);

                        Ok(DecisionResult::MessageQueued)
                    }
                }
            }
            _ => Ok(DecisionResult::MessageIgnored),
        }
    }

    fn process_message_commit_leader<NT, CR, CP, RQA>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
        rqa: &Arc<RQA>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        RQA: ReqAggregator<RQ>,
    {
        match message.message().message() {
            HotFeOxMsgType::Vote(vote_msg)
                if message.message().sequence_number() == self.view.sequence_number() =>
            {
                match vote_msg.vote_type() {
                    VoteType::PreCommitVote(_) => self
                        .process_message_leader::<NT, CR, CP, RQA>(message, network, crypto, rqa),
                    VoteType::PrepareVote(_) => Ok(DecisionResult::MessageIgnored),
                    _ => {
                        self.decision_queue.queue_message(message);

                        Ok(DecisionResult::MessageQueued)
                    }
                }
            }
            _ => {
                // Leaders do not receive proposals
                Ok(DecisionResult::MessageIgnored)
            }
        }
    }

    fn process_message_commit<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut HotIronDecisionHandler,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        match message.message().message() {
            HotFeOxMsgType::Proposal(prop)
                if message.message().sequence_number() == self.view.sequence_number()
                    && message.header().from() == self.leader() =>
            {
                match prop.proposal_type() {
                    ProposalType::Commit(_) => self.process_message_replica::<NT, CR, CP>(
                        message,
                        dec_handler,
                        network,
                        crypto,
                    ),
                    ProposalType::Decide(_) => {
                        self.decision_queue.queue_message(message);

                        Ok(DecisionResult::MessageQueued)
                    }
                    _ => Ok(DecisionResult::MessageIgnored),
                }
            }
            _ => Ok(DecisionResult::MessageIgnored),
        }
    }

    fn process_message_decide_leader<NT, CR, CP, RQA>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
        rqa: &Arc<RQA>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        RQA: ReqAggregator<RQ>,
    {
        match message.message().message() {
            HotFeOxMsgType::Vote(vote_msg)
                if message.message().sequence_number() == self.view.sequence_number() =>
            {
                if let VoteType::CommitVote(_) = vote_msg.vote_type() {
                    self.process_message_leader::<NT, CR, CP, RQA>(message, network, crypto, rqa)
                } else {
                    // This is the last type of message we can receive
                    Ok(DecisionResult::MessageIgnored)
                }
            }
            _ => {
                // Leaders do not receive proposals
                Ok(DecisionResult::MessageIgnored)
            }
        }
    }

    fn process_message_decide<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        dec_handler: &mut HotIronDecisionHandler,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        match message.message().message() {
            HotFeOxMsgType::Proposal(prop)
                if message.message().sequence_number() == self.view.sequence_number()
                    && message.header().from() == self.leader() =>
            {
                match prop.proposal_type() {
                    ProposalType::Decide(_) => self.process_message_replica::<NT, CR, CP>(
                        message,
                        dec_handler,
                        network,
                        crypto,
                    ),
                    _ => Ok(DecisionResult::MessageIgnored),
                }
            }
            _ => Ok(DecisionResult::MessageIgnored),
        }
    }

    pub fn finalize_decision(mut self) -> ProtocolConsensusDecision<RQ>
    where
        RQ: SerMsg + SessionBased,
    {
        let seq = self.sequence_number();

        self.consensus_metric
            .as_replica()
            .register_decision_finalized();

        let decision_node = self
            .decision_log
            .into_decision_node()
            .expect("Terminated a decision without an associated decision node");

        let (header, client_commands) = decision_node.into();

        let client_rq_infos = client_commands
            .iter()
            .map(|cmd| {
                ClientRqInfo::new(
                    *cmd.header().digest(),
                    cmd.header().from(),
                    cmd.message().sequence_number(),
                    cmd.message().session_number(),
                )
            })
            .collect::<Vec<_>>();

        let batch_decision = BatchedDecision::new(seq, client_commands, None);

        ProtocolConsensusDecision::new(
            seq,
            batch_decision,
            client_rq_infos,
            header.current_block_digest(),
        )
    }
}

struct DecisionNodeExtensionVerifier;

impl DecisionExtensionVerifier<QC> for DecisionNodeExtensionVerifier {
    fn is_extension_of_known_node(
        &self,
        node: &DecisionNodeHeader,
        locked_qc: Option<&QC>,
    ) -> bool {
        match (node.previous_block(), locked_qc) {
            (Some(prev), Some(latest_qc)) => {
                *prev == latest_qc.decision_node().current_block_digest()
            }
            (None, None) => true,
            _ => false,
        }
    }
}

impl<RQ> Orderable for HSDecision<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.view.sequence_number()
    }
}

#[derive(Error, Debug)]
pub enum DecisionError<CS: Error> {
    #[error("Received a prepare message with an invalid qc")]
    PrepareMessageWithEmptyQC,
    #[error("No QC was available to create the decision")]
    NoQCAvailable,
    #[error("Vote accept error {0}")]
    VoteAcceptError(#[from] VoteAcceptError),
    #[error("Vote store error {0}")]
    VoteStoreError(#[from] VoteStoreError<CS>),
    #[error("New view accept error {0}")]
    NewViewAcceptError(#[from] NewViewAcceptError),
    #[error("Failed to generate new QC {0}")]
    NewViewGenerateError(#[from] NewViewGenerateError<CS>),
    #[error("Invalid state for processing message")]
    InvalidState,
    #[error("Failed to get the leader message log when processing vote message")]
    NoLeaderMessageLog,
    #[error("Invalid decision node")]
    InvalidDecisionNode,
}
