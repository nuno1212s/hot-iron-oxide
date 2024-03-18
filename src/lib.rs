use std::sync::Arc;
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::ordering_protocol::{DecisionMetadata, OPExecResult, OPPollResult, OrderingProtocol, OrderProtocolTolerance, ProtocolMessage, ShareableConsensusMessage};
use atlas_core::timeouts::RqTimeout;
use crate::messages::serialize::HotIronOxSer;
use crate::view::View;

pub mod decisions;
pub mod messages;
pub mod view;

pub struct HotStuff<D, NT> {
    node_id: NodeId,
    current_view: View,
    network_node: Arc<NT>,

}

impl<D, NT> OrderProtocolTolerance for HotStuff<D, NT> {
    fn get_n_for_f(f: usize) -> usize {
        3 * f + 1
    }

    fn get_quorum_for_n(n: usize) -> usize {
        2 * Self::get_f_for_n(n) + 1
    }

    fn get_f_for_n(n: usize) -> usize {
        (n - 1) / 3
    }
}

impl<D, NT> Orderable for HotStuff<D, NT> {
    fn sequence_number(&self) -> SeqNo {
        self.current_view.sequence_number()
    }
}

impl<D, NT> OrderingProtocol<D> for HotStuff<D, NT> {
    type Serialization = HotIronOxSer<D>;
    type Config = ();

    fn handle_off_ctx_message(&mut self, message: ShareableConsensusMessage<D, Self::Serialization>) {
        todo!()
    }

    fn handle_execution_changed(&mut self, is_executing: bool) -> Result<()> {
        todo!()
    }

    fn poll(&mut self) -> Result<OPPollResult<DecisionMetadata<D, Self::Serialization>, ProtocolMessage<D, Self::Serialization>, D>> {
        todo!()
    }

    fn process_message(&mut self, message: ShareableConsensusMessage<D, Self::Serialization>) -> Result<OPExecResult<DecisionMetadata<D, Self::Serialization>, ProtocolMessage<D, Self::Serialization>, D>> {
        todo!()
    }

    fn install_seq_no(&mut self, seq_no: SeqNo) -> Result<()> {
        todo!()
    }

    fn handle_timeout(&mut self, timeout: Vec<RqTimeout>) -> Result<OPExecResult<DecisionMetadata<D, Self::Serialization>, ProtocolMessage<D, Self::Serialization>, D>> {
        todo!()
    }
}