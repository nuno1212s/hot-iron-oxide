use std::marker::PhantomData;
use std::sync::Arc;
use atlas_communication::message::Header;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::{OrderingProtocolMessage, OrderProtocolVerificationHelper};
use crate::decisions::DecisionNodeHeader;
use crate::messages::HotIronOxMsg;

pub struct HotIronOxSer<D>(PhantomData<(D)>);

impl <D> OrderingProtocolMessage<D> for HotIronOxSer<D> {
    type ProtocolMessage = HotIronOxMsg<D>;
    type ProofMetadata = DecisionNodeHeader;

    fn internally_verify_message<NI, OPVH>(network_info: &Arc<NI>, header: &Header, message: &Self::ProtocolMessage) -> atlas_common::error::Result<()>
        where NI: NetworkInformationProvider,
              OPVH: OrderProtocolVerificationHelper<D, Self, NI>,
              Self: Sized {
        todo!()
    }
}