use crate::chained::messages::IronChainMessage;
use crate::chained::ChainedQC;
use crate::decision_tree::DecisionNodeHeader;
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::Header;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::{OrderProtocolVerificationHelper, OrderingProtocolMessage, PermissionedOrderingProtocolMessage};
use std::marker::PhantomData;
use std::sync::Arc;
use crate::view::View;

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

impl<RQ> PermissionedOrderingProtocolMessage for IronChainSer<RQ> {
    type ViewInfo = View;
}

