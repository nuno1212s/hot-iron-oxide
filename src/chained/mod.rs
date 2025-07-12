use crate::chained::messages::serialize::IronChainSer;
use crate::chained::protocol::ChainedHotStuffProtocol;
use crate::config::HotIronInitConfig;
use crate::crypto::{AtlasTHCryptoProvider, CryptoInformationProvider};
use crate::decision_tree::{DecisionHandler, DecisionNodeHeader, TQuorumCertificate};
use crate::req_aggr::{ReqAggregator, RequestAggr};
use crate::view::View;
use crate::{get_f_for_n, get_n_for_f, get_quorum_for_n};
use atlas_common::crypto::threshold_crypto::CombinedSignature;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::messages::SessionBased;
use atlas_core::ordering_protocol::networking::{
    NetworkedOrderProtocolInitializer, OrderProtocolSendNode,
};
use atlas_core::ordering_protocol::{
    Decision, DecisionAD, DecisionMetadata, OPExResult, OPExecResult, OPPollResult, OPResult,
    OrderProtocolTolerance, OrderingProtocol, OrderingProtocolArgs, PermissionedOrderingProtocol,
    ProtocolMessage, ShareableConsensusMessage,
};
use atlas_core::timeouts::timeout::{ModTimeout, TimeoutModHandle, TimeoutableMod};
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use tracing::{debug, error, info, trace};

mod chained_decision_tree;
mod loggable_protocol;
pub mod messages;
mod proof;
mod protocol;

type IronChainResult<RQ> = OPExecResult<
    DecisionMetadata<RQ, IronChainSer<RQ>>,
    DecisionAD<RQ, IronChainSer<RQ>>,
    ProtocolMessage<RQ, IronChainSer<RQ>>,
    RQ,
>;

type IronChainDecision<RQ> = Decision<
    DecisionMetadata<RQ, IronChainSer<RQ>>,
    DecisionAD<RQ, IronChainSer<RQ>>,
    ProtocolMessage<RQ, IronChainSer<RQ>>,
    RQ,
>;

type IronChainPollResult<RQ> = OPPollResult<
    DecisionMetadata<RQ, IronChainSer<RQ>>,
    DecisionAD<RQ, IronChainSer<RQ>>,
    ProtocolMessage<RQ, IronChainSer<RQ>>,
    RQ,
>;

type ChainedDecisionHandler = DecisionHandler<ChainedQC>;

static MOD_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("CHAINED-HOT-IRON"));

pub struct IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg,
    CR: Send + Sync,
{
    node_id: NodeId,
    network_node: Arc<NT>,
    quorum_information: Arc<CR>,
    is_executing: bool,
    decision_handler: ChainedDecisionHandler,
    protocol: ChainedHotStuffProtocol<RQ>,
    pre_processor: RP,
}

impl<RQ, NT, CR, RP> IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    CR: Send + Sync,
{
    pub fn new(
        node_id: NodeId,
        network_node: Arc<NT>,
        rq_aggr: Arc<RequestAggr<RQ>>,
        timeouts: TimeoutModHandle,
        quorum_information: Arc<CR>,
        current_view: View,
        reconfig_protocol: RP,
    ) -> Self {
        let protocol = ChainedHotStuffProtocol::new(node_id, rq_aggr, timeouts, current_view);

        Self {
            node_id,
            network_node,
            quorum_information,
            is_executing: false,
            decision_handler: DecisionHandler::default(),
            protocol,
            pre_processor: reconfig_protocol,
        }
    }
}

impl<RQ, NT, CR, RP> OrderProtocolTolerance for IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg,
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

impl<RQ, NT, CR, RP> Orderable for IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg,
    CR: Send + Sync,
{
    fn sequence_number(&self) -> SeqNo {
        self.protocol.sequence_number()
    }
}

impl<RQ, NT, CR, RP> TimeoutableMod<OPExResult<RQ, IronChainSer<RQ>>> for IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg,
    CR: Send + Sync,
{
    fn mod_name() -> Arc<str> {
        MOD_NAME.clone()
    }

    fn handle_timeout(
        &mut self,
        timeout: Vec<ModTimeout>,
    ) -> atlas_common::error::Result<OPExResult<RQ, IronChainSer<RQ>>> {
        todo!()
    }
}

impl<RQ, NT, CR, RP> OrderingProtocol<RQ> for IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg + SessionBased,
    CR: CryptoInformationProvider,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
{
    type Serialization = IronChainSer<RQ>;
    type Config = HotIronInitConfig<CR>;

    fn handle_off_ctx_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) {
        self.protocol.queue(message);
    }

    fn handle_execution_changed(&mut self, is_executing: bool) -> atlas_common::error::Result<()> {
        self.is_executing = is_executing;

        Ok(())
    }

    fn poll(&mut self) -> atlas_common::error::Result<OPResult<RQ, Self::Serialization>> {
        trace!("Polling IronChain protocol...");

        self.protocol
            .poll::<_, AtlasTHCryptoProvider, _>(
                &self.quorum_information,
                &self.network_node,
                &self.decision_handler,
            )
            .map_err(From::from)
    }

    fn process_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) -> atlas_common::error::Result<OPExResult<RQ, Self::Serialization>> {
        let result = self
            .protocol
            .process_message::<_, AtlasTHCryptoProvider, _>(
                &self.quorum_information,
                &self.network_node,
                &mut self.decision_handler,
                message,
            )?;

        match &result {
            IronChainResult::MessageQueued => {
                info!("Message queued in IronChain protocol.");
            }
            IronChainResult::MessageDropped => {
                info!("Message dropped in IronChain protocol.");
            }
            IronChainResult::MessageProcessedNoUpdate => {
                info!("Message processed with no update in IronChain protocol.");
            }
            IronChainResult::ProgressedDecision(_, _) => {
                info!("Decision progressed in IronChain protocol.");
            }
            IronChainResult::QuorumJoined(_, _, _) => {
                info!("Quorum joined in IronChain protocol.");
            }
            IronChainResult::RunCst => {
                info!("Running consensus in IronChain protocol.");
            }
        }

        Ok(result)
    }

    fn install_seq_no(&mut self, seq_no: SeqNo) -> atlas_common::error::Result<()> {
        let view = self.protocol.view().with_new_seq(seq_no);

        self.protocol.install_view(view).map_err(From::from)
    }
}

impl<RQ, RP, NT, CR> NetworkedOrderProtocolInitializer<RQ, RP, NT> for IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg + SessionBased,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
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

        let request_aggr = RequestAggr::new(batch_output);

        let view = View::new_from_quorum(SeqNo::ZERO, quorum);

        Ok(IronChain::new(
            node_id,
            node,
            request_aggr,
            timeout,
            Arc::new(quorum_info),
            view,
            rq,
        ))
    }
}

impl<RQ, NT, CR, RP> PermissionedOrderingProtocol for IronChain<RQ, NT, CR, RP>
where
    RQ: SerMsg + SessionBased,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
    CR: CryptoInformationProvider + Send + Sync,
{
    type PermissionedSerialization = IronChainSer<RQ>;

    fn view(&self) -> atlas_core::ordering_protocol::View<Self::PermissionedSerialization> {
        self.protocol.view().clone()
    }

    fn install_view(
        &mut self,
        view: atlas_core::ordering_protocol::View<Self::PermissionedSerialization>,
    ) {
        if let Err(err) = self.protocol.install_view(view) {
            error!("Failed to install view: {:?}", err);
        }
    }
}

/// In chained hotstuff, there is only one QC type,
/// generic which will be used for all the steps of the protocol
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Debug, Getters)]
pub struct ChainedQC {
    /// The sequence number of the QC
    seq_no: SeqNo,
    decision_node: DecisionNodeHeader,
    #[get = "pub"]
    signature: CombinedSignature,
}

impl ChainedQC {
    pub fn new(
        seq_no: SeqNo,
        decision_node: DecisionNodeHeader,
        signature: CombinedSignature,
    ) -> Self {
        ChainedQC {
            seq_no,
            decision_node,
            signature,
        }
    }
}

impl Orderable for ChainedQC {
    fn sequence_number(&self) -> SeqNo {
        self.seq_no
    }
}

impl TQuorumCertificate for ChainedQC {
    fn decision_node(&self) -> &DecisionNodeHeader {
        &self.decision_node
    }

    fn signature(&self) -> &CombinedSignature {
        &self.signature
    }
}
