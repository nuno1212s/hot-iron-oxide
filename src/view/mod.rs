use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use atlas_core::ordering_protocol::OrderProtocolTolerance;
use crate::HotStuff;

/// A view struct, containing a view of the current quorym
#[derive(Clone, Debug)]
pub struct View {
    seq: SeqNo,
    members: Vec<NodeId>,
    leader: NodeId,
    f: usize,
}

impl Orderable for View {
    fn sequence_number(&self) -> SeqNo {
        self.seq
    }
}

impl NetworkView for View {
    fn primary(&self) -> NodeId {
        self.leader
    }

    fn quorum(&self) -> usize {
        crate::get_quorum_for_n(self.members.len())
    }

    fn quorum_members(&self) -> &Vec<NodeId> {
        &self.members
    }

    fn f(&self) -> usize {
        self.f
    }

    fn n(&self) -> usize {
        self.members.len()
    }
}