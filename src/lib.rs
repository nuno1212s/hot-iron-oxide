use std::sync::Arc;
use lazy_static::lazy_static;
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::ordering_protocol::{DecisionMetadata, OPExecResult, OPExResult, OPPollResult, OrderingProtocol, OrderingProtocolArgs, OrderProtocolTolerance, ProtocolMessage, ShareableConsensusMessage};
use atlas_core::ordering_protocol::networking::NetworkedOrderProtocolInitializer;
use atlas_core::timeouts::timeout::{ModTimeout, TimeoutableMod};
use crate::crypto::QuorumInfo;
use crate::messages::serialize::HotIronOxSer;
use crate::view::View;

pub mod decisions;
pub mod messages;
pub mod view;
mod crypto;
mod request_provider;


lazy_static!(
    static ref MOD_NAME: Arc<str> = Arc::from("HOT-IRON");
);

pub struct HotStuff<D, NT, CR> {
    node_id: NodeId,
    current_view: View,
    network_node: Arc<NT>,
    quorum_information: CR,
    
}

impl<D, NT, CR> OrderProtocolTolerance for HotStuff<D, NT, CR> {
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

impl<D, NT, CR> Orderable for HotStuff<D, NT, CR> {
    fn sequence_number(&self) -> SeqNo {
        self.current_view.sequence_number()
    }
}

impl<D, NT, CR> TimeoutableMod<OPExResult<D, HotIronOxSer<D>>> for HotStuff<D, NT, CR> {
    fn mod_name() -> Arc<str> {
        MOD_NAME.clone()
    }

    fn handle_timeout(&mut self, timeout: Vec<ModTimeout>) -> Result<OPExResult<D, HotIronOxSer<D>>> {
        todo!()
    }
}

type HotIronResult<D> = OPExecResult<DecisionMetadata<D, HotIronOxSer<D>>, ProtocolMessage<D, HotIronOxSer<D>>, D>;
type HotIronPollResult<D> = OPPollResult<DecisionMetadata<D, HotIronOxSer<D>>, ProtocolMessage<D, HotIronOxSer<D>>, D>;

impl<D, NT, CR> OrderingProtocol<D> for HotStuff<D, NT, CR> {
    type Serialization = HotIronOxSer<D>;
    type Config = ();

    fn handle_off_ctx_message(&mut self, message: ShareableConsensusMessage<D, Self::Serialization>) {
        todo!()
    }

    fn handle_execution_changed(&mut self, is_executing: bool) -> Result<()> {
        todo!()
    }

    fn poll(&mut self) -> Result<HotIronPollResult<D>> {
        todo!()
    }

    fn process_message(&mut self, message: ShareableConsensusMessage<D, Self::Serialization>)
                       -> Result<HotIronResult<D>> {
        todo!()
    }

    fn install_seq_no(&mut self, seq_no: SeqNo) -> Result<()> {
        todo!()
    }
}

impl<D, NT, RQ, CR> NetworkedOrderProtocolInitializer<D, RQ, NT> for HotStuff<D, NT, CR> {
    fn initialize(config: Self::Config, ordering_protocol_args: OrderingProtocolArgs<D, RQ, NT>) -> Result<Self> where Self: Sized {
        todo!()
    }
}