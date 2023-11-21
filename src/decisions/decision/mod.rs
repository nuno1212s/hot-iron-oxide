use std::sync::Arc;
use atlas_common::crypto::hash::Digest;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::smr::smr_decision_log::ShareableMessage;
use crate::decisions::DecisionNode;
use crate::decisions::log::DecisionLog;
use crate::decisions::msg_queue::HotStuffTBOQueue;
use crate::messages::{HotIronOxMsg, HotStuffOrderProtocolMessage};
use crate::messages::serialize::HotIronOxSer;
use crate::view::View;

pub enum DecisionState {
    Prepare(usize),
    PreCommit,
    Commit,
    Decide,
}


/// The decision for a given object
pub struct Decision<D> {
    view: View,
    node_id: NodeId,
    current_state: DecisionState,
    decision_queue: HotStuffTBOQueue,
    decision_log: DecisionLog,
}

/// The result of the poll operation on the decision
pub enum DecisionPollResult {
    TryPropose,
    Recv,
    NextMessage(ShareableMessage<HotIronOxMsg>),
    Decided,
}

/// The decision of the poll operation on the decision
pub enum DecisionResult {
    DuplicateVote(NodeId),
    MessageIgnored,
    MessageQueued,
    DecisionProgressed(ShareableMessage<HotIronOxMsg>),
    Decided(ShareableMessage<HotIronOxMsg>),
    DecidedIgnored,
}

impl<D> Decision<D> {
    /// Poll this decision
    pub fn poll(&mut self) -> DecisionPollResult {
        todo!()
    }

    /// Are we the leader of the current
    fn is_leader(&self) -> bool {
        self.view.primary() == self.node_id
    }

    /// Process a given consensus message
    pub fn process_message<NT>(&mut self, network: Arc<NT>, message: ShareableMessage<HotIronOxMsg>) -> Result<DecisionResult>
        where NT: OrderProtocolSendNode<D, HotIronOxSer<D>> {
        match &mut self.current_state {
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

                    self.current_state = DecisionState::PreCommit;

                    Ok(DecisionResult::DecisionProgressed(message))
                } else {
                    Ok(DecisionResult::DecisionProgressed(message))
                }
            }
            DecisionState::Prepare(received) => {
                match message.message().clone().into_kind() {
                    HotStuffOrderProtocolMessage::NewView(_) => {
                        return Ok(DecisionResult::MessageIgnored);
                    }
                    HotStuffOrderProtocolMessage::Prepare(_, _) => {}
                    HotStuffOrderProtocolMessage::PreCommit(_) => {}
                    HotStuffOrderProtocolMessage::Commit(_) => {}
                    HotStuffOrderProtocolMessage::Decide(_) => {}
                }
            }
            DecisionState::PreCommit => {}
            DecisionState::Commit => {}
            DecisionState::Decide => {}
        }

        Ok(DecisionResult::MessageIgnored)
    }
}