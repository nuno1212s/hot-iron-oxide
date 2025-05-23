mod decision;
mod log;
mod msg_queue;

use crate::chained::chained_decision_tree::{ChainedDecisionNode, PendingDecisionNodes};
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::IronChainMessage;
use crate::chained::protocol::decision::{
    ChainedDecision, ChainedDecisionResult, ChainedDecisionState, FinalizeDecisionError,
};
use crate::chained::{
    ChainedDecisionHandler, IronChainDecision, IronChainPollResult, IronChainResult,
};
use crate::crypto::{CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionHandler, DecisionNodeHeader, TQuorumCertificate};
use crate::protocol::hotstuff::Signals;
use crate::req_aggr::ReqAggregator;
use crate::view::View;
use atlas_common::crypto::hash::Digest;
use atlas_common::maybe_vec::MaybeVec;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{InvalidSeqNo, Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::messages::{ClientRqInfo, SessionBased};
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{
    BatchedDecision, Decision, DecisionsAhead, OPPollResult, ProtocolConsensusDecision,
    ShareableMessage,
};
use atlas_core::timeouts::timeout::TimeoutModHandle;
use either::Either;
use std::collections::VecDeque;
use std::sync::Arc;
use thiserror::Error;
use tracing::error;

const PROTOCOL_PHASES: u8 = 4;
const PROTOCOL_PHASES_USIZE: usize = PROTOCOL_PHASES as usize;

pub(crate) struct ChainedHotStuffProtocol<RQ, RQA> {
    node_id: NodeId,

    decision_handler: ChainedDecisionHandler,

    request_aggr: Arc<RQA>,

    timeouts: TimeoutModHandle,
    decision_list: VecDeque<ChainedDecision<RQ>>,
    signal: Signals,
    pending_nodes: PendingDecisionNodes<RQ>,
    curr_view: View,
    last_decided_view: Option<View>,
}

impl<RQ, RQA> Orderable for ChainedHotStuffProtocol<RQ, RQA> {
    fn sequence_number(&self) -> SeqNo {
        self.curr_view.sequence_number()
    }
}

impl<RQ, RQA> ChainedHotStuffProtocol<RQ, RQA>
where
    RQ: SerMsg,
    RQA: ReqAggregator<RQ>,
{
    #[must_use]
    pub fn new(
        node_id: NodeId,
        rq_aggr: Arc<RQA>,
        timeouts: TimeoutModHandle,
        mut curr_view: View,
    ) -> Self {
        let mut decision_list = VecDeque::with_capacity(PROTOCOL_PHASES as usize);

        for _ in 0..PROTOCOL_PHASES {
            let next_view = curr_view.next_view();

            let chained_decision = ChainedDecision::new(curr_view, node_id);

            decision_list.push_back(chained_decision);

            curr_view = next_view;
        }

        Self {
            node_id,
            decision_handler: DecisionHandler::default(),
            request_aggr: rq_aggr,
            timeouts,
            decision_list,
            signal: Signals::default(),
            pending_nodes: PendingDecisionNodes::default(),
            curr_view,
            last_decided_view: None,
        }
    }

    fn get_decision(&mut self, seq_no: SeqNo) -> Option<&mut ChainedDecision<RQ>> {
        match seq_no.index(self.sequence_number()) {
            Either::Right(index) => self.decision_list.get_mut(index),
            Either::Left(_) => None,
        }
    }

    pub(crate) fn view(&self) -> &View {
        &self.curr_view
    }
    
    pub(crate) fn install_view(&mut self, view: View) {
        self.curr_view = view;
    }

    pub(super) fn poll(&mut self) -> IronChainPollResult<RQ> {
        while let Some(seq) = self.signal.pop_signalled() {
            let Some(decision) = self.get_decision(seq) else {
                error!("Decision not found for seq_no: {seq:?}");
                continue;
            };

            let poll_result = decision.poll();

            match poll_result {
                OPPollResult::RunCst => {
                    return IronChainPollResult::RunCst;
                }
                OPPollResult::ReceiveMsg => return IronChainPollResult::ReceiveMsg,
                OPPollResult::Exec(message) => (),
                OPPollResult::ProgressedDecision(_, _) => (),
                OPPollResult::QuorumJoined(_, _, _) => (),
                OPPollResult::RePoll => (),
            }
        }

        todo!()
    }

    pub(super) fn process_message<CR, CP, NT>(
        &mut self,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> Result<IronChainResult<RQ>, IronChainProcessError>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
        RQ: SessionBased,
    {
        let decision_index = match message
            .sequence_number()
            .index(self.curr_view.sequence_number())
        {
            Either::Right(index) => index,
            Either::Left(_) => return Ok(IronChainResult::MessageDropped),
        };

        let Some(decision) = self.decision_list.get_mut(decision_index) else {
            //TODO: Queue message

            return Ok(IronChainResult::MessageQueued);
        };

        let result = decision.process_message::<CR, CP, NT>(
            &mut self.decision_handler,
            crypto,
            network,
            message,
        );

        match result {
            ChainedDecisionResult::ProposalGenerated(proposal_header, message) => {
                Ok(IronChainResult::ProgressedDecision(
                    DecisionsAhead::Ignore,
                    MaybeVec::from_one(Decision::decision_info_from_metadata_and_messages(
                        decision.sequence_number(),
                        proposal_header,
                        MaybeVec::None,
                        MaybeVec::from_one(message),
                    )),
                ))
            }
            ChainedDecisionResult::VoteGenerated(node, message) => {
                self.handle_vote_generated(decision_index, node, message)?
            }
            ChainedDecisionResult::MessageProcessed(_) => {
                Ok(IronChainResult::MessageProcessedNoUpdate)
            }
            ChainedDecisionResult::MessageIgnored => Ok(IronChainResult::MessageDropped),
        }
    }

    fn handle_vote_generated(
        &mut self,
        decision_index: usize,
        node: ChainedDecisionNode<RQ>,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> Result<Result<IronChainResult<RQ>, IronChainProcessError>, IronChainProcessError>
    where
        RQ: SessionBased,
    {
        let decision = self
            .decision_list
            .get(decision_index)
            .ok_or(IronChainProcessError::GetDecision(decision_index))?;
        let header = *node.decision_header();
        let chained_qc = MaybeVec::from_option(node.justify().clone());

        self.pending_nodes.insert(node);

        let decision_information = Decision::decision_info_from_metadata_and_messages(
            decision.sequence_number(),
            header,
            chained_qc,
            MaybeVec::from_one(message),
        );

        Ok(if self.node_chain(&header, decision_index)? {
            let finalized_decisions = self.finalize()?;

            let mut vec = Vec::with_capacity(finalized_decisions.len());

            vec.push(decision_information);

            vec.append(&mut finalized_decisions.into_vec());

            let vec = MaybeVec::from_many(vec);

            Ok(IronChainResult::ProgressedDecision(
                DecisionsAhead::Ignore,
                vec,
            ))
        } else {
            Ok(IronChainResult::ProgressedDecision(
                DecisionsAhead::Ignore,
                MaybeVec::from_one(decision_information),
            ))
        })
    }

    fn node_chain(
        &mut self,
        current_node: &DecisionNodeHeader,
        decision_index: usize,
    ) -> Result<bool, ChainConstructionError> {
        self.decision_list[decision_index - 1].set_state(ChainedDecisionState::PreCommit);

        let b_2 = self.chain_link(*current_node)?;

        let Some(b_2) = b_2 else { return Ok(false) };

        self.decision_list[decision_index - 2].set_state(ChainedDecisionState::Commit);

        let b_1 = self.chain_link(b_2)?;

        let Some(b_1) = b_1 else { return Ok(false) };

        self.decision_list[decision_index - 3].set_state(ChainedDecisionState::Decide);

        let b = self.chain_link(b_1)?;

        if b.is_none() {
            return Ok(false);
        }

        for index in (0..=decision_index - 4).rev() {
            self.decision_list[index].set_state(ChainedDecisionState::Finalized);
        }

        Ok(true)
    }

    fn chain_link(
        &mut self,
        first_link_header: DecisionNodeHeader,
    ) -> Result<Option<DecisionNodeHeader>, ChainConstructionError> {
        let first_link = self
            .pending_nodes
            .get(&first_link_header.current_block_digest())
            .ok_or_else(|| {
                ChainConstructionError::FailedToGetDecisionNode(
                    first_link_header.current_block_digest(),
                )
            })?;

        let first_link_justify = first_link.justify();

        let b_2 = match (
            first_link_justify,
            first_link.decision_header().previous_block(),
        ) {
            (Some(next_link), Some(previous))
                if *previous == next_link.decision_node().current_block_digest() =>
            {
                self.pending_nodes
                    .get(&next_link.decision_node().current_block_digest())
                    .ok_or_else(|| {
                        ChainConstructionError::FailedToGetDecisionNode(
                            next_link.decision_node().current_block_digest(),
                        )
                    })?
            }
            (None, None) => {
                // This is the root node, no previous block so no justify
                return Ok(None);
            }
            _ => {
                // Missmatching link, what to do :?
                todo!()
            }
        };

        Ok(Some(*b_2.decision_header()))
    }

    fn can_finalize(&self) -> bool {
        self.decision_list
            .front()
            .map(ChainedDecision::state)
            .is_some_and(|state| matches!(state, ChainedDecisionState::Finalized))
    }

    fn pop_decision(&mut self) -> ChainedDecision<RQ> {
        let decision = self
            .decision_list
            .pop_front()
            .expect("Decision list should not be empty");

        self.curr_view = self.curr_view.next_view();

        decision
    }

    fn finalize(&mut self) -> Result<MaybeVec<IronChainDecision<RQ>>, FinalizeDecisionsError>
    where
        RQ: SessionBased,
    {
        let mut finalized_decisions = Vec::new();

        while self.can_finalize() {
            let decision = self.pop_decision();

            self.last_decided_view = Some(decision.view().clone());

            let decision_seq = decision.sequence_number();

            let digest = decision.finalize()?;

            let node = self
                .pending_nodes
                .remove(&digest)
                .ok_or(FinalizeDecisionsError::FailedToGetNode(digest))?;

            let commands = node.into_decision_node().into_commands();

            let client_rqs = commands.iter().map(ClientRqInfo::from).collect();

            let batched_accepted = BatchedDecision::new_with_batch(decision_seq, commands);

            let finalized_decision = IronChainDecision::completed_decision(
                decision_seq,
                ProtocolConsensusDecision::new(decision_seq, batched_accepted, client_rqs, digest),
            );

            finalized_decisions.push(finalized_decision);
        }

        Ok(MaybeVec::from_many(finalized_decisions))
    }
}

#[derive(Debug, Clone, Error)]
pub enum IronChainProcessError {
    #[error("Failed while processing chain {0:?}")]
    ProcessChain(#[from] ChainConstructionError),
    #[error("Failed to finalize decisions {0:?}")]
    FinalizeDecisions(#[from] FinalizeDecisionsError),
    #[error("Failed to get decision")]
    GetDecision(usize),
}

#[derive(Debug, Clone, Error)]
pub enum ChainConstructionError {
    #[error("Failed to construct chain, missing link node {0:?} given by node {1:?}")]
    MissingLinkNode(Digest, DecisionNodeHeader),
    #[error("Failed to get decision node {0:?}")]
    FailedToGetDecisionNode(Digest),
    #[error("Failed to get decision node b_1")]
    FailedToGetB1Node,
    #[error("Failed to get decision node b")]
    FailedToGetBNode,
    #[error("Failed to get justify for b_2")]
    FailedToGetB2Justify,
}

#[derive(Debug, Clone, Error)]
pub enum FinalizeDecisionsError {
    #[error("Failed to finalize decision, {0:?}")]
    FailedToFinalizeDecision(#[from] FinalizeDecisionError),
    #[error("Failed to get decision node {0:?}")]
    FailedToGetNode(Digest),
}
