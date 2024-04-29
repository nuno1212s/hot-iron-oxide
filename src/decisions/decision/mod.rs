use std::sync::Arc;
use thiserror::Error;
use atlas_common::crypto::hash::Digest;
use atlas_common::ordering::{Orderable};
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_common::quiet_unwrap;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::ordering_protocol::ShareableMessage;
use crate::crypto::{CryptoInformationProvider, get_partial_signature_for_message};
use crate::decisions::{DecisionHandler, DecisionNode};
use crate::decisions::log::DecisionLog;
use crate::decisions::msg_queue::HotStuffTBOQueue;
use crate::messages::{HotFeOxMsg, HotFeOxMsgType, ProposalType, QC, ProposalMessage, VoteMessage, VoteType};
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


impl<D> Decision<D> {
    /// Poll this decision
    pub fn poll<NT, CR>(&mut self, network: &Arc<NT>, dec_handler: &DecisionHandler<D>, crypto_info: &CR) -> DecisionPollResult<D>
        where
            NT: OrderProtocolSendNode<D, HotIronOxSer<D>>,
            CR: CryptoInformationProvider {
        match self.current_state {
            DecisionState::Init => {
                //TODO: We need to include the prepare QC here
                // Also, should the leader do this?
                let view_leader = self.view.primary();
                let seq = self.view.sequence_number();

                atlas_common::threadpool::execute(move || {
                    let vote_type = VoteType::NewView(dec_handler.latest_qc());

                    let partial_sig = get_partial_signature_for_message(crypto_info, seq, &vote_type);

                    quiet_unwrap!(network.send(HotFeOxMsg::new(seq, HotFeOxMsgType::Vote(VoteMessage::new(vote_type, partial_sig))), view_leader, true));
                });

                self.current_state = DecisionState::Prepare(0);

                DecisionPollResult::Recv
            }
            DecisionState::Prepare(_) => {
                todo!()
            }
            DecisionState::PreCommit(_) => {
                todo!()
            }
            DecisionState::Commit => {
                todo!()
            }
            DecisionState::Decide => {
                todo!()
            }
        }
    }

    /// Are we the leader of the current view
    fn is_leader(&self) -> bool {
        self.view.primary() == self.node_id
    }

    fn leader(&self) -> NodeId {
        self.view.primary()
    }

    /// Process a given consensus message
    pub fn process_message<NT, CR>(&mut self, network: Arc<NT>,
                                   message: ShareableMessage<HotFeOxMsg<D>>,
                                   dec_handler: &DecisionHandler<D>,
                                   crypto: &CR) -> Result<DecisionResult<D>>
        where NT: OrderProtocolSendNode<D, HotIronOxSer<D>>,
              CR: CryptoInformationProvider {
        return match &mut self.current_state {
            DecisionState::Init => unreachable!(),
            DecisionState::Prepare(received) if self.is_leader() => {
                let (i, quorum_certificate) = match message.message().clone().into() {
                    HotFeOxMsgType::Vote(vote_msg)
                    if self.view.sequence_number() == message.sequence_number() => {
                        match vote_msg.into() {
                            VoteType::NewView(qc) => {
                                (received + 1, qc)
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

                *received = i;

                if let Some(certificate) = quorum_certificate {
                    self.decision_log.handle_new_view_prepare_QC_received(&self.view, certificate);
                }

                let digest = Digest::blank();

                if *received >= self.view.quorum() {
                    let qc = self.decision_log.populate_highest_prepare_QC(&self.view)?
                        .ok_or(DecisionError::NoQCAvailable)?;

                    let node = if let Some(qc) = qc {
                        DecisionNode::create_leaf(qc.decision_node(), digest, Vec::new())
                    } else {
                        DecisionNode::create_root_leaf(&self.view, digest, Vec::new())
                    };

                    let msg = HotFeOxMsgType::Proposal(ProposalMessage::new(ProposalType::Prepare(node, qc.cloned())));

                    let _ = network.broadcast(HotFeOxMsg::new(self.view.sequence_number(), msg), self.view.quorum_members().clone().into_iter());

                    self.current_state = DecisionState::PreCommit(0);
                }

                Ok(DecisionResult::DecisionProgressed(message))
            }
            DecisionState::Prepare(_) => {
                let (node, qc) = match message.message().clone().into_kind() {
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
                    atlas_common::threadpool::execute(|| {
                        // Send the message signing processing to the threadpool

                        let prepare_vote = VoteType::PrepareVote(node);

                        let msg_signature = get_partial_signature_for_message(crypto, self.view.sequence_number(), &prepare_vote);

                        let vote_msg = HotFeOxMsgType::Vote(VoteMessage::new(prepare_vote, msg_signature));

                        let _ = network.send(HotFeOxMsg::new(self.view.sequence_number(), vote_msg), self.view.primary(), true);
                    });

                    self.current_state = DecisionState::PreCommit(0);

                    Ok(DecisionResult::DecisionProgressed(message))
                } else {
                    Ok(DecisionResult::MessageIgnored)
                }
            }
            DecisionState::PreCommit(received) if self.is_leader() => {
                let (node, signature) = match message.message().message() {
                    HotFeOxMsgType::Vote(vote_msg) if message.message().sequence_number() == self.view.sequence_number() => {
                        match vote_msg.vote_type() {
                            VoteType::PrepareVote(vote) => {
                                vote_msg.clone().into_inner()
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

                self.decision_log.queue_prepare_vote(&self.view, &node, signature);

                if *received >= self.view.quorum() {
                    let created_qc = self.decision_log.make_prepare_certificate(&self.view)?;

                    let view_seq = self.view.sequence_number();

                    atlas_common::threadpool::execute(move || {
                        let prop = ProposalType::PreCommit(created_qc.clone());

                        let msg = HotFeOxMsg::new(view_seq, HotFeOxMsgType::Proposal(prop));

                        let _ = network.broadcast(msg, self.view.quorum_members().clone().into_iter());
                    });

                    self.current_state = DecisionState::Commit;

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

                self.decision_log.

                self.current_state = DecisionState::Commit;

                Ok(DecisionResult::PrepareQC(qc, message))
            }
            DecisionState::Commit if self.is_leader() => {}
            DecisionState::Commit => {
                let signature = match message.message().clone().into_kind() {
                    HotStuffOrderProtocolMessage::NewView(_) |
                    HotStuffOrderProtocolMessage::Prepare(_, _) |
                    HotStuffOrderProtocolMessage::PrepareVote(_, _) |
                    HotStuffOrderProtocolMessage::PreCommit(_) =>
                        return Ok(DecisionResult::MessageIgnored),
                    HotStuffOrderProtocolMessage::Commit(sig) => sig,
                    _ => {
                        self.decision_queue.queue_message(message);

                        return Ok(DecisionResult::MessageQueued);
                    }
                };

                self.current_state = DecisionState::Decide;

                //Ok(DecisionResult::LockedQC(signature, message))
            }
            DecisionState::Decide => {}
        };
    }
}

#[derive(Error, Debug)]
pub enum DecisionError {
    #[error("Received a prepare message with an invalid qc")]
    PrepareMessageWithEmptyQC,
    #[error("No QC was available to create the decision")]
    NoQCAvailable,
}