use std::marker::PhantomData;
use crate::crypto::QuorumInfo;
use crate::messages::serialize::HotIronOxSer;
use crate::view::View;
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerType;
use atlas_core::ordering_protocol::networking::NetworkedOrderProtocolInitializer;
use atlas_core::ordering_protocol::{
    DecisionMetadata, OPExResult, OPExecResult, OPPollResult, OrderProtocolTolerance,
    OrderingProtocol, OrderingProtocolArgs, ProtocolMessage, ShareableConsensusMessage,
};
use atlas_core::timeouts::timeout::{ModTimeout, TimeoutableMod};
use lazy_static::lazy_static;
use std::sync::Arc;

mod crypto;
pub mod decisions;
pub mod messages;
mod request_provider;
pub mod view;

lazy_static! {
    static ref MOD_NAME: Arc<str> = Arc::from("HOT-IRON");
}

pub struct HotStuff<RQ, NT, CR> {
    node_id: NodeId,
    current_view: View,
    network_node: Arc<NT>,
    quorum_information: CR,
    phantom: PhantomData<RQ>,
}


pub(crate) fn get_n_for_f(f: usize) -> usize {
    3 * f + 1
}

pub(crate) fn get_quorum_for_n(n: usize) -> usize {
    2 * get_f_for_n(n) + 1
}

pub(crate) fn get_f_for_n(n: usize) -> usize {
    (n - 1) / 3
}

impl<D, NT, CR> OrderProtocolTolerance for HotStuff<D, NT, CR> {
    fn get_n_for_f(f: usize) -> usize {
        get_n_for_f(f)
    }

    fn get_quorum_for_n(n: usize) -> usize {
        get_quorum_for_n(n)
    }

    fn get_f_for_n(n: usize) -> usize {
        get_f_for_n(n)
    }
}

impl<D, NT, CR> Orderable for HotStuff<D, NT, CR> {
    fn sequence_number(&self) -> SeqNo {
        self.current_view.sequence_number()
    }
}

impl<D, NT, CR> TimeoutableMod<OPExResult<D, HotIronOxSer<D>>> for HotStuff<D, NT, CR>
where
    D: SerType,
{
    fn mod_name() -> Arc<str> {
        MOD_NAME.clone()
    }

    fn handle_timeout(
        &mut self,
        timeout: Vec<ModTimeout>,
    ) -> Result<OPExResult<D, HotIronOxSer<D>>> {
        todo!()
    }
}

type HotIronResult<D> =
    OPExecResult<DecisionMetadata<D, HotIronOxSer<D>>, ProtocolMessage<D, HotIronOxSer<D>>, D>;
type HotIronPollResult<D> =
    OPPollResult<DecisionMetadata<D, HotIronOxSer<D>>, ProtocolMessage<D, HotIronOxSer<D>>, D>;

impl<RQ, NT, CR> OrderingProtocol<RQ> for HotStuff<RQ, NT, CR>
where
    RQ: SerType,
{
    type Serialization = HotIronOxSer<RQ>;
    type Config = ();

    fn handle_off_ctx_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) {
        todo!()
    }

    fn handle_execution_changed(&mut self, is_executing: bool) -> Result<()> {
        todo!()
    }

    fn poll(&mut self) -> Result<HotIronPollResult<RQ>> {
        todo!()
    }

    fn process_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) -> Result<HotIronResult<RQ>> {
        todo!()
    }

    fn install_seq_no(&mut self, seq_no: SeqNo) -> Result<()> {
        todo!()
    }
}

impl<NT, RQ, RP, CR> NetworkedOrderProtocolInitializer<RQ, RP, NT> for HotStuff<RQ, NT, CR>
where
    RQ: SerType,
{
    fn initialize(
        config: Self::Config,
        ordering_protocol_args: OrderingProtocolArgs<RQ, RP, NT>,
    ) -> Result<Self>
    where
        Self: Sized,
    {
        todo!()
    }
}
