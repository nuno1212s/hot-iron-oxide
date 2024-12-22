use std::cmp::Reverse;
use std::collections::{BinaryHeap, BTreeSet, VecDeque};
use std::sync::Arc;
use either::Either;
use thiserror::Error;
use tracing::{debug, instrument};
use atlas_common::maybe_vec::MaybeVec;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{InvalidSeqNo, Orderable, SeqNo};
use atlas_common::serialization_helper::SerType;
use atlas_core::messages::RequestMessage;
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::ShareableMessage;
use atlas_core::request_pre_processing::{BatchOutput, RequestPreProcessing};
use atlas_core::timeouts::timeout::TimeoutModHandle;
use atlas_core::timeouts::TimeoutsHandle;
use crate::crypto::CryptoInformationProvider;
use crate::decisions::decision::{Decision, DecisionPollResult};
use crate::decisions::{DecisionHandler, DecisionNodeHeader};
use crate::decisions::req_aggr::RequestAggr;
use crate::messages::HotFeOxMsg;
use crate::messages::serialize::HotIronOxSer;

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
    Decided(MaybeVec<atlas_core::ordering_protocol::Decision<DecisionNodeHeader, HotFeOxMsg<RQ>, RQ>>),
}

pub enum ConsensusStatus<RQ> {
    MessageIgnored(),
    //TODO
    Decided(MaybeVec<atlas_core::ordering_protocol::Decision<DecisionNodeHeader, HotFeOxMsg<RQ>, RQ>>),
}

pub(crate) struct HotStuffProtocol<RQ, RP, NT>
where
    RQ: SerType,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>>,
{
    /// The decision store
    decisions: VecDeque<Decision<RQ>>,

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
    pub fn new(timeouts: TimeoutModHandle, node: Arc<NT>, request_pre_processor: RP, batch_output: BatchOutput<RequestMessage<RQ>>) -> Result<Self, ()> {
        
        let rq_aggr = RequestAggr::new(batch_output);
        
        Ok(Self {
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
            debug!("Queueing message out of context msg {:?} received from {:?} into tbo queue",
                 message, message.header().from());

            //TODO: Add message to the tbo queue
        } else {

            // Queue the message in the corresponding pending decision
            self.decisions.get_mut(i).unwrap().queue(message);

            // Signal that we are ready to receive messages
            self.signal_queue.push_signalled(message_seq);
        }
    }

    #[instrument(skip_all)]
    pub fn poll<CR>(&mut self, crypto: &Arc<CR>) -> ConsensusPollStatus<RQ> 
    where CR: CryptoInformationProvider {
        while let Some(seq) = self.signal_queue.pop_signalled() {
            match seq.index(self.current_seq_no) {
                Either::Right(index) => {
                    let poll_result = self.decisions[index].poll(&self.node, &self.decision_handler, crypto);

                    match poll_result {
                        DecisionPollResult::NextMessage(message) => {
                            self.signal_queue.push_signalled(seq);

                            return ConsensusPollStatus::NextMessage(message);
                        }
                        DecisionPollResult::TryPropose => {
                            self.signal_queue.push_signalled(seq);

                            todo!()
                        }
                        _ => {}
                    }
                }
                Either::Left(_) => {
                    debug!("Cannot possibly poll sequence number that is in the past {:?} vs {:?}",
                    seq, self.current_seq_no);
                }
            }
        }

        while self.can_finalize() {
            return ConsensusPollStatus::Decided(MaybeVec::None);
        }

        ConsensusPollStatus::Recv
    }

    #[instrument(skip_all)]
    pub fn process_message(&mut self) -> Result<ConsensusStatus<RQ>, ProcessMessageErr> {
        todo!()
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
    
}