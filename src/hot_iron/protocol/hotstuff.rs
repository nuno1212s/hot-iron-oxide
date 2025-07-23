use crate::crypto::{CryptoInformationProvider, CryptoProvider};
use crate::decision_tree::{DecisionHandler, DecisionNodeHeader};
use crate::req_aggr::RequestAggr;
use crate::view::View;
use atlas_common::maybe_vec::MaybeVec;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{InvalidSeqNo, Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::messages::SessionBased;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{DecisionsAhead, ShareableMessage};
use atlas_core::request_pre_processing::BatchOutput;
use atlas_core::timeouts::timeout::TimeoutModHandle;
use either::Either;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, VecDeque};
use std::error::Error;
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, instrument, warn};
use crate::hot_iron::{HotIronDecision, HotIronPollResult, HotIronResult};
use crate::hot_iron::protocol::decision::{DecisionError, DecisionFinalizationResult, DecisionPollResult, DecisionResult, HSDecision};
use crate::hot_iron::messages::HotFeOxMsg;
use crate::hot_iron::protocol::QC;
use crate::hot_iron::messages::serialize::HotIronOxSer;


pub(crate) struct HotStuffProtocol<RQ, NT>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
{
    node_id: NodeId,

    /// The decision store
    decisions: VecDeque<HSDecision<RQ>>,

    decision_handler: DecisionHandler<QC>,

    request_aggr: Arc<RequestAggr<RQ>>,

    node: Arc<NT>,

    current_view: View,

    /// Sequence numbers that need to be polled
    signal_queue: Signals,

    timeouts: TimeoutModHandle,
}

impl<RQ, NT> HotStuffProtocol<RQ, NT>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
{
    pub fn new(
        timeouts: TimeoutModHandle,
        node: Arc<NT>,
        view: View,
        batch_output: BatchOutput<RQ>,
    ) -> Self {
        let rq_aggr = RequestAggr::new(batch_output);

        let mut decisions = VecDeque::new();

        decisions.push_back(HSDecision::new(view.clone(), node.id()));

        let mut signals: Signals = Default::default();

        signals.push_signalled(view.sequence_number());

        Self {
            node_id: node.id(),
            decisions,
            decision_handler: DecisionHandler::default(),
            request_aggr: rq_aggr,
            node,
            current_view: view,
            signal_queue: signals,
            timeouts,
        }
    }

    pub fn install_seq_no(&mut self, mut seq_no: SeqNo) {
        self.current_view = self.current_view.with_new_seq(seq_no);

        let last_decision_view = self
            .decisions
            .back()
            .map(HSDecision::view)
            .cloned()
            .unwrap_or(self.current_view.clone());

        let new_view = last_decision_view.with_new_seq(seq_no);

        let decisions_to_pop = self.decisions.len();

        self.decisions.clear();

        for _ in 0..decisions_to_pop {
            seq_no = seq_no.next();

            let new_view = new_view.with_new_seq(seq_no);

            self.decisions
                .push_back(HSDecision::new(new_view, self.node_id));
        }
    }

    pub fn install_view(&mut self, view: View) {
        self.current_view = view;
    }

    pub fn view(&self) -> &View {
        &self.current_view
    }

    fn index(&self, seq_no: SeqNo) -> Either<InvalidSeqNo, usize> {
        seq_no.index(self.current_view.sequence_number())
    }

    pub fn handle_next_view_for_decision(&mut self, seq_no: SeqNo) {
        let index = match self.index(seq_no) {
            Either::Right(i) => i,
            Either::Left(_) => return,
        };

        if index < self.decisions.len() {
            self.decisions.get_mut(index).unwrap().next_view_received();
        } else {
            warn!(
                "Ignoring next view message for seq no {:?} as we are already in seq no {:?}",
                seq_no,
                self.sequence_number()
            );
        }
    }

    pub fn can_finalize(&self) -> bool {
        for item in &self.decisions {
            match item.can_be_finalized() {
                DecisionFinalizationResult::Finalized => return true,
                DecisionFinalizationResult::NextView => (),
                DecisionFinalizationResult::NotFinal => return false,
            }
        }

        false
    }

    #[instrument(skip_all)]
    pub fn queue(&mut self, message: ShareableMessage<HotFeOxMsg<RQ>>) {
        let message_seq = message.message().sequence_number();

        let i = match message_seq.index(self.sequence_number()) {
            Either::Right(i) => i,
            Either::Left(_) => {
                debug!("Ignoring consensus message {:?} received from {:?} as we are already in seq no {:?}",
                    message, message.header().from(), self.sequence_number());

                return;
            }
        };

        if i >= self.decisions.len() {
            debug!(
                "Queueing message out of context msg {:?} received from {:?} into tbo queue",
                message,
                message.header().from()
            );

            //TODO: Add message to the tbo queue
            todo!("Add message to the tbo queue")
        } else {
            // Queue the message in the corresponding pending decision
            self.decisions.get_mut(i).unwrap().queue(message);

            // Signal that we are ready to receive messages
            self.signal_queue.push_signalled(message_seq);
        }
    }

    #[instrument(skip_all)]
    pub fn poll<CR, CP>(&mut self, crypto: &Arc<CR>) -> HotIronPollResult<RQ>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        RQ: SerMsg + SessionBased,
    {
        while let Some(seq) = self.signal_queue.pop_signalled() {
            match seq.index(self.sequence_number()) {
                Either::Right(index) => {
                    let poll_result = self.decisions[index].poll::<_, _, CP>(
                        &self.node,
                        &self.decision_handler,
                        crypto,
                    );

                    match poll_result {
                        DecisionPollResult::NextMessage(message) => {
                            self.signal_queue.push_signalled(seq);

                            return HotIronPollResult::Exec(message);
                        }
                        DecisionPollResult::TryPropose => {
                            self.signal_queue.push_signalled(seq);

                            return HotIronPollResult::ReceiveMsg;
                        }
                        _ => {}
                    }
                }
                Either::Left(_) => {
                    debug!(
                        "Cannot possibly poll sequence number that is in the past {:?} vs {:?}",
                        seq,
                        self.sequence_number()
                    );
                }
            }
        }

        if self.can_finalize() {
            return HotIronPollResult::ProgressedDecision(
                DecisionsAhead::Ignore,
                self.finalize_decisions(),
            );
        }

        HotIronPollResult::ReceiveMsg
    }

    #[instrument(skip_all)]
    pub fn process_message<CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        crypto: &Arc<CR>,
    ) -> Result<HotIronResult<RQ>, ProcessMessageErr<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
        RQ: SerMsg + SessionBased,
    {
        let message_seq = message.message().sequence_number();

        let index = match message_seq.index(self.sequence_number()) {
            Either::Right(i) => i,
            Either::Left(_) => {
                debug!("Ignoring consensus message {:?} received from {:?} as we are already in seq no {:?}",
                    message, message.header().from(), self.sequence_number());

                return Ok(HotIronResult::MessageDropped);
            }
        };

        if index >= self.decisions.len() {
            self.queue(message);

            Ok(HotIronResult::MessageQueued)
        } else {
            let decision = self.decisions.get_mut(index).unwrap();

            let decision_result = decision.process_message::<_, _, CP, _>(
                message,
                &self.node,
                &mut self.decision_handler,
                crypto,
                &self.request_aggr,
            )?;

            Ok(match decision_result {
                DecisionResult::MessageIgnored | DecisionResult::DuplicateVote(_) => {
                    HotIronResult::MessageDropped
                }
                DecisionResult::MessageQueued => HotIronResult::MessageQueued,
                DecisionResult::DecisionProgressed(qc, node_header, message) => {
                    let decisions = Self::turn_into_decision(
                        decision.sequence_number(),
                        qc,
                        node_header,
                        message,
                    );

                    HotIronResult::ProgressedDecision(
                        DecisionsAhead::Ignore,
                        MaybeVec::from_one(decisions),
                    )
                }
                DecisionResult::Decided(qc, message) => {
                    let decision =
                        Self::turn_into_decision(decision.sequence_number(), qc, None, message);

                    let decisions =
                        MaybeVec::from_many(vec![decision]).joining(self.finalize_decisions());

                    HotIronResult::ProgressedDecision(DecisionsAhead::Ignore, decisions)
                }
            })
        }
    }

    fn next_seq_no(&mut self) -> SeqNo {
        let current_seq_no = self.sequence_number();

        self.current_view = self.current_view.with_new_seq(current_seq_no.next());

        self.sequence_number()
    }

    fn pop_front_decision(&mut self) -> HSDecision<RQ> {
        let popped_decision = self
            .decisions
            .pop_front()
            .expect("Cannot have empty decision queue");

        let no = self.next_seq_no();

        let next_view = self.decisions.back().map_or_else(
            || popped_decision.view().with_new_seq(no),
            |d| d.view().with_new_seq(no),
        );

        let decision = HSDecision::new(next_view, self.node_id);

        debug!("Popped decision {:?}, pushing new decision {:?} into the decision queue. Current queue size={}", popped_decision.sequence_number(), decision.sequence_number(), self.decisions.len());

        self.decisions.push_back(decision);

        self.signal_queue.push_signalled(no);

        popped_decision
    }

    pub fn finalize_decisions(&mut self) -> MaybeVec<HotIronDecision<RQ>>
    where
        RQ: SerMsg + SessionBased,
    {
        let mut decisions = Vec::new();

        while self.can_finalize() {
            let decision = self.pop_front_decision();

            let decision = decision.finalize_decision();

            decisions.push(HotIronDecision::completed_decision(
                decision.sequence_number(),
                decision,
            ));
        }

        MaybeVec::from_many(decisions)
    }

    fn turn_into_decision(
        seq: SeqNo,
        qc: Option<QC>,
        metadata: Option<DecisionNodeHeader>,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
    ) -> HotIronDecision<RQ> {
        if let Some(metadata) = metadata {
            HotIronDecision::decision_info_from_metadata_and_messages(
                seq,
                metadata,
                MaybeVec::from(qc),
                MaybeVec::from_one(message),
            )
        } else if let Some(qc) = qc {
            HotIronDecision::partial_decision_info(
                seq,
                MaybeVec::from_one(qc),
                MaybeVec::from_one(message),
            )
        } else {
            HotIronDecision::decision_info_from_message(seq, message)
        }
    }
}

impl<RQ, NT> Orderable for HotStuffProtocol<RQ, NT>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
{
    fn sequence_number(&self) -> SeqNo {
        self.current_view.sequence_number()
    }
}

/// A data structure to keep track of any consensus instances that have been signalled
///
/// A consensus instance being signalled means it should be polled.
#[derive(Debug, Default)]
pub struct Signals {
    // Prevent duplicates efficiently
    signaled_nos: BTreeSet<SeqNo>,
    signaled_seq_no: BinaryHeap<Reverse<SeqNo>>,
}

impl Signals {
    fn new(watermark: u32) -> Self {
        Self {
            signaled_nos: Default::default(),
            signaled_seq_no: BinaryHeap::with_capacity(watermark as usize),
        }
    }

    /// Pop a signalled sequence number
    pub(crate) fn pop_signalled(&mut self) -> Option<SeqNo> {
        self.signaled_seq_no.pop().map(|reversed| {
            let seq_no = reversed.0;

            self.signaled_nos.remove(&seq_no);

            seq_no
        })
    }

    /// Mark a given sequence number as signalled
    pub(crate) fn push_signalled(&mut self, seq: SeqNo) {
        if self.signaled_nos.insert(seq) {
            self.signaled_seq_no.push(Reverse(seq));
        }
    }

    pub(crate) fn clear(&mut self) {
        self.signaled_nos.clear();
        self.signaled_seq_no.clear();
    }
}

#[derive(Error, Debug)]
pub enum ProcessMessageErr<CS: Error> {
    #[error("Decision Error during processing {0}")]
    DecisionError(#[from] DecisionError<CS>),
}
