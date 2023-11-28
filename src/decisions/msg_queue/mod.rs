use std::collections::VecDeque;
use atlas_common::ordering::tbo_queue_message;
use atlas_core::smr::smr_decision_log::ShareableMessage;
use crate::messages::{HotIronOxMsg, HotStuffOrderProtocolMessage};


macro_rules! extract_msg {
    ($g:expr, $q:expr) => {
        extract_msg!(DecisionPollStatus::Recv, $g, $q)
    };
    ($rsp:expr, $g:expr, $q:expr) => {
        if let Some(stored) = $q.pop_front() {

            DecisionPollStatus::NextMessage(stored)
        } else {
            *$g = false;
            $rsp
        }
    };
}


pub struct HotStuffTBOQueue<D> {
    get_queue: bool,
    new_view: VecDeque<ShareableMessage<HotIronOxMsg<D>>>,
    prepare: VecDeque<ShareableMessage<HotIronOxMsg<D>>>,
    pre_commit: VecDeque<ShareableMessage<HotIronOxMsg<D>>>,
    commit: VecDeque<ShareableMessage<HotIronOxMsg<D>>>,
    decide: VecDeque<ShareableMessage<HotIronOxMsg<D>>>,
}

impl<D> HotStuffTBOQueue<D> {
    pub fn queue_message(&mut self, message: ShareableMessage<HotIronOxMsg<D>>) {
        self.get_queue = true;

        match message.message().kind() {
            HotStuffOrderProtocolMessage::NewView(_) => {
                self.new_view.push_back(message)
            }
            HotStuffOrderProtocolMessage::Prepare(_, _) => {
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

    pub fn signal(&mut self) {
        self.get_queue = true;
    }

    pub fn should_poll(&self) -> bool {
        self.get_queue
    }
}