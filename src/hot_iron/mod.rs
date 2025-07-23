use std::sync::Arc;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::messages::SessionBased;
use atlas_core::ordering_protocol::{Decision, DecisionAD, DecisionMetadata, OPExResult, OPExecResult, OPPollResult, OrderProtocolTolerance, OrderingProtocol, OrderingProtocolArgs, PermissionedOrderingProtocol, ProtocolMessage, ShareableConsensusMessage};
use atlas_core::ordering_protocol::networking::{NetworkedOrderProtocolInitializer, OrderProtocolSendNode};
use atlas_core::timeouts::timeout::{ModTimeout, TimeoutableMod};
use atlas_core::timeouts::TimeOutable;
use tracing::trace;
use crate::config::HotIronInitConfig;
use crate::crypto::{AtlasTHCryptoProvider, CryptoInformationProvider};
use crate::decision_tree::DecisionNodeHeader;
use crate::{get_f_for_n, get_n_for_f, get_quorum_for_n, MOD_NAME};
use crate::hot_iron::protocol::hotstuff::HotStuffProtocol;
use messages::HotFeOxMsg;
use crate::hot_iron::protocol::QC;
use crate::view::View;

pub use messages::serialize::HotIronOxSer;

pub mod protocol;
mod loggable_protocol;
pub(super) mod metric;
mod messages;


pub struct HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync,
{
    node_id: NodeId,
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
        self.hot_stuff_protocol.sequence_number()
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
    ) -> atlas_common::error::Result<OPExResult<RQ, HotIronOxSer<RQ>>> {
        timeout
            .iter()
            .map(ModTimeout::extra_info)
            .map(|info| info.map(TimeOutable::as_any))
            .for_each(|any| {
                if let Some(seq_no) = any.as_ref().and_then(|any| any.downcast_ref::<SeqNo>()) {
                    self.hot_stuff_protocol
                        .handle_next_view_for_decision(*seq_no);
                }
            });

        Ok(OPExecResult::MessageProcessedNoUpdate)
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

    fn handle_execution_changed(&mut self, is_executing: bool) -> atlas_common::error::Result<()> {
        // We don't really need anything here since our proposer design is different,
        // We only propose when we have the necessary new view messages
        Ok(())
    }

    fn poll(&mut self) -> atlas_common::error::Result<HotIronPollResult<RQ>> {
        trace!("Polling hot iron");
        Ok(self
            .hot_stuff_protocol
            .poll::<CR, AtlasTHCryptoProvider>(&self.quorum_information))
    }

    fn process_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) -> atlas_common::error::Result<HotIronResult<RQ>> {
        self.hot_stuff_protocol
            .process_message::<CR, AtlasTHCryptoProvider>(message, &self.quorum_information)
            .map_err(Into::into)
    }

    fn install_seq_no(&mut self, seq_no: SeqNo) -> atlas_common::error::Result<()> {
        self.hot_stuff_protocol.install_seq_no(seq_no);
        Ok(())
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
    ) -> atlas_common::error::Result<Self>
    where
        Self: Sized,
    {
        let OrderingProtocolArgs(node_id, timeout, rq, batch_output, node, quorum) =
            ordering_protocol_args;

        let HotIronInitConfig { quorum_info } = config;

        let view = View::new_from_quorum(SeqNo::ZERO, quorum);

        let hot_stuff_protocol = HotStuffProtocol::new(timeout, node.clone(), view, batch_output);

        Ok(HotIron {
            node_id,
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
        self.hot_stuff_protocol.view().clone()
    }

    fn install_view(
        &mut self,
        view: atlas_core::ordering_protocol::View<Self::PermissionedSerialization>,
    ) {
        self.hot_stuff_protocol.install_view(view);
    }
}
