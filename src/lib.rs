use crate::config::HotIronInitConfig;
use crate::crypto::{AtlasTHCryptoProvider, CryptoInformationProvider};
use crate::decisions::hotstuff::HotStuffProtocol;
use crate::decisions::{DecisionNodeHeader, QC};
use crate::messages::serialize::HotIronOxSer;
use crate::messages::HotFeOxMsg;
use crate::view::View;
use atlas_common::error::*;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::messages::SessionBased;
use atlas_core::ordering_protocol::networking::{
    NetworkedOrderProtocolInitializer, OrderProtocolSendNode,
};
use atlas_core::ordering_protocol::{Decision, DecisionAD, DecisionMetadata, OPExResult, OPExecResult, OPPollResult, OrderProtocolTolerance, OrderingProtocol, OrderingProtocolArgs, PermissionedOrderingProtocol, ProtocolMessage, ShareableConsensusMessage};
use atlas_core::timeouts::timeout::{ModTimeout, TimeoutableMod};
use lazy_static::lazy_static;
use std::sync::Arc;
use tracing::info;

pub mod config;
pub mod crypto;
pub mod decisions;
mod loggable_protocol;
pub mod messages;
pub mod view;
pub mod metric;

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
    hot_stuff_protocol: HotStuffProtocol<RQ, NT>,
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

type HotIronDecision<RQ> = Decision<DecisionNodeHeader, QC, HotFeOxMsg<RQ>, RQ>;

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
    RQ: SerMsg + SessionBased,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: CryptoInformationProvider + Send + Sync,
{
    type Serialization = HotIronOxSer<RQ>;
    type Config = HotIronInitConfig<CR>;

    fn handle_off_ctx_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) {
        self.hot_stuff_protocol.queue(message);
    }

    fn handle_execution_changed(&mut self, is_executing: bool) -> Result<()> {
        // We don't really need anything here since our proposer design is different,
        // We only propose when we have the necessary new view messages
        Ok(())
    }

    fn poll(&mut self) -> Result<HotIronPollResult<RQ>> {
        info!("Polling hot iron");
        Ok(self
            .hot_stuff_protocol
            .poll::<CR, AtlasTHCryptoProvider>(&self.quorum_information))
    }

    fn process_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) -> Result<HotIronResult<RQ>> {
        self.hot_stuff_protocol
            .process_message::<CR, AtlasTHCryptoProvider>(message, &self.quorum_information)
            .map_err(|e| e.into())
    }

    fn install_seq_no(&mut self, seq_no: SeqNo) -> Result<()> {
        Ok(self.hot_stuff_protocol.install_seq_no(seq_no))
    }
}

impl<NT, RQ, RP, CR> NetworkedOrderProtocolInitializer<RQ, RP, NT> for HotIron<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: CryptoInformationProvider + Send + Sync,
{
    fn initialize(
        config: Self::Config,
        ordering_protocol_args: OrderingProtocolArgs<RQ, RP, NT>,
    ) -> Result<Self>
    where
        Self: Sized,
    {
        let OrderingProtocolArgs(node_id, timeout, rq, batch_output, node, quorum) =
            ordering_protocol_args;

        let HotIronInitConfig { quorum_info } = config;

        let hot_stuff_protocol = HotStuffProtocol::new(timeout, node.clone(), batch_output)
            .map_err(|_e| anyhow::anyhow!("Error while initializing hot stuff protocol"))?;

        let view = View::new_from_quorum(SeqNo::ZERO, quorum);

        Ok(HotIron {
            node_id,
            current_view: view,
            network_node: node,
            quorum_information: Arc::new(quorum_info),
            hot_stuff_protocol,
        })
    }
}

impl<RQ, NT, CR> PermissionedOrderingProtocol for HotIron<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: CryptoInformationProvider + Send + Sync,
{
    type PermissionedSerialization = HotIronOxSer<RQ>;

    fn view(&self) -> atlas_core::ordering_protocol::View<Self::PermissionedSerialization> {
        self.current_view.clone()
    }

    fn install_view(&mut self, view: atlas_core::ordering_protocol::View<Self::PermissionedSerialization>) {
        self.current_view = view;
    }
}