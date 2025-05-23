use crate::protocol::messages::serialize::HotIronOxSer;
use crate::view::View;
use atlas_core::ordering_protocol::networking::serialize::PermissionedOrderingProtocolMessage;
use crate::chained::messages::serialize::IronChainSer;

impl<RQ> PermissionedOrderingProtocolMessage for HotIronOxSer<RQ> {
    type ViewInfo = View;
}


impl<RQ> PermissionedOrderingProtocolMessage for IronChainSer<RQ> {
    type ViewInfo = View;
}