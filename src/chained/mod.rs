use crate::chained::messages::serialize::IronChainSer;
use crate::chained::protocol::ChainedHotStuffProtocol;
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
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{
    Decision, DecisionAD, DecisionMetadata, OPExResult, OPExecResult, OPPollResult, OPResult,
    OrderProtocolTolerance, OrderingProtocol, ProtocolMessage, ShareableConsensusMessage,
};
use atlas_core::timeouts::timeout::{ModTimeout, TimeoutModHandle, TimeoutableMod};
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};

mod chained_decision_tree;
pub mod messages;
mod protocol;
mod loggable_protocol;

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

pub struct IronChain<RQ, NT, CR>
where
    RQ: SerMsg,
    CR: Send + Sync,
{
    node_id: NodeId,
    network_node: Arc<NT>,
    quorum_information: Arc<CR>,
    protocol: ChainedHotStuffProtocol<RQ, RequestAggr<RQ>>,
}

impl<RQ, NT, CR> IronChain<RQ, NT, CR>
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
    ) -> Self {
        let protocol = ChainedHotStuffProtocol::new(node_id, rq_aggr, timeouts, current_view);

        Self {
            node_id,
            network_node,
            quorum_information,
            protocol,
        }
    }
}

impl<RQ, NT, CR> OrderProtocolTolerance for IronChain<RQ, NT, CR>
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

impl<RQ, NT, CR> Orderable for IronChain<RQ, NT, CR>
where
    RQ: SerMsg,
    CR: Send + Sync,
{
    fn sequence_number(&self) -> SeqNo {
        self.protocol.sequence_number()
    }
}

impl<RQ, NT, CR> TimeoutableMod<OPExResult<RQ, IronChainSer<RQ>>> for IronChain<RQ, NT, CR>
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

impl<RQ, NT, CR> OrderingProtocol<RQ> for IronChain<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    CR: CryptoInformationProvider,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
{
    type Serialization = IronChainSer<RQ>;
    type Config = ();

    fn handle_off_ctx_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) {
        todo!()
    }

    fn handle_execution_changed(&mut self, is_executing: bool) -> atlas_common::error::Result<()> {
        todo!()
    }

    fn poll(&mut self) -> atlas_common::error::Result<OPResult<RQ, Self::Serialization>> {
        Ok(self.protocol.poll())
    }

    fn process_message(
        &mut self,
        message: ShareableConsensusMessage<RQ, Self::Serialization>,
    ) -> atlas_common::error::Result<OPExResult<RQ, Self::Serialization>> {
        let result = self
            .protocol
            .process_message::<CR, AtlasTHCryptoProvider, NT>(
                &self.quorum_information,
                &self.network_node,
                message,
            )?;

        Ok(result)
    }

    fn install_seq_no(&mut self, seq_no: SeqNo) -> atlas_common::error::Result<()> {
        todo!()
    }
}

/// In chained hotstuff, there is only one QC type,
/// generic which will be used for all the steps of the protocol
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
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
