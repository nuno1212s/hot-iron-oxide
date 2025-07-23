use atlas_core::ordering_protocol::ShareableMessage;
use enum_map::EnumMap;
use std::collections::VecDeque;
use crate::hot_iron::messages::{HotFeOxMsg, HotFeOxMsgType, VoteTypes, ProposalTypes};
use crate::hot_iron::protocol::decision::DecisionState;

pub struct HotStuffTBOQueue<D> {
    get_queue: bool,
    vote_messages: EnumMap<VoteTypes, VecDeque<ShareableMessage<HotFeOxMsg<D>>>>,
    proposal_messages: EnumMap<ProposalTypes, VecDeque<ShareableMessage<HotFeOxMsg<D>>>>,
}

impl<D> HotStuffTBOQueue<D> {
    pub fn queue_message(&mut self, message: ShareableMessage<HotFeOxMsg<D>>) {
        self.get_queue = true;

        match message.message().message() {
            HotFeOxMsgType::Proposal(prop_message) => {
                self.proposal_messages[prop_message.proposal_type().into()].push_back(message);
            }
            HotFeOxMsgType::Vote(vote_message) => {
                self.vote_messages[vote_message.vote_type().into()].push_back(message);
            }
        }
    }

    pub fn should_poll(&self) -> bool {
        self.get_queue
    }

    fn get_message_for_type(
        &mut self,
        is_leader: bool,
        proposal_types: ProposalTypes,
        vote_types: VoteTypes,
    ) -> Option<ShareableMessage<HotFeOxMsg<D>>> {
        if is_leader {
            self.proposal_queue(proposal_types)
                .pop_front()
                .or_else(|| self.vote_queue(vote_types).pop_front())
        } else {
            self.vote_queue(vote_types).pop_front()
        }
    }

    pub fn get_message_for(
        &mut self,
        is_leader: bool,
        decision_state: &DecisionState,
    ) -> Option<ShareableMessage<HotFeOxMsg<D>>> {
        let decision_result = match decision_state {
            DecisionState::Prepare(_, _) => {
                self.get_message_for_type(is_leader, ProposalTypes::Prepare, VoteTypes::NewView)
            }
            DecisionState::PreCommit(_, _) => self.get_message_for_type(
                is_leader,
                ProposalTypes::PreCommit,
                VoteTypes::PrepareVote,
            ),
            DecisionState::Commit(_, _) => self.get_message_for_type(
                is_leader,
                ProposalTypes::Commit,
                VoteTypes::PreCommitVote,
            ),
            DecisionState::Decide(_, _) => {
                self.get_message_for_type(is_leader, ProposalTypes::Decide, VoteTypes::NewView)
            }
            _ => None,
        };

        if decision_result.is_none() {
            self.get_queue = false;
        }

        decision_result
    }

    fn proposal_queue(
        &mut self,
        proposal_type: ProposalTypes,
    ) -> &mut VecDeque<ShareableMessage<HotFeOxMsg<D>>> {
        &mut self.proposal_messages[proposal_type]
    }

    fn vote_queue(
        &mut self,
        vote_type: VoteTypes,
    ) -> &mut VecDeque<ShareableMessage<HotFeOxMsg<D>>> {
        &mut self.vote_messages[vote_type]
    }
}

impl<D> Default for HotStuffTBOQueue<D> {
    fn default() -> Self {
        Self {
            get_queue: false,
            vote_messages: EnumMap::default(),
            proposal_messages: EnumMap::default(),
        }
    }
}
