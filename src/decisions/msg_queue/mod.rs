use std::collections::VecDeque;
use atlas_core::ordering_protocol::ShareableMessage;
use crate::messages::{HotFeOxMsg, HotFeOxMsgType, ProposalType, VoteType};

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
    new_view: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    prepare: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    pre_commit: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    commit: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    decide: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
}

impl<D> HotStuffTBOQueue<D> {
    pub fn queue_message(&mut self, message: ShareableMessage<HotFeOxMsg<D>>) {
        self.get_queue = true;

        match message.message().message() {
            HotFeOxMsgType::Proposal(prop) => {
                match prop.proposal_type() {
                    ProposalType::Prepare(_) => {
                        self.prepare.push_back(message);
                    }
                    ProposalType::PreCommit => {
                        self.pre_commit.push_back(message);
                    }
                    ProposalType::Commit => {
                        self.commit.push_back(message);
                    }
                    ProposalType::Decide => {
                        self.decide.push_back(message);
                    }
                }
            }
            HotFeOxMsgType::Vote(vote) => {
                match vote.vote_type() {
                    VoteType::PrepareVote => {
                        self.prepare.push_back(message);
                    }
                    VoteType::PreCommitVote => {
                        self.pre_commit.push_back(message);
                    }
                    VoteType::CommitVote => {
                        self.commit.push_back(message);
                    }
                }
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