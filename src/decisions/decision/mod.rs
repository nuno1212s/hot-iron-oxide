use std::sync::Arc;
use thiserror::Error;
use tracing::error;
use atlas_common::crypto::hash::Digest;
use atlas_common::ordering::{Orderable};
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_common::quiet_unwrap;
use atlas_common::serialization_helper::SerType;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::ordering_protocol::ShareableMessage;
use crate::crypto::{CryptoInformationProvider, get_partial_signature_for_message};
use crate::decisions::{DecisionHandler, DecisionNode, QCType, QC};
use crate::decisions::log::DecisionLog;
use crate::decisions::msg_queue::HotStuffTBOQueue;
use crate::decisions::req_aggr::RequestAggr;
use crate::messages::{HotFeOxMsg, HotFeOxMsgType, ProposalType, ProposalMessage, VoteMessage, VoteType};
use crate::messages::serialize::HotIronOxSer;
use crate::view::View;

pub enum DecisionState {
    Init,
    Prepare(usize),
    PreCommit(usize),
    Commit(usize),
    Decide,
}


/// The decision for a given object
pub struct Decision<D> {
    view: View,
    node_id: NodeId,
    current_state: DecisionState,
    decision_queue: HotStuffTBOQueue<D>,
    decision_log: DecisionLog<D>,
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
    DecisionProgressed(ShareableMessage<HotFeOxMsg<D>>),
    PrepareQC(QC<D>, ShareableMessage<HotFeOxMsg<D>>),
    LockedQC(QC<D>, ShareableMessage<HotFeOxMsg<D>>),
    Decided(ShareableMessage<HotFeOxMsg<D>>),
    DecidedIgnored,
}


impl<RQ> Decision<RQ> 
where RQ: SerType {
    /// Poll this decision
    pub fn poll<NT, CR>(&mut self, network: &Arc<NT>, dec_handler: &DecisionHandler<RQ>, crypto_info: &Arc<CR>) -> DecisionPollResult<RQ>
        where
            NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
            CR: CryptoInformationProvider {
        match self.current_state {
            DecisionState::Init => {
                //TODO: We need to include the prepare QC here
                // Also, should the leader do this?

                atlas_common::threadpool::execute({
                    let network = network.clone();
                    let crypto_info = crypto_info.clone();
                    let latest_qc = dec_handler.latest_qc();
                    let view_leader = self.view.primary();
                    let seq = self.view.sequence_number();
                    
                    move || {
                        let vote_type = VoteType::NewView(latest_qc);

                        let partial_sig = get_partial_signature_for_message(&*crypto_info, seq, &vote_type);

                        quiet_unwrap!(network.send(HotFeOxMsg::new(seq, HotFeOxMsgType::Vote(VoteMessage::new(vote_type, partial_sig))), view_leader, true));
                    }
                });

                self.current_state = DecisionState::Prepare(0);

                DecisionPollResult::Recv
            }
            DecisionState::Prepare(_) if self.decision_queue.should_poll() => {
                todo!()
            }
            DecisionState::Prepare(_) => {
                todo!()
            }
            DecisionState::PreCommit(_) => {
                todo!()
            }
            DecisionState::Commit(_) => {
                todo!()
            }
            DecisionState::Decide => {
                todo!()
            }
        }
    }

    /// Queue a message into this decision, so it can be polled later
    pub(super) fn queue(&mut self, message: ShareableMessage<HotFeOxMsg<RQ>>) {
        self.decision_queue.queue_message(message)
    }

    /// Are we the leader of the current view
    fn is_leader(&self) -> bool {
        self.view.primary() == self.node_id
    }

    fn leader(&self) -> NodeId {
        self.view.primary()
    }
    
    pub(super) fn can_be_finalized(&self) -> bool {
        todo!()
    }

    /// Process a given consensus message
    pub fn process_message<NT, CR>(&mut self, network: Arc<NT>,
                                   message: ShareableMessage<HotFeOxMsg<RQ>>,
                                   dec_handler: &DecisionHandler<RQ>,
                                   crypto: &Arc<CR>,
                                   req_aggr: &Arc<RequestAggr<RQ>>) -> Result<DecisionResult<RQ>>
        where NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
              CR: CryptoInformationProvider {
        let is_leader = self.is_leader();
        
        match &mut self.current_state {
            DecisionState::Init => unreachable!(),
            DecisionState::Prepare(received) if is_leader => {
                let (i, qc, signature) = match message.message().clone().into() {
                    HotFeOxMsgType::Vote(vote_msg)
                    if self.view.sequence_number() == message.sequence_number() => {
                        match vote_msg.into_inner() {
                            (VoteType::NewView(qc), signature) => {
                                (*received + 1, qc, signature)
                            }
                            _ => {
                                self.decision_queue.queue_message(message);

                                return Ok(DecisionResult::MessageQueued);
                            }
                        }
                    }
                    HotFeOxMsgType::Proposal(prop_msg) => {
                        // Leaders create the proposals, they never receive them, so
                        // All proposal messages are dropped
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    _ => {
                        // The received message does not match our sequence number

                        return Ok(DecisionResult::MessageIgnored);
                    }
                };

                let leader_log = self.decision_log.as_mut_leader().unwrap();
                
                *received = i;

                if let Some(qc) = qc {
                    leader_log.new_view_store().accept_new_view(qc)?;;
                }
                
                let digest = Digest::blank();

                if *received >= self.view.quorum() {
                    let qc = leader_log.new_view_store().get_high_qc();

                    let node = if let Some(qc) = qc {
                        DecisionNode::create_leaf(qc.decision_node(), digest, req_aggr.take_pool_requests())
                    } else {
                        DecisionNode::create_root_leaf(&self.view, digest, req_aggr.take_pool_requests())
                    };

                    let msg = HotFeOxMsgType::Proposal(ProposalMessage::new(ProposalType::Prepare(node, qc.unwrap().clone())));

                    let _ = network.broadcast(HotFeOxMsg::new(self.view.sequence_number(), msg), self.view.quorum_members().clone().into_iter());

                    self.current_state = DecisionState::PreCommit(0);
                }

                Ok(DecisionResult::DecisionProgressed(message))
            }
            DecisionState::Prepare(_) => {
                let (node, qc) = match message.message().clone().into() {
                    HotFeOxMsgType::Proposal(proposal)
                    if self.view.sequence_number() == message.message().sequence_number()
                        && message.header().from() == self.leader() => {
                        match proposal.into() {
                            ProposalType::Prepare(node, qc) => {
                                (node, qc)
                            }
                            _ => {
                                self.decision_queue.queue_message(message);

                                return Ok(DecisionResult::MessageQueued);
                            }
                        }
                    }
                    HotFeOxMsgType::Vote(_) | HotFeOxMsgType::Proposal(_) => {
                        // A non leader can never receive vote messages, they can only receive
                        // Proposals which they can vote on

                        return Ok(DecisionResult::MessageIgnored);
                    }
                };

                if dec_handler.safe_node(&node, &qc) &&
                    node.extends_from(qc.decision_node())
                {
                    atlas_common::threadpool::execute({
                        let network = network.clone();
                        
                        let crypto = crypto.clone();
                        
                        let view = self.view.clone();
                        
                        move || {
                            // Send the message signing processing to the threadpool 
                            let prepare_vote = VoteType::PrepareVote(node);

                            let msg_signature = get_partial_signature_for_message(&*crypto, view.sequence_number(), &prepare_vote);

                            let vote_msg = HotFeOxMsgType::Vote(VoteMessage::new(prepare_vote, msg_signature));

                            let _ = network.send(HotFeOxMsg::new(view.sequence_number(), vote_msg), view.primary(), true);
                        }
                    });

                    self.current_state = DecisionState::PreCommit(0);

                    Ok(DecisionResult::DecisionProgressed(message))
                } else {
                    Ok(DecisionResult::MessageIgnored)
                }
            }
            DecisionState::PreCommit(received) if is_leader => {
                let vote_msg = match message.message().message() {
                    HotFeOxMsgType::Vote(vote_msg) if message.message().sequence_number() == self.view.sequence_number() => {
                        match vote_msg.vote_type() {
                            VoteType::PrepareVote(_) => {
                                vote_msg.clone()
                            }
                            _ => {
                                self.decision_queue.queue_message(message);

                                return Ok(DecisionResult::MessageQueued);
                            }
                        }
                    }
                    HotFeOxMsgType::Proposal(_) => {
                        // Leaders do not receive proposals
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    _ => {
                        return Ok(DecisionResult::MessageIgnored);
                    }
                };

                *received += 1;

                let leader_log = self.decision_log.as_mut_leader().expect("Leader decision log not available");
                
                leader_log.accept_vote(vote_msg)?;

                if *received >= self.view.quorum() {
                    let created_qc = leader_log.generate_qc(&**crypto, self.view.sequence_number(), QCType::PrepareVote)?;

                    let view_seq = self.view.sequence_number();

                    atlas_common::threadpool::execute({
                        let view_members = self.view.quorum_members().clone();
                        let created_qc = created_qc.clone();
                        
                        move || {
                            let prop = ProposalType::PreCommit(created_qc);

                            let proposal_message = ProposalMessage::new(prop);

                            let msg = HotFeOxMsg::new(view_seq, HotFeOxMsgType::Proposal(proposal_message));

                            let _ = network.broadcast(msg, view_members.into_iter());
                        }
                    });

                    self.current_state = DecisionState::Commit(0);

                    Ok(DecisionResult::PrepareQC(created_qc, message))
                } else {
                    Ok(DecisionResult::DecisionProgressed(message))
                }
            }
            DecisionState::PreCommit(_) => {
                let qc = match message.message().message() {
                    HotFeOxMsgType::Proposal(prop)
                    if message.message().sequence_number() == self.view.sequence_number()
                        && message.header().from() == self.leader() => {
                        match prop.proposal_type() {
                            ProposalType::PreCommit(pre_commit) => {
                                pre_commit.clone()
                            }
                            ProposalType::Prepare(_, _) => {
                                return Ok(DecisionResult::MessageIgnored);
                            }
                            ProposalType::Commit(_) | ProposalType::Decide(_) => {
                                self.decision_queue.queue_message(message);

                                return Ok(DecisionResult::MessageQueued);
                            }
                        }
                    }
                    _ => return Ok(DecisionResult::MessageIgnored)
                };


                self.current_state = DecisionState::Commit(0);

                Ok(DecisionResult::PrepareQC(qc, message))
            }
            DecisionState::Commit(_) if is_leader => {
                todo!()
            }
            DecisionState::Commit(_) => {
                self.current_state = DecisionState::Decide;
                
                todo!();
                //Ok(DecisionResult::LockedQC(signature, message))
            }
            DecisionState::Decide => {
                todo!()
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum DecisionError {
    #[error("Received a prepare message with an invalid qc")]
    PrepareMessageWithEmptyQC,
    #[error("No QC was available to create the decision")]
    NoQCAvailable,
}