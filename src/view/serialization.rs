use atlas_core::ordering_protocol::networking::serialize::PermissionedOrderingProtocolMessage;
use crate::messages::serialize::HotIronOxSer;
use crate::view::View;

impl<RQ> PermissionedOrderingProtocolMessage for HotIronOxSer<RQ> {
    type ViewInfo = View;
}