use crate::decision_tree::DecisionNodeHeader;
use crate::hot_iron::protocol::QC;
use crate::view::View;
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::Header;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::{OrderProtocolVerificationHelper, OrderingProtocolMessage, PermissionedOrderingProtocolMessage};
use std::marker::PhantomData;
use std::sync::Arc;
use crate::hot_iron::messages::HotFeOxMsg;

pub struct HotIronOxSer<RQ>(PhantomData<fn() -> RQ>);

impl<RQ> OrderingProtocolMessage<RQ> for HotIronOxSer<RQ>
where
    RQ: SerMsg,
{
    type ProtocolMessage = HotFeOxMsg<RQ>;
    type DecisionMetadata = DecisionNodeHeader;
    type DecisionAdditionalInfo = QC;

    fn internally_verify_message<NI, OPVH>(
        _network_info: &Arc<NI>,
        _header: &Header,
        message: &Self::ProtocolMessage,
    ) -> atlas_common::error::Result<()>
    where
        NI: NetworkInformationProvider,
        OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
        Self: Sized,
    {
        //TODO: Verify message correctness
        Ok(())
    }
}

impl<RQ> PermissionedOrderingProtocolMessage for HotIronOxSer<RQ> {
    type ViewInfo = View;
}