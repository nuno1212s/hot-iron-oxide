use std::marker::PhantomData;
use std::sync::Arc;
use atlas_communication::message::Header;
use atlas_communication::reconfiguration_node::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::OrderingProtocolMessage;
use atlas_core::ordering_protocol::networking::signature_ver::OrderProtocolSignatureVerificationHelper;
use crate::decisions::DecisionNodeHeader;
use crate::messages::HotIronOxMsg;

pub struct HotIronOxSer<D>(PhantomData<(D)>);

impl <D> OrderingProtocolMessage<D> for HotIronOxSer<D> {
    type ProtocolMessage = HotIronOxMsg;
    type ProofMetadata = DecisionNodeHeader;

    fn verify_order_protocol_message<NI, OPVH>(network_info: &Arc<NI>, header: &Header, message: Self::ProtocolMessage) -> atlas_common::error::Result<Self::ProtocolMessage> where NI: NetworkInformationProvider, OPVH: OrderProtocolSignatureVerificationHelper<D, Self, NI>, D: atlas_smr_application::serialize::ApplicationData, Self: Sized {
        todo!()
    }
}