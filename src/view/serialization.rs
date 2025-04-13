use crate::protocol::messages::serialize::HotIronOxSer;
use crate::view::View;
use atlas_core::ordering_protocol::networking::serialize::PermissionedOrderingProtocolMessage;

impl<RQ> PermissionedOrderingProtocolMessage for HotIronOxSer<RQ> {
    type ViewInfo = View;
}
