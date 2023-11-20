use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_core::smr::smr_decision_log::ShareableMessage;
use crate::messages::{HotStuffMessage, HotStuffOrderProtocolMessage};
use crate::view::View;

pub enum DecisionState {
    Prepare(usize),
    PreCommit,
    Commit,
    Decide,
}

pub struct DecisionLog {
    new_view_msgs: Vec<ShareableMessage<HotStuffMessage>>,
}

pub struct Decision {
    view: View,
    current_state: DecisionState,
}

pub enum DecisionResult {
    DuplicateVote(NodeId),
    MessageIgnored,
    MessageQueued,
    DecisionProgressed(ShareableMessage<HotStuffMessage>),
    Decided(ShareableMessage<HotStuffMessage>),
    DecidedIgnored
}

impl Decision {

    pub fn process_message(&mut self, message: ShareableMessage<HotStuffMessage>) -> Result<DecisionResult> {
        match &mut self.current_state {
            DecisionState::Prepare(received) => {

                let i = match message.message().kind() {
                    HotStuffOrderProtocolMessage::NewView(qc) if self.view.sequence_number() != message.sequence_number() - 1 => {
                        *received + 1
                    }
                    HotStuffOrderProtocolMessage::NewView(qc) => {

                    }
                    HotStuffOrderProtocolMessage::Prepare(_) => {}
                    HotStuffOrderProtocolMessage::PreCommit(_) => {}
                    HotStuffOrderProtocolMessage::Commit(_) => {}
                    HotStuffOrderProtocolMessage::Decide(_) => {}
                };

            }
            DecisionState::PreCommit => {}
            DecisionState::Commit => {}
            DecisionState::Decide => {}
        }

        Ok(DecisionResult::MessageIgnored)
    }

}