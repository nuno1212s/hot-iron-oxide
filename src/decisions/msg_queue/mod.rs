use crate::messages::{HotFeOxMsg, HotFeOxMsgType, ProposalType, VoteType};
use atlas_core::ordering_protocol::ShareableMessage;
use std::collections::VecDeque;

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
    pub new_view: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    pub prepare: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    pub pre_commit: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    pub commit: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
    pub decide: VecDeque<ShareableMessage<HotFeOxMsg<D>>>,
}

impl<D> HotStuffTBOQueue<D> {
    pub fn queue_message(&mut self, message: ShareableMessage<HotFeOxMsg<D>>) {
        self.get_queue = true;

        match message.message().message() {
            HotFeOxMsgType::Proposal(prop) => match prop.proposal_type() {
                ProposalType::Prepare(_, _) => {
                    self.prepare.push_back(message);
                }
                ProposalType::PreCommit(_) => {
                    self.pre_commit.push_back(message);
                }
                ProposalType::Commit(_) => {
                    self.commit.push_back(message);
                }
                ProposalType::Decide(_) => {
                    self.decide.push_back(message);
                }
            },
            HotFeOxMsgType::Vote(vote) => match vote.vote_type() {
                VoteType::PrepareVote(_) => {
                    self.prepare.push_back(message);
                }
                VoteType::PreCommitVote(_) => {
                    self.pre_commit.push_back(message);
                }
                VoteType::CommitVote(_) => {
                    self.commit.push_back(message);
                }
                VoteType::NewView(_) => {
                    self.new_view.push_back(message);
                }
            },
        }
    }

    pub fn signal(&mut self) {
        self.get_queue = true;
    }

    pub fn should_poll(&self) -> bool {
        self.get_queue
    }
}

impl<RQ> Default for HotStuffTBOQueue<RQ> {
    fn default() -> Self {
        Self {
            get_queue: false,
            new_view: Default::default(),
            prepare: Default::default(),
            pre_commit: Default::default(),
            commit: Default::default(),
            decide: Default::default(),
        }
    }
}
