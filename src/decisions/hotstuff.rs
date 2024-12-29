use crate::crypto::{CryptoInformationProvider, CryptoProvider};
use crate::decisions::decision::{DecisionError, DecisionPollResult, HSDecision};
use crate::decisions::req_aggr::RequestAggr;
use crate::decisions::{DecisionHandler, DecisionNodeHeader};
use crate::messages::serialize::HotIronOxSer;
use crate::messages::HotFeOxMsg;
use atlas_common::maybe_vec::MaybeVec;
use atlas_common::ordering::{InvalidSeqNo, Orderable, SeqNo};
use atlas_common::serialization_helper::SerType;
use atlas_core::messages::RequestMessage;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{Decision, ProtocolConsensusDecision, ShareableMessage};
use atlas_core::request_pre_processing::BatchOutput;
use atlas_core::timeouts::timeout::TimeoutModHandle;
use either::Either;
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use tracing::{debug, instrument};
use atlas_common::node_id::NodeId;
use crate::view::leader_allocation::RoundRobinLA;

/// A data structure to keep track of any consensus instances that have been signalled
///
/// A consensus instance being signalled means it should be polled.
#[derive(Debug, Default)]
pub struct Signals {
    // Prevent duplicates efficiently
    signaled_nos: BTreeSet<SeqNo>,
    signaled_seq_no: BinaryHeap<Reverse<SeqNo>>,
}

pub enum ConsensusPollStatus<RQ> {
    Recv,
    NextMessage(ShareableMessage<HotFeOxMsg<RQ>>),
    Decided(
        MaybeVec<atlas_core::ordering_protocol::Decision<DecisionNodeHeader, HotFeOxMsg<RQ>, RQ>>,
    ),
}

pub enum ConsensusStatus<RQ> {
    MessageIgnored,
    MessageQueued,
    //TODO
    Decided(
        MaybeVec<atlas_core::ordering_protocol::Decision<DecisionNodeHeader, HotFeOxMsg<RQ>, RQ>>,
    ),
}

pub(crate) struct HotStuffProtocol<RQ, RP, NT>
where
    RQ: SerType,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
{
    node_id: NodeId,
    
    /// The decision store
    decisions: VecDeque<HSDecision<RQ>>,

    request_pre_processor: RP,

    decision_handler: DecisionHandler<RQ>,

    request_aggr: Arc<RequestAggr<RQ>>,

    node: Arc<NT>,

    current_seq_no: SeqNo,

    /// Sequence numbers that need to be polled
    signal_queue: Signals,

    timeouts: TimeoutModHandle,
}

impl<RQ, RP, NT> HotStuffProtocol<RQ, RP, NT>
where
    RQ: SerType,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
{
    pub fn new(
        timeouts: TimeoutModHandle,
        node: Arc<NT>,
        request_pre_processor: RP,
        batch_output: BatchOutput<RequestMessage<RQ>>,
    ) -> Result<Self, ()> {
        let rq_aggr = RequestAggr::new(batch_output);

        Ok(Self {
            node_id: node.id(),
            decisions: Default::default(),
            request_pre_processor,
            decision_handler: DecisionHandler::default(),
            request_aggr: rq_aggr,
            node,
            current_seq_no: SeqNo::ZERO,
            signal_queue: Default::default(),
            timeouts,
        })
    }

    pub fn install_seq_no(&mut self, seq_no: SeqNo) {
        self.current_seq_no = seq_no;
    }

    fn index(&self, seq_no: &SeqNo) -> Either<InvalidSeqNo, usize> {
        seq_no.index(self.current_seq_no)
    }

    pub fn can_finalize(&self) -> bool {
        self.decisions
            .front()
            .map(|d| d.can_be_finalized())
            .unwrap_or(false)
    }

    #[instrument(skip_all)]
    pub fn queue(&mut self, message: ShareableMessage<HotFeOxMsg<RQ>>) {
        let message_seq = message.message().sequence_number();

        let i = match message_seq.index(self.current_seq_no) {
            Either::Right(i) => i,
            Either::Left(_) => {
                debug!("Ignoring consensus message {:?} received from {:?} as we are already in seq no {:?}",
                    message, message.header().from(), self.current_seq_no);

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
        } else {
            // Queue the message in the corresponding pending decision
            self.decisions.get_mut(i).unwrap().queue(message);

            // Signal that we are ready to receive messages
            self.signal_queue.push_signalled(message_seq);
        }
    }

    #[instrument(skip_all)]
    pub fn poll<CR, CP>(&mut self, crypto: &Arc<CR>) -> ConsensusPollStatus<RQ>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        while let Some(seq) = self.signal_queue.pop_signalled() {
            match seq.index(self.current_seq_no) {
                Either::Right(index) => {
                    let poll_result =
                        self.decisions[index].poll::<_, _, CP>(&self.node, &self.decision_handler, crypto);

                    match poll_result {
                        DecisionPollResult::NextMessage(message) => {
                            self.signal_queue.push_signalled(seq);

                            return ConsensusPollStatus::NextMessage(message);
                        }
                        DecisionPollResult::TryPropose => {
                            self.signal_queue.push_signalled(seq);

                            return ConsensusPollStatus::Recv;
                        }
                        DecisionPollResult::Decided => {}
                        _ => {}
                    }
                }
                Either::Left(_) => {
                    debug!(
                        "Cannot possibly poll sequence number that is in the past {:?} vs {:?}",
                        seq, self.current_seq_no
                    );
                }
            }
        }
        
        let mut to_finalize = Vec::new();
        
        while self.can_finalize() {
            let decision = self.pop_front_decision();

            let protocol_decision = ProtocolConsensusDecision::new(
                decision.sequence_number(),
                
            );
            
            let completed = Decision::completed_decision(decision.sequence_number(), protocol_decision);
            
            to_finalize.push(completed);
        }
        
        if !to_finalize.is_empty() {
            
            
            return ConsensusPollStatus::Decided(MaybeVec::from_many(to_finalize));
        }

        ConsensusPollStatus::Recv
    }

    #[instrument(skip_all)]
    pub fn process_message<CR, CP>(
        &mut self,
        message: ShareableMessage<HotFeOxMsg<RQ>>,
        crypto: &Arc<CR>,
    ) -> Result<ConsensusStatus<RQ>, ProcessMessageErr>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let message_seq = message.message().sequence_number();

        let index = match message_seq.index(self.current_seq_no) {
            Either::Right(i) => i,
            Either::Left(_) => {
                debug!("Ignoring consensus message {:?} received from {:?} as we are already in seq no {:?}",
                    message, message.header().from(), self.current_seq_no);

                return Ok(ConsensusStatus::MessageIgnored);
            }
        };

        if index >= self.decisions.len() {
            self.queue(message);

            Ok(ConsensusStatus::MessageQueued)
        } else {
            let decision = self.decisions.get_mut(index).unwrap();

            let decision_result = decision.process_message::<_, _, CP, _>(
                message,
                &self.node,
                &self.decision_handler,
                &crypto,
                &self.request_aggr,
            )?;

            todo!()
        }
    }
    
    fn next_seq_no(&mut self) -> SeqNo {
        self.current_seq_no = self.current_seq_no.next();
        
        self.current_seq_no
    }
    
    fn pop_front_decision(&mut self) -> HSDecision<RQ> {
        let popped_decision = self.decisions.pop_front().expect("Cannot have empty decision queue");
        
        let no = self.next_seq_no();

        let next_view = popped_decision.view().with_new_seq::<RoundRobinLA>(no);

        let decision = HSDecision::new(next_view, self.node_id);
        
        self.decisions.push_back(decision);
        
        popped_decision
    }
}

impl Signals {
    fn new(watermark: u32) -> Self {
        Self {
            signaled_nos: Default::default(),
            signaled_seq_no: BinaryHeap::with_capacity(watermark as usize),
        }
    }

    /// Pop a signalled sequence number
    fn pop_signalled(&mut self) -> Option<SeqNo> {
        self.signaled_seq_no.pop().map(|reversed| {
            let seq_no = reversed.0;

            self.signaled_nos.remove(&seq_no);

            seq_no
        })
    }

    /// Mark a given sequence number as signalled
    fn push_signalled(&mut self, seq: SeqNo) {
        if self.signaled_nos.insert(seq) {
            self.signaled_seq_no.push(Reverse(seq));
        }
    }

    fn clear(&mut self) {
        self.signaled_nos.clear();
        self.signaled_seq_no.clear();
    }
}

#[derive(Error, Debug)]
pub enum ProcessMessageErr {
    #[error("Decision Error during processing {0}")]
    DecisionError(#[from] DecisionError),
}
