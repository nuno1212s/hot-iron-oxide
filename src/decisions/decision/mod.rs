use std::sync::Arc;
use thiserror::Error;
use atlas_common::crypto::hash::Digest;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::smr::smr_decision_log::ShareableMessage;
use crate::decisions::{DecisionHandler, DecisionNode};
use crate::decisions::log::DecisionLog;
use crate::decisions::msg_queue::HotStuffTBOQueue;
use crate::messages::{HotIronOxMsg, HotStuffOrderProtocolMessage, QC};
use crate::messages::serialize::HotIronOxSer;
use crate::view::View;

pub enum DecisionState {
    Init,
    Prepare(usize),
    PreCommit(usize),
    Commit,
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
    NextMessage(ShareableMessage<HotIronOxMsg<D>>),
    Decided,
}

/// The decision of the poll operation on the decision
pub enum DecisionResult<D> {
    DuplicateVote(NodeId),
    MessageIgnored,
    MessageQueued,
    DecisionProgressed(ShareableMessage<HotIronOxMsg<D>>),
    PrepareQC(QC<D>, ShareableMessage<HotIronOxMsg<D>>),
    LockedQC(QC<D>, ShareableMessage<HotIronOxMsg<D>>),
    Decided(ShareableMessage<HotIronOxMsg<D>>),
    DecidedIgnored,
}

impl<D> Decision<D> {
    /// Poll this decision
    pub fn poll<NT>(&mut self, network: &Arc<NT>, dec_handler: &DecisionHandler<D>) -> DecisionPollResult<D> where NT: OrderProtocolSendNode<D, HotIronOxSer<D>> {
        match self.current_state {
            DecisionState::Init => {
                //TODO: We need to include the prepare QC here
                // Also, should the leader do this?
                let view_leader = self.view.primary();

                network.send(HotIronOxMsg::message(self.view.sequence_number(), HotStuffOrderProtocolMessage::NewView(dec_handler.latest_qc())), view_leader, true);

                self.current_state = DecisionState::Prepare(0);
            }
            DecisionState::Prepare(_) => {}
            DecisionState::PreCommit(_) => {}
            DecisionState::Commit => {}
            DecisionState::Decide => {}
        }

        todo!()
    }

    /// Are we the leader of the current view
    fn is_leader(&self) -> bool {
        self.view.primary() == self.node_id
    }

    /// Process a given consensus message
    pub fn process_message<NT>(&mut self, network: Arc<NT>, message: ShareableMessage<HotIronOxMsg<D>>, dec_handler: &DecisionHandler<D>) -> Result<DecisionResult<D>>
        where NT: OrderProtocolSendNode<D, HotIronOxSer<D>> {
        return match &mut self.current_state {
            DecisionState::Init => unreachable!(),
            DecisionState::Prepare(received) if self.is_leader() => {
                let (i, quorum_certificate) = match message.message().clone().into_kind() {
                    HotStuffOrderProtocolMessage::NewView(qc) if self.view.sequence_number() != message.sequence_number() - 1 => {
                        (*received + 1, qc)
                    }
                    HotStuffOrderProtocolMessage::NewView(qc) => {
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    _ => {
                        self.decision_queue.queue_message(message);

                        return Ok(DecisionResult::MessageQueued);
                    }
                };

                *received = i;

                if let Some(certificate) = quorum_certificate {
                    self.decision_log.handle_new_view_prepareQC_received(&self.view, certificate);
                }

                let digest = Digest::blank();

                if *received >= self.view.quorum() {
                    let qc = self.decision_log.populate_highest_prepareQC(&self.view)?;

                    let node = if let Some(qc) = qc {
                        DecisionNode::create_leaf(qc.decision_node(), digest, Vec::new())
                    } else {
                        DecisionNode::create_root_leaf(&self.view, digest, Vec::new())
                    };

                    let _ = network.broadcast(HotIronOxMsg::message(self.view.sequence_number(), HotStuffOrderProtocolMessage::Prepare(node, qc.cloned())), self.view.quorum_members().clone().into_iter());

                    self.current_state = DecisionState::PreCommit(0);

                    Ok(DecisionResult::DecisionProgressed(message))
                } else {
                    Ok(DecisionResult::DecisionProgressed(message))
                }
            }
            DecisionState::Prepare(received) => {
                let (node, qc) = match message.message().clone().into_kind() {
                    HotStuffOrderProtocolMessage::NewView(_) => {
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    HotStuffOrderProtocolMessage::Prepare(node, qc) if message.header().from() == self.view.primary() => {
                        //TODO: Handle the first view, in which the previous highest QC is going to be null?
                        (node, qc.ok_or(DecisionError::PrepareMessageWithEmptyQC)?)
                    }
                    HotStuffOrderProtocolMessage::Prepare(_, _) => {
                        // Prepare messages from anyone other than the leader are ignored
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    _ => {
                        self.decision_queue.queue_message(message);

                        return Ok(DecisionResult::MessageQueued);
                    }
                };

                if dec_handler.safe_node(&node, &qc) &&
                    node.extends_from(qc.decision_node())
                {
                    let sig = todo!();

                    network.send(HotIronOxMsg::message(self.view.sequence_number(), HotStuffOrderProtocolMessage::PrepareVote(node, sig)), self.view.primary(), true);

                    self.current_state = DecisionState::PreCommit(0);

                    Ok(DecisionResult::DecisionProgressed(message))
                } else {
                    Ok(DecisionResult::MessageIgnored)
                }
            }
            DecisionState::PreCommit(received) if self.is_leader() => {
                let (node, signature) = match message.message().clone().into_kind() {
                    HotStuffOrderProtocolMessage::NewView(_) => {
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    HotStuffOrderProtocolMessage::Prepare(_, _) => {
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    HotStuffOrderProtocolMessage::PrepareVote(node, partial_sig) => {
                        (node, partial_sig)
                    }
                    _ => {
                        self.decision_queue.queue_message(message);

                        return Ok(DecisionResult::MessageQueued);
                    }
                };

                *received += 1;

                self.decision_log.queue_prepare_vote(&self.view, &node, signature);

                if *received >= self.view.quorum() {
                    let result = self.decision_log.make_prepare_certificate(&self.view)?;

                    network.broadcast(HotIronOxMsg::message(self.view.sequence_number(), HotStuffOrderProtocolMessage::PreCommit(result.clone())), self.view.quorum_members().clone().into_iter());

                    self.current_state = DecisionState::Commit;

                    Ok(DecisionResult::PrepareQC(result, message))
                } else {
                    Ok(DecisionResult::DecisionProgressed(message))
                }
            }
            DecisionState::PreCommit(_) => {
                let qc = match message.message().clone().into_kind() {
                    HotStuffOrderProtocolMessage::NewView(_) | HotStuffOrderProtocolMessage::Prepare(_, _) | HotStuffOrderProtocolMessage::PrepareVote(_, _) =>
                        return Ok(DecisionResult::MessageIgnored),
                    HotStuffOrderProtocolMessage::PreCommit(qc) => qc,
                    _ => {
                        self.decision_queue.queue_message(message);

                        return Ok(DecisionResult::MessageQueued);
                    }
                };

                self.current_state = DecisionState::Commit;

                Ok(DecisionResult::PrepareQC(qc, message))
            }
            DecisionState::Commit if self.is_leader() => {



            }
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
    PrepareMessageWithEmptyQC
}