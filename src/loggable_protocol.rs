use crate::messages::serialize::HotIronOxSer;
use crate::HotIron;
use crate::SerMsg;
use atlas_communication::message::StoredMessage;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::loggable::message::PersistentOrderProtocolTypes;
use atlas_core::ordering_protocol::loggable::{
    DecomposedProof, LoggableOrderProtocol, OrderProtocolLogHelper, PProof,
};
use atlas_core::ordering_protocol::networking::serialize::{
    OrderProtocolVerificationHelper, OrderingProtocolMessage,
};
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{
    DecisionAD, DecisionMetadata, OrderingProtocol, ProtocolConsensusDecision, ProtocolMessage,
    ShareableConsensusMessage,
};
use std::sync::Arc;

impl<RQ, NT, CR> LoggableOrderProtocol<RQ> for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync + 'static,
{
    type PersistableTypes = HotIronOxSer<RQ>;
}

impl<RQ, NT, CR> OrderProtocolLogHelper<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>
    for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync + 'static,
{
    fn message_types() -> Vec<&'static str> {
        todo!()
    }

    fn get_type_for_message(
        msg: &ProtocolMessage<RQ, HotIronOxSer<RQ>>,
    ) -> atlas_common::error::Result<&'static str> {
        todo!()
    }

    fn init_proof_from(
        metadata: DecisionMetadata<RQ, HotIronOxSer<RQ>>,
        additional_data: Vec<DecisionAD<RQ, HotIronOxSer<RQ>>>,
        messages: Vec<StoredMessage<ProtocolMessage<RQ, HotIronOxSer<RQ>>>>,
    ) -> atlas_common::error::Result<PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>> {
        todo!()
    }

    fn init_proof_from_scm(
        metadata: DecisionMetadata<RQ, HotIronOxSer<RQ>>,
        additional_data: Vec<DecisionAD<RQ, HotIronOxSer<RQ>>>,
        messages: Vec<ShareableConsensusMessage<RQ, HotIronOxSer<RQ>>>,
    ) -> atlas_common::error::Result<PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>> {
        todo!()
    }

    fn decompose_proof(
        proof: &PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>,
    ) -> DecomposedProof<RQ, HotIronOxSer<RQ>> {
        todo!()
    }

    fn get_requests_in_proof(
        proof: &PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>,
    ) -> atlas_common::error::Result<ProtocolConsensusDecision<RQ>> {
        todo!()
    }
}

impl<RQ> PersistentOrderProtocolTypes<RQ, Self> for HotIronOxSer<RQ>
where
    RQ: SerMsg,
{
    type Proof = ();

    fn verify_proof<NI, OPVH>(
        network_info: &Arc<NI>,
        proof: Self::Proof,
    ) -> atlas_common::error::Result<Self::Proof>
    where
        NI: NetworkInformationProvider,
        Self: OrderingProtocolMessage<RQ> + Sized,
        OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
    {
        todo!()
    }
}
