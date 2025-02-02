use crate::decisions::{DecisionNodeHeader, QC};
use crate::messages::serialize::HotIronOxSer;
use crate::messages::HotFeOxMsg;
use crate::view::View;
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::ordering_protocol::networking::{
    NetworkedOrderProtocolInitializer, OrderProtocolSendNode,
};
use atlas_core::ordering_protocol::{Decision, DecisionAD, DecisionMetadata, OPExResult, OPExecResult, OPPollResult, OrderProtocolTolerance, OrderingProtocol, OrderingProtocolArgs, ProtocolMessage, ShareableConsensusMessage};
use atlas_core::timeouts::timeout::{ModTimeout, TimeoutableMod};
use lazy_static::lazy_static;
use std::marker::PhantomData;
use std::sync::Arc;

pub mod crypto;
pub mod decisions;
mod loggable_protocol;
pub mod messages;
pub mod view;

lazy_static! {
    static ref MOD_NAME: Arc<str> = Arc::from("HOT-IRON");
}

pub struct HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync,
{
    node_id: NodeId,
    current_view: View,
    network_node: Arc<NT>,
    quorum_information: Arc<CR>,
    phantom: PhantomData<RQ>,
}

type HotIronResult<RQ> = OPExecResult<
    DecisionMetadata<RQ, HotIronOxSer<RQ>>,
    DecisionAD<RQ, HotIronOxSer<RQ>>,
    ProtocolMessage<RQ, HotIronOxSer<RQ>>,
    RQ,
>;
type HotIronPollResult<RQ> = OPPollResult<
    DecisionMetadata<RQ, HotIronOxSer<RQ>>,
    DecisionAD<RQ, HotIronOxSer<RQ>>,
    ProtocolMessage<RQ, HotIronOxSer<RQ>>,
    RQ,
>;

type HotIronDecision<RQ> = Decision<
    DecisionNodeHeader,
    QC,
    HotFeOxMsg<RQ>,
    RQ
>;

pub(crate) fn get_n_for_f(f: usize) -> usize {
    3 * f + 1
}

pub(crate) fn get_quorum_for_n(n: usize) -> usize {
    2 * get_f_for_n(n) + 1
}

pub(crate) fn get_f_for_n(n: usize) -> usize {
    (n - 1) / 3
}

impl<RQ, NT, CR> OrderProtocolTolerance for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync,
{
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

impl<RQ, NT, CR> Orderable for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync,
{
    fn sequence_number(&self) -> SeqNo {
        self.current_view.sequence_number()
    }
}

impl<RQ, NT, CR> TimeoutableMod<OPExResult<RQ, HotIronOxSer<RQ>>> for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,    
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync,
{
    fn mod_name() -> Arc<str> {
        MOD_NAME.clone()
    }

    fn handle_timeout(
        &mut self,
        timeout: Vec<ModTimeout>,
    ) -> Result<OPExResult<RQ, HotIronOxSer<RQ>>> {
        todo!()
    }
}


impl<RQ, NT, CR> OrderingProtocol<RQ> for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync,
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

impl<NT, RQ, RP, CR> NetworkedOrderProtocolInitializer<RQ, RP, NT> for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync,
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
