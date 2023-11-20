use std::collections::VecDeque;
use atlas_common::ordering::tbo_queue_message;
use atlas_core::smr::smr_decision_log::ShareableMessage;
use crate::messages::{HotStuffMessage, HotStuffOrderProtocolMessage};

pub struct HotStuffTBOQueue {
    get_queue: bool,
    new_view: VecDeque<ShareableMessage<HotStuffMessage>>,
    prepare: VecDeque<ShareableMessage<HotStuffMessage>>,
    pre_commit: VecDeque<ShareableMessage<HotStuffMessage>>,
    commit: VecDeque<ShareableMessage<HotStuffMessage>>,
    decide: VecDeque<ShareableMessage<HotStuffMessage>>
}

impl HotStuffTBOQueue {

    pub fn queue_message(&mut self, message: ShareableMessage<HotStuffMessage>) {
        self.get_queue = true;

        match message.message().kind() {
            HotStuffOrderProtocolMessage::NewView(_) => {
                self.new_view.push_back(message)
            }
            HotStuffOrderProtocolMessage::Prepare(_) => {
                self.prepare.push_back(message)
            }
            HotStuffOrderProtocolMessage::PreCommit(_) => {
                self.pre_commit.push_back(message)
            }
            HotStuffOrderProtocolMessage::Commit(_) => {
                self.commit.push_back(message)
            }
            HotStuffOrderProtocolMessage::Decide(_) => {
                self.decide.push_back(message)
            }
        }
    }

}