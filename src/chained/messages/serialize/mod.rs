use crate::chained::messages::{ChainedQC, IronChainMessage, IronChainMessageType};
use crate::protocol::DecisionNodeHeader;
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::Header;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::{
    OrderProtocolVerificationHelper, OrderingProtocolMessage,
};
use std::marker::PhantomData;
use std::sync::Arc;
pub struct IronChainSer<RQ>(PhantomData<fn() -> RQ>);

impl<RQ> OrderingProtocolMessage<RQ> for IronChainSer<RQ>
where
    RQ: SerMsg,
{
    type ProtocolMessage = IronChainMessage<RQ>;
    type DecisionMetadata = DecisionNodeHeader;
    type DecisionAdditionalInfo = ChainedQC;

    fn internally_verify_message<NI, OPVH>(
        _network_info: &Arc<NI>,
        _header: &Header,
        _message: &Self::ProtocolMessage,
    ) -> atlas_common::error::Result<()>
    where
        NI: NetworkInformationProvider,
        OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
        Self: Sized,
    {
        //TODO: Verify the message integrity
        Ok(())
    }
}
