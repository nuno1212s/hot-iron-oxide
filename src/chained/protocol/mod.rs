mod decision;
mod log;
mod msg_queue;

use crate::chained::chained_decision_tree::{ChainedDecisionNode, PendingDecisionNodes};
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::IronChainMessage;
use crate::chained::protocol::decision::{
    ChainedDecision, ChainedDecisionPollResult, ChainedDecisionResult, ChainedDecisionState,
    FinalizeDecisionError,
};
use crate::chained::protocol::log::NewViewGenerateError;
use crate::chained::{
    ChainedDecisionHandler, IronChainDecision, IronChainPollResult, IronChainResult,
};
use crate::crypto::{CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionNodeHeader, TQuorumCertificate};
use crate::req_aggr::RequestAggr;
use crate::view::View;
use atlas_common::crypto::hash::Digest;
use atlas_common::maybe_vec::MaybeVec;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::messages::{ClientRqInfo, SessionBased};
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{
    BatchedDecision, Decision, DecisionsAhead, ProtocolConsensusDecision,
    ShareableMessage,
};
use atlas_core::timeouts::timeout::TimeoutModHandle;
use either::Either;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, error, info, warn};
use crate::hot_iron::protocol::hotstuff::Signals;

const PROTOCOL_PHASES: u8 = 4;
const PROTOCOL_PHASES_USIZE: usize = PROTOCOL_PHASES as usize;

pub(crate) struct ChainedHotStuffProtocol<RQ> {
    node_id: NodeId,
    request_aggr: Arc<RequestAggr<RQ>>,
    timeouts: TimeoutModHandle,
    decision_list: VecDeque<ChainedDecision<RQ>>,
    signals: Signals,
    pending_nodes: PendingDecisionNodes<RQ>,
    curr_view: View,
    last_decided_view: Option<View>,
}

impl<RQ> Orderable for ChainedHotStuffProtocol<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.curr_view.sequence_number()
    }
}

impl<RQ> ChainedHotStuffProtocol<RQ>
where
    RQ: SerMsg,
{
    #[must_use]
    pub fn new(
        node_id: NodeId,
        rq_aggr: Arc<RequestAggr<RQ>>,
        timeouts: TimeoutModHandle,
        mut curr_view: View,
    ) -> Self {
        let mut decision_list = VecDeque::with_capacity(PROTOCOL_PHASES as usize);

        let mut signals = Signals::default();

        let first_view = curr_view.clone();

        signals.push_signalled(curr_view.sequence_number());

        for i in 0..PROTOCOL_PHASES {
            let next_view = curr_view.next_view();

            let chained_decision = if i == 0 {
                ChainedDecision::new_first_view(curr_view, node_id)
            } else {
                ChainedDecision::new(curr_view, node_id)
            };

            decision_list.push_back(chained_decision);

            curr_view = next_view;
        }

        Self {
            node_id,
            request_aggr: rq_aggr,
            timeouts,
            decision_list,
            signals,
            pending_nodes: PendingDecisionNodes::default(),
            curr_view: first_view,
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

    pub(crate) fn install_view(&mut self, view: View) -> Result<(), InstallViewError> {
        match view.sequence_number().index(self.sequence_number()) {
            Either::Right(index) => {
                let phases_to_create = if index >= self.decision_list.len() {
                    self.decision_list.clear();

                    PROTOCOL_PHASES_USIZE
                } else {
                    // Remove all decisions that are before the new view
                    for _ in 0..index {
                        self.decision_list.pop_front();
                    }

                    index
                };

                let mut current_view = view.clone();

                for i in 0..phases_to_create {
                    let chained_decision = if i == 0 {
                        ChainedDecision::new_first_view(current_view.clone(), self.node_id)
                    } else {
                        ChainedDecision::new(current_view.clone(), self.node_id)
                    };

                    self.decision_list.push_back(chained_decision);

                    current_view = current_view.next_view();
                }

                self.signals.clear();
                self.signals.push_signalled(view.sequence_number());
            }
            Either::Left(_) => {
                return Err(InstallViewError::ViewInPast(view, self.curr_view.clone()))
            }
        }

        self.curr_view = view;
        Ok(())
    }

    pub(super) fn poll<CR, CP, NT>(
        &mut self,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        decision_handler: &ChainedDecisionHandler,
    ) -> Result<IronChainPollResult<RQ>, FinalizeDecisionsError>
    where
        RQ: SessionBased,
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    {
        while let Some(seq) = self.signals.pop_signalled() {
            let poll_result = {
                let Some(decision) = self.get_decision(seq) else {
                    error!("Decision not found for seq_no: {seq:?}");
                    continue;
                };

                decision.poll::< _, CP, _>( crypto, decision_handler, network)
            };

            match poll_result {
                ChainedDecisionPollResult::ReceiveMsg => {
                    return Ok(IronChainPollResult::ReceiveMsg)
                }
                ChainedDecisionPollResult::Exec(message) => {
                    info!(
                        "Polling revealed message {:?} for decision seq_no {:?}",
                        message.message(),
                        seq
                    );

                    self.signals.push_signalled(seq);

                    return Ok(IronChainPollResult::Exec(message));
                }
                ChainedDecisionPollResult::Decided if self.can_finalize() => {
                    let finalized_decisions = self.finalize()?;

                    if !finalized_decisions.is_empty() {
                        return Ok(IronChainPollResult::ProgressedDecision(
                            DecisionsAhead::Ignore,
                            finalized_decisions,
                        ));
                    }
                }
                ChainedDecisionPollResult::Decided => {
                    warn!(
                        "Decision {:?} is not finalized yet, but is marked as decided",
                        seq
                    );
                }
            }
        }

        Ok(IronChainPollResult::ReceiveMsg)
    }

    pub(super) fn queue(&mut self, message: ShareableMessage<IronChainMessage<RQ>>) {
        let message_seq = message.sequence_number();

        let decision_index = match message_seq.index(self.curr_view.sequence_number()) {
            Either::Right(index) => index,
            Either::Left(_) => {
                debug!("Ignoring consensus message {:?} received from {:?} as we are already in seq no {:?}",
                    message, message.header().from(), self.sequence_number());

                return;
            } // Message is from a past view, ignore it
        };

        if decision_index >= self.decision_list.len() {
            debug!(
                "Queueing message out of context msg {:?} received from {:?} into tbo queue",
                message.message(),
                message.header().from()
            );

            //TODO: Add message to the tbo queue
            todo!("Add message to the tbo queue")
        } else {
            // Queue the message in the corresponding pending decision
            self.decision_list
                .get_mut(decision_index)
                .unwrap()
                .queue_message(message);

            // Signal that we are ready to receive messages
            self.signals.push_signalled(message_seq);
        }
    }

    pub(super) fn process_message<CR, CP, NT>(
        &mut self,
        crypto: &Arc<CR>,
        network: &Arc<NT>,
        decision_handler: &mut ChainedDecisionHandler,
        message: ShareableMessage<IronChainMessage<RQ>>,
    ) -> Result<IronChainResult<RQ>, IronChainProcessError<CP::CombinationError>>
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
            Either::Left(_) => {
                warn!(
                    "Received message {:?} from {:?} in past view {:?}, ignoring",
                    message.message(),
                    message.header().from(),
                    self.curr_view.sequence_number()
                );

                return Ok(IronChainResult::MessageDropped);
            }
        };

        let (decision_seq, result) = {
            let Some(decision) = self.decision_list.get_mut(decision_index) else {
                //TODO: Queue message
                todo!("Queue message for later processing");
            };

            (
                decision.sequence_number(),
                decision.process_message::<CR, CP, NT>(
                    &self.request_aggr,
                    decision_handler,
                    crypto,
                    network,
                    &self.pending_nodes,
                    message,
                )?,
            )
        };

        match result {
            ChainedDecisionResult::ProposalGenerated(proposal_header, message) => {
                Ok(IronChainResult::ProgressedDecision(
                    DecisionsAhead::Ignore,
                    MaybeVec::from_one(Decision::decision_info_from_metadata_and_messages(
                        decision_seq,
                        proposal_header,
                        MaybeVec::None,
                        MaybeVec::from_one(message),
                    )),
                ))
            }
            ChainedDecisionResult::VoteGenerated(node, message) => {
                self.handle_vote_generated(decision_index, node, message, decision_handler)
            }
            ChainedDecisionResult::MessageQueued => {
                self.signals.push_signalled(decision_seq);

                Ok(IronChainResult::MessageQueued)
            }
            ChainedDecisionResult::MessageProcessed(_) => {
                self.signals.push_signalled(decision_seq);

                Ok(IronChainResult::MessageProcessedNoUpdate)
            }
            ChainedDecisionResult::MessageIgnored => Ok(IronChainResult::MessageDropped),
            ChainedDecisionResult::NextLeaderMessages(message, qc) => {
                let next_decision = decision_seq.next();

                self.signals.push_signalled(next_decision);

                let rq_aggr = self.request_aggr.clone();

                if let Some(decision) = self.get_decision(next_decision) {
                    decision.handle_previous_node_completed_leader::<CR, NT>(
                        &rq_aggr, crypto, network, &qc,
                    );
                } else {
                    error!(
                        "Next leader messages for seq_no {:?} but no decision found",
                        next_decision
                    );
                };

                Ok(IronChainResult::MessageProcessedNoUpdate)
            }
        }
    }

    fn handle_vote_generated<CS>(
        &mut self,
        decision_index: usize,
        node: ChainedDecisionNode<RQ>,
        message: ShareableMessage<IronChainMessage<RQ>>,
        decision_handler: &mut ChainedDecisionHandler,
    ) -> Result<IronChainResult<RQ>, IronChainProcessError<CS>>
    where
        RQ: SessionBased,
        CS: Error,
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

        Ok(
            match self.node_chain(&header, decision_index, decision_handler) {
                Ok(_) => {
                    let finalized_decisions = self.finalize()?;

                    info!("Finalized Decisions: {:?}", finalized_decisions);
                    let mut vec = Vec::with_capacity(finalized_decisions.len());

                    vec.push(decision_information);

                    vec.append(&mut finalized_decisions.into_vec());

                    let vec = MaybeVec::from_many(vec);

                    IronChainResult::ProgressedDecision(DecisionsAhead::Ignore, vec)
                }
                Err(node_chain) => {
                    info!("Failed to create node chain for decision index {decision_index}, decision: {:?}. Node chain is {}",
                    decision_information, node_chain
                );

                    IronChainResult::ProgressedDecision(
                        DecisionsAhead::Ignore,
                        MaybeVec::from_one(decision_information),
                    )
                }
            },
        )
    }

    fn node_chain(
        &mut self,
        current_node: &DecisionNodeHeader,
        decision_index: usize,
        decision_handler: &mut ChainedDecisionHandler,
    ) -> Result<(), usize> {
        let b_2 = match self.chain_link(*current_node) {
            Ok(Some(node)) => node,
            Ok(None) => {
                // This is the root node, no previous block so no justify
                info!("Node {current_node:?} is root node, no previous block so no justify");
                return Err(1);
            }
            Err(err) => {
                error!("Failed to chain link for decision index {decision_index}: {err}");
                return Err(1);
            }
        };

        self.decision_list[decision_index - 1]
            .advance_state(ChainedDecisionState::PreCommit, decision_handler);

        let b_1 = match self.chain_link(b_2) {
            Ok(Some(node)) => node,
            Ok(None) => {
                // This is the root node, no previous block so no justify
                info!("Node {b_2:?} is root node, no previous block so no justify");
                return Err(2);
            }
            Err(err) => {
                error!("Failed to chain link for decision index {decision_index}: {err}");
                return Err(2);
            }
        };

        self.decision_list[decision_index - 2]
            .advance_state(ChainedDecisionState::Commit, decision_handler);

        let _ = match self.chain_link(b_1) {
            Ok(Some(node)) => node,
            Ok(None) => {
                // This is the root node, no previous block so no justify
                info!("Node {b_1:?} is root node, no previous block so no justify");
                return Err(3);
            }
            Err(err) => {
                error!("Failed to chain link for decision index {decision_index}: {err}");
                return Err(3);
            }
        };

        self.decision_list[decision_index - 3]
            .advance_state(ChainedDecisionState::Decide, decision_handler);

        // Set all other previous, pending, requests to finalized state as this chain
        // Is now valid
        for index in (0..=decision_index - 3).rev() {
            self.decision_list[index]
                .advance_state(ChainedDecisionState::Finalized, decision_handler);
        }

        Ok(())
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

        self.populate_decision();

        decision
    }

    fn populate_decision(&mut self) {
        let to_create = self
            .decision_list
            .back()
            .map(Orderable::sequence_number)
            .unwrap_or_else(|| self.curr_view.sequence_number())
            .next();

        let view = self.curr_view.with_new_seq(to_create);

        let new_decision = ChainedDecision::new(view, self.node_id);

        self.decision_list.push_back(new_decision);
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
pub enum IronChainProcessError<CS: Error> {
    #[error("Failed while processing chain {0:?}")]
    ProcessChain(#[from] ChainConstructionError),
    #[error("Failed to finalize decisions {0:?}")]
    FinalizeDecisions(#[from] FinalizeDecisionsError),
    #[error("")]
    NewViewError(#[from] NewViewGenerateError<CS>),
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

#[derive(Debug, Clone, Error)]
pub enum InstallViewError {
    #[error("Failed to install view {0:?}, current view is ahead {1:?}")]
    ViewInPast(View, View),
}
