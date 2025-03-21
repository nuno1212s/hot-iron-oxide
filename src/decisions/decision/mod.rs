mod test;

use crate::crypto::{get_partial_signature_for_message, CryptoInformationProvider, CryptoProvider};
use crate::decisions::log::{
    DecisionLog, DecisionLogType, LeaderDecisionLog, MsgDecisionLog, MsgLeaderDecisionLog,
    MsgReplicaDecisionLog, NewViewAcceptError, NewViewGenerateError, ReplicaDecisionLog,
    VoteAcceptError, VoteStoreError,
};
use crate::decisions::msg_queue::HotStuffTBOQueue;
use crate::decisions::req_aggr::ReqAggregator;
use crate::decisions::{DecisionHandler, DecisionNode, DecisionNodeHeader, QCType, QC};
use crate::messages::serialize::HotIronOxSer;
use crate::messages::{
    HotFeOxMsg, HotFeOxMsgType, ProposalMessage, ProposalType, VoteMessage, VoteType,
};
use crate::metric::ConsensusDecisionMetric;
use crate::view::View;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_common::{quiet_unwrap, threadpool};
use atlas_core::messages::{ClientRqInfo, SessionBased};
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{BatchedDecision, ProtocolConsensusDecision, ShareableMessage};
use derive_more::with_trait::Display;
use getset::{Getters, Setters};
use std::error::Error;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info};

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
            MsgDecisionLog::Leader(MsgLeaderDecisionLog::default())
        } else {
            MsgDecisionLog::Replica(MsgReplicaDecisionLog::default())
        };

        let decision_log_type = if view.primary() == node_id {
            DecisionLogType::Leader(LeaderDecisionLog::default(), ReplicaDecisionLog::default())
        } else {
            DecisionLogType::Replica(ReplicaDecisionLog::default())
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
            decision_log: DecisionLog::new(decision_log_type),
            consensus_metric,
        }
    }

    /// Poll this decision
    pub fn poll<NT, CR, CP>(
        &mut self,
        network: &Arc<NT>,
        dec_handler: &DecisionHandler,
        crypto_info: &Arc<CR>,
    ) -> DecisionPollResult<RQ>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        match self.current_state {
            DecisionState::Init  => {
                threadpool::execute({
                    let network = network.clone();
                    let crypto_info = crypto_info.clone();
                    let latest_qc = dec_handler.latest_qc();
                    let view_leader = self.view.primary();
                    let seq = self.view.sequence_number();

                    move || {
                        let vote_type = VoteType::NewView(latest_qc);

                        let partial_sig = get_partial_signature_for_message::<CR, CP>(
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

                self.consensus_metric.as_replica().register_new_view_sent();
                self.current_state = DecisionState::Prepare(false, 0);

                DecisionPollResult::Recv
            }
            DecisionState::Prepare(_, _) if self.decision_queue.should_poll() => {
                let decision_queue = self.decision_queue.prepare.pop_front();

                if let Some(message) = decision_queue {
                    DecisionPollResult::NextMessage(message)
                } else {
                    DecisionPollResult::Recv
                }
            }
            DecisionState::PreCommit(_, _) if self.decision_queue.should_poll() => {
                let decision_queue = self.decision_queue.pre_commit.pop_front();

                if let Some(message) = decision_queue {
                    DecisionPollResult::NextMessage(message)
                } else {
                    DecisionPollResult::Recv
                }
            }
            DecisionState::Commit(_, _) if self.decision_queue.should_poll() => {
                let decision_queue = self.decision_queue.commit.pop_front();

                if let Some(message) = decision_queue {
                    DecisionPollResult::NextMessage(message)
                } else {
                    DecisionPollResult::Recv
                }
            }
            DecisionState::Decide(_, _) if self.decision_queue.should_poll() => {
                let decision_queue = self.decision_queue.decide.pop_front();

                if let Some(message) = decision_queue {
                    DecisionPollResult::NextMessage(message)
                } else {
                    DecisionPollResult::Recv
                }
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
        dec_handler: &mut DecisionHandler,
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

        info!(
            decision = &(u32::from(self.sequence_number())),
            "Processing message {:?} with current state {:?}", message, self.current_state
        );

        match &mut self.current_state {
            DecisionState::Init if self.view.sequence_number() == message.sequence_number() => {
                self.decision_queue.queue_message(message);

                Ok(DecisionResult::MessageQueued)
            }
            DecisionState::Init | DecisionState::Finally | DecisionState::NextView => {
                Ok(DecisionResult::MessageIgnored)
            }
            DecisionState::Prepare(_, _) if is_leader => {
                match message.message().message() {
                    HotFeOxMsgType::Proposal(_) => {
                        Ok(self.process_message_prepare::<NT, CR, CP>(
                            message,
                            network,
                            dec_handler,
                            crypto,
                        ))
                    }
                    HotFeOxMsgType::Vote(_) => {
                        self.process_message_prepare_leader::<NT, CR, CP, RQA>(
                                message, network, crypto, req_aggr,
                            )
                    }
                }
                
            },
            DecisionState::Prepare(_, _) => Ok(self.process_message_prepare::<NT, CR, CP>(
                message,
                network,
                dec_handler,
                crypto,
            )),
            DecisionState::PreCommit(_, _) if is_leader => match &message.message().message() {
                HotFeOxMsgType::Proposal(_) => Ok(self.process_message_pre_commit::<NT, CR, CP>(
                    message,
                    network,
                    dec_handler,
                    crypto,
                )),
                HotFeOxMsgType::Vote(_) => {
                    self.process_message_pre_commit_leader::<NT, CR, CP>(message, network, crypto)
                }
            },
            DecisionState::PreCommit(_, _) => Ok(self.process_message_pre_commit::<NT, CR, CP>(
                message,
                network,
                dec_handler,
                crypto,
            )),
            DecisionState::Commit(_, _) if is_leader => match &message.message().message() {
                HotFeOxMsgType::Proposal(_) => Ok(self.process_message_commit::<NT, CR, CP>(
                    message,
                    network,
                    dec_handler,
                    crypto,
                )),
                HotFeOxMsgType::Vote(_) => {
                    self.process_message_commit_leader::<NT, CR, CP>(message, network, crypto)
                }
            },
            DecisionState::Commit(_, _) => Ok(self.process_message_commit::<NT, CR, CP>(
                message,
                network,
                dec_handler,
                crypto,
            )),
            DecisionState::Decide(_, _) if is_leader => match &message.message().message() {
                HotFeOxMsgType::Vote(_) => self.process_message_decide_leader::<NT, CR, CP>(
                    message,
                    network,
                    dec_handler,
                    crypto,
                ),
                HotFeOxMsgType::Proposal(_) => {
                    Ok(self.process_message_decide::<NT, CR, CP>(message, dec_handler))
                }
            },
            DecisionState::Decide(_, _) => {
                Ok(self.process_message_decide::<NT, CR, CP>(message, dec_handler))
            }
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
        let DecisionState::Prepare(sent_prepare, received) = &mut self.current_state else {
            unreachable!("Leader decision state is not Prepare")
        };

        let vote_message = match message.message().clone().into() {
            HotFeOxMsgType::Vote(vote_msg)
                if self.view.sequence_number() == message.sequence_number() =>
            {
                if let VoteType::NewView(_) = vote_msg.vote_type() {
                    vote_msg
                } else {
                    self.decision_queue.queue_message(message);

                    return Ok(DecisionResult::MessageQueued);
                }
            }
            _ => {
                // Leaders create the proposals, they never receive them, so
                // All proposal messages are dropped or
                // The received message does not match our sequence number

                return Ok(DecisionResult::MessageIgnored);
            }
        };

        self.consensus_metric
            .as_leader()
            .register_new_view_received();
        let leader_log = self.msg_decision_log.as_mut_leader().unwrap();

        *received += 1;

        leader_log
            .new_view_store()
            .accept_new_view(message.header().from(), vote_message)?;

        if *received >= self.view.quorum() && !*sent_prepare {
            *sent_prepare = true;
            
            let high_qc = leader_log.new_view_store().get_high_qc();

            let (pooled_request, digest) = req_aggr.take_pool_requests();

            let node = if let Some(qc) = high_qc {
                DecisionNode::create_leaf(&qc.decision_node(), digest, pooled_request)
            } else {
                DecisionNode::create_root_leaf(&self.view, digest, pooled_request)
            };

            self.decision_log.set_current_proposal(Some(node.clone()));

            let decision_header = *node.decision_header();

            let new_qc = leader_log
                .new_view_store()
                .create_new_qc::<_, CP>(&**crypto, node.decision_header())?;

            let msg = HotFeOxMsgType::Proposal(ProposalMessage::new(ProposalType::Prepare(
                node,
                new_qc.clone(),
            )));

            let _ = network.broadcast(
                HotFeOxMsg::new(self.view.sequence_number(), msg),
                self.view.quorum_members().iter().copied(),
            );

            self.consensus_metric.as_leader().register_prepare_sent();

            Ok(DecisionResult::DecisionProgressed(
                Some(new_qc),
                Some(decision_header),
                message,
            ))
        } else {
            Ok(DecisionResult::DecisionProgressed(None, None, message))
        }
    }

    fn process_message_prepare<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut DecisionHandler,
        crypto: &Arc<CR>,
    ) -> DecisionResult<RQ>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let (node, qc) = match message.message().clone().into() {
            HotFeOxMsgType::Proposal(proposal)
                if self.view.sequence_number() == message.message().sequence_number()
                    && message.header().from() == self.leader() =>
            {
                if let ProposalType::Prepare(node, qc) = proposal.into() {
                    (node, qc)
                } else {
                    self.decision_queue.queue_message(message);

                    return DecisionResult::MessageQueued;
                }
            }
            _ => {
                // A non leader can never receive vote messages, they can only receive
                // Proposals which they can vote on

                return DecisionResult::MessageIgnored;
            }
        };

        if !dec_handler.safe_node(&node, &qc) {
            debug!(
                "Node {:?}, QC {:?} does not extend from {:?}",
                node,
                qc,
                dec_handler.latest_qc()
            );

           return DecisionResult::MessageIgnored
        }
        
        let short_node = *node.decision_header();

        self.decision_log.set_current_proposal(Some(node.clone()));
        dec_handler.install_latest_prepare_qc(qc.clone());

        threadpool::execute({
            let network = network.clone();

            let crypto = crypto.clone();

            let view = self.view.clone();

            move || {
                // Send the message signing processing to the threadpool
                let prepare_vote = VoteType::PrepareVote(short_node);

                let msg_signature = get_partial_signature_for_message::<CR, CP>(
                    &*crypto,
                    view.sequence_number(),
                    &prepare_vote,
                );

                let vote_msg =
                    HotFeOxMsgType::Vote(VoteMessage::new(prepare_vote, msg_signature));

                let _ = network.send(
                    HotFeOxMsg::new(view.sequence_number(), vote_msg),
                    view.primary(),
                    true,
                );
            }
        });

        self.consensus_metric
            .as_replica()
            .register_prepare_received();
        self.current_state = DecisionState::PreCommit(false, 0);

        DecisionResult::DecisionProgressed(None, Some(short_node), message)
    }

    fn process_message_pre_commit_leader<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let DecisionState::PreCommit(sent_pre_commit, received) = &mut self.current_state else {
            unreachable!("Leader decision state is not PreCommit")
        };

        let vote_msg = match message.message().message() {
            HotFeOxMsgType::Vote(vote_msg)
                if message.message().sequence_number() == self.view.sequence_number() =>
            {
                if let VoteType::PrepareVote(_) = vote_msg.vote_type() {
                    vote_msg.clone()
                } else {
                    self.decision_queue.queue_message(message);

                    return Ok(DecisionResult::MessageQueued);
                }
            }
            _ => {
                // Leaders do not receive proposals
                return Ok(DecisionResult::MessageIgnored);
            }
        };

        *received += 1;

        self.consensus_metric.as_leader().register_prepare_vote();
        let leader_log = self
            .msg_decision_log
            .as_mut_leader()
            .expect("Leader decision log not available");

        if !leader_log.accept_vote(message.header().from(), vote_msg)? {
            return Ok(DecisionResult::DuplicateVote(message.header().from()));
        }

        if *received >= self.view.quorum() && !*sent_pre_commit {
            *sent_pre_commit = true;
            
            let created_qc =
                leader_log.generate_qc::<CR, CP>(&**crypto, &self.view, QCType::PrepareVote)?;

            let view_seq = self.view.sequence_number();

            self.decision_log
                .as_mut_leader()
                .set_prepare_qc(created_qc.clone());

            let view_members = self.view.quorum_members().clone();

            let prop = ProposalType::PreCommit(created_qc.clone());

            let proposal_message = ProposalMessage::new(prop);

            let msg = HotFeOxMsg::new(view_seq, HotFeOxMsgType::Proposal(proposal_message));

            let _ = network.broadcast(msg, view_members.into_iter());

            self.consensus_metric.as_leader().register_pre_commit_sent();

            Ok(DecisionResult::DecisionProgressed(
                Some(created_qc),
                None,
                message,
            ))
        } else {
            Ok(DecisionResult::DecisionProgressed(None, None, message))
        }
    }

    fn process_message_pre_commit<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut DecisionHandler,
        crypto: &Arc<CR>,
    ) -> DecisionResult<RQ>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let qc = match message.message().message() {
            HotFeOxMsgType::Proposal(prop)
                if message.message().sequence_number() == self.view.sequence_number()
                    && message.header().from() == self.leader() =>
            {
                match prop.proposal_type() {
                    ProposalType::PreCommit(pre_commit) => pre_commit.clone(),
                    ProposalType::Prepare(_, _) => {
                        return DecisionResult::MessageIgnored;
                    }
                    ProposalType::Commit(_) | ProposalType::Decide(_) => {
                        self.decision_queue.queue_message(message);

                        return DecisionResult::MessageQueued;
                    }
                }
            }
            _ => return DecisionResult::MessageIgnored,
        };

        self.decision_log
            .as_mut_replica()
            .set_prepare_qc(qc.clone());

        dec_handler.install_latest_prepare_qc(qc.clone());

        threadpool::execute({
            let network = network.clone();
            let view = self.view.clone();
            let crypto = crypto.clone();
            let short_node = qc.decision_node();

            move || {
                let commit_vote = VoteType::PreCommitVote(short_node);

                let msg_signature = get_partial_signature_for_message::<CR, CP>(
                    &*crypto,
                    view.sequence_number(),
                    &commit_vote,
                );

                let vote_msg = HotFeOxMsgType::Vote(VoteMessage::new(commit_vote, msg_signature));

                let _ = network.send(
                    HotFeOxMsg::new(view.sequence_number(), vote_msg),
                    view.primary(),
                    true,
                );
            }
        });

        self.consensus_metric
            .as_replica()
            .register_pre_commit_proposal();
        self.current_state = DecisionState::Commit(false, 0);

        DecisionResult::DecisionProgressed(Some(qc), None, message)
    }

    fn process_message_commit_leader<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let DecisionState::Commit(sent_commit, received) = &mut self.current_state else {
            unreachable!("Leader decision state is not Commit")
        };

        let vote_msg = match message.message().message() {
            HotFeOxMsgType::Vote(vote_msg)
                if message.message().sequence_number() == self.view.sequence_number() =>
            {
                match vote_msg.vote_type() {
                    VoteType::PreCommitVote(_) => vote_msg.clone(),
                    VoteType::PrepareVote(_) => {
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    _ => {
                        self.decision_queue.queue_message(message);

                        return Ok(DecisionResult::MessageQueued);
                    }
                }
            }
            _ => {
                // Leaders do not receive proposals
                return Ok(DecisionResult::MessageIgnored);
            }
        };

        *received += 1;

        self.consensus_metric.as_leader().register_pre_commit_vote();
        let leader_log = self
            .msg_decision_log
            .as_mut_leader()
            .expect("Leader decision log not available");

        if !leader_log.accept_vote(message.header().from(), vote_msg)? {
            return Ok(DecisionResult::DuplicateVote(message.header().from()));
        }

        if *received >= self.view.quorum() && !*sent_commit {
            *sent_commit = true;
            
            let created_qc =
                leader_log.generate_qc::<CR, CP>(&**crypto, &self.view, QCType::PreCommitVote)?;

            let view_seq = self.view.sequence_number();

            self.decision_log
                .as_mut_leader()
                .set_pre_commit_qc(created_qc.clone());

            threadpool::execute({
                let view_members = self.view.quorum_members().clone();
                let created_qc = created_qc.clone();
                let network = network.clone();

                move || {
                    let prop = ProposalType::Commit(created_qc);

                    let proposal_message = ProposalMessage::new(prop);

                    let msg = HotFeOxMsg::new(view_seq, HotFeOxMsgType::Proposal(proposal_message));

                    let _ = network.broadcast(msg, view_members.into_iter());
                }
            });

            self.consensus_metric.as_leader().register_commit_sent();

            Ok(DecisionResult::DecisionProgressed(
                Some(created_qc),
                None,
                message,
            ))
        } else {
            Ok(DecisionResult::DecisionProgressed(None, None, message))
        }
    }

    fn process_message_commit<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut DecisionHandler,
        crypto: &Arc<CR>,
    ) -> DecisionResult<RQ>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let qc = match message.message().message() {
            HotFeOxMsgType::Proposal(prop)
                if message.message().sequence_number() == self.view.sequence_number()
                    && message.header().from() == self.leader() =>
            {
                match prop.proposal_type() {
                    ProposalType::Commit(commit) => commit.clone(),
                    ProposalType::Decide(_) => {
                        self.decision_queue.queue_message(message);

                        return DecisionResult::MessageQueued;
                    },
                    _ => return DecisionResult::MessageIgnored,
                }
            }
            _ => return DecisionResult::MessageIgnored,
        };

        self.decision_log.as_mut_replica().set_locked_qc(qc.clone());
        dec_handler.install_latest_locked_qc(qc.clone());

        threadpool::execute({
            let network = network.clone();
            let view = self.view.clone();
            let crypto = crypto.clone();
            let short_node = qc.decision_node();

            move || {
                let commit_vote = VoteType::CommitVote(short_node);

                let msg_signature = get_partial_signature_for_message::<CR, CP>(
                    &*crypto,
                    view.sequence_number(),
                    &commit_vote,
                );

                let vote_msg = HotFeOxMsgType::Vote(VoteMessage::new(commit_vote, msg_signature));

                let _ = network.send(
                    HotFeOxMsg::new(view.sequence_number(), vote_msg),
                    view.primary(),
                    true,
                );
            }
        });

        self.consensus_metric
            .as_replica()
            .register_commit_proposal();
        self.current_state = DecisionState::Decide(false, 0);

        DecisionResult::DecisionProgressed(Some(qc), None, message)
    }

    fn process_message_decide_leader<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        network: &Arc<NT>,
        dec_handler: &mut DecisionHandler,
        crypto: &Arc<CR>,
    ) -> Result<DecisionResult<RQ>, DecisionError<CP::CombinationError>>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let DecisionState::Decide(sent_decide, received) = &mut self.current_state else {
            unreachable!("Leader decision state is not Decide")
        };
        
        let vote_msg = match message.message().message() {
            HotFeOxMsgType::Vote(vote_msg)
                if message.message().sequence_number() == self.view.sequence_number() =>
            {
                if let VoteType::CommitVote(_) = vote_msg.vote_type() {
                    vote_msg.clone()
                } else {
                    // This is the last type of message we can receive
                    return Ok(DecisionResult::MessageIgnored)
                }
            }
            _ => {
                // Leaders do not receive proposals
                return Ok(DecisionResult::MessageIgnored);
            }
        };

        *received += 1;

        self.consensus_metric.as_leader().register_commit_vote();
        let leader_log = self
            .msg_decision_log
            .as_mut_leader()
            .expect("Leader decision log not available");

        if !leader_log.accept_vote(message.header().from(), vote_msg)? {
            return Ok(DecisionResult::DuplicateVote(message.header().from()));
        }

        if *received >= self.view.quorum() && !*sent_decide {
            *sent_decide = true;
            
            let created_qc =
                leader_log.generate_qc::<CR, CP>(&**crypto, &self.view, QCType::CommitVote)?;

            let view_seq = self.view.sequence_number();

            self.decision_log
                .as_mut_leader()
                .set_commit_qc(created_qc.clone());

            threadpool::execute({
                let view_members = self.view.quorum_members().clone();
                let created_qc = created_qc.clone();
                let network = network.clone();

                move || {
                    let prop = ProposalType::Decide(created_qc);

                    let proposal_message = ProposalMessage::new(prop);

                    let msg = HotFeOxMsg::new(view_seq, HotFeOxMsgType::Proposal(proposal_message));

                    let _ = network.broadcast(msg, view_members.into_iter());
                }
            });

            self.consensus_metric.as_leader().register_decided_sent();

            dec_handler.install_latest_qc(created_qc.clone());

            Ok(DecisionResult::Decided(Some(created_qc), message))
        } else {
            Ok(DecisionResult::DecisionProgressed(None, None, message))
        }
    }

    fn process_message_decide<NT, CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        dec_handler: &mut DecisionHandler,
    ) -> DecisionResult<RQ>
    where
        NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let qc = match message.message().message() {
            HotFeOxMsgType::Proposal(prop)
                if message.message().sequence_number() == self.view.sequence_number()
                    && message.header().from() == self.leader() =>
            {
                match prop.proposal_type() {
                    ProposalType::Decide(decide) => decide.clone(),
                    _ => return DecisionResult::MessageIgnored,
                }
            }
            _ => return DecisionResult::MessageIgnored,
        };

        self.consensus_metric
            .as_replica()
            .register_decided_received();
        self.current_state = DecisionState::Finally;

        dec_handler.install_latest_qc(qc.clone());

        DecisionResult::Decided(Some(qc), message)
    }

    pub fn finalize_decision(mut self) -> ProtocolConsensusDecision<RQ>
    where
        RQ: SerMsg + SessionBased,
    {
        let seq = self.sequence_number();

        if self.is_leader() {
            self.consensus_metric.as_leader().register_decided();
        } else {
            self.consensus_metric.as_replica().register_decided();
        }

        let decision_node = self
            .decision_log
            .into_current_proposal()
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
            header.current_block_digest,
        )
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
}
