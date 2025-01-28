use crate::messages::serialize::HotIronOxSer;
use crate::HotIron;
use atlas_communication::message::StoredMessage;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::loggable::message::PersistentOrderProtocolTypes;
use atlas_core::ordering_protocol::loggable::{DecomposedProof, LoggableOrderProtocol, PProof};
use atlas_core::ordering_protocol::networking::serialize::{
    OrderProtocolVerificationHelper, OrderingProtocolMessage,
};
use atlas_core::ordering_protocol::{
    DecisionAD, DecisionMetadata, OrderingProtocol, ProtocolConsensusDecision, ProtocolMessage,
    ShareableConsensusMessage,
};
use std::sync::Arc;
use crate::SerMsg;

impl<RQ, NT, CR> LoggableOrderProtocol<RQ> for HotIron<RQ, NT, CR>
where
    RQ: SerMsg,
{
    type PersistableTypes = HotIronOxSer<RQ>;

    fn message_types() -> Vec<&'static str> {
        todo!()
    }

    fn get_type_for_message(
        msg: &ProtocolMessage<RQ, Self::Serialization>,
    ) -> atlas_common::error::Result<&'static str> {
        todo!()
    }

    fn init_proof_from(
        metadata: DecisionMetadata<RQ, Self::Serialization>,
        additional_data: Vec<DecisionAD<RQ, Self::Serialization>>,
        messages: Vec<StoredMessage<ProtocolMessage<RQ, Self::Serialization>>>,
    ) -> atlas_common::error::Result<PProof<RQ, Self::Serialization, Self::PersistableTypes>> {
        todo!()
    }

    fn init_proof_from_scm(
        metadata: DecisionMetadata<RQ, Self::Serialization>,
        additional_data: Vec<DecisionAD<RQ, Self::Serialization>>,
        messages: Vec<ShareableConsensusMessage<RQ, Self::Serialization>>,
    ) -> atlas_common::error::Result<PProof<RQ, Self::Serialization, Self::PersistableTypes>> {
        todo!()
    }

    fn decompose_proof(
        proof: &PProof<RQ, Self::Serialization, Self::PersistableTypes>,
    ) -> DecomposedProof<RQ, Self::Serialization> {
        todo!()
    }

    fn get_requests_in_proof(
        proof: &PProof<RQ, Self::Serialization, Self::PersistableTypes>,
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
        Self: OrderingProtocolMessage<RQ>,
        OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
        Self: Sized,
    {
        todo!()
    }
}
