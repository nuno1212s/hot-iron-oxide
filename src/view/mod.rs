pub mod leader_allocation;
pub mod serialization;

use crate::view::leader_allocation::{LeaderAllocator, RoundRobinLA};
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};

/// A view struct, containing a view of the current quorym
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct View {
    seq: SeqNo,
    members: Vec<NodeId>,
    leader: NodeId,
    f: usize,
}

impl View {
    pub fn new(seq: SeqNo, members: Vec<NodeId>, leader: NodeId, f: usize) -> Self {
        Self {
            seq,
            members,
            leader,
            f,
        }
    }

    pub fn new_from_quorum(seq_no: SeqNo, members: Vec<NodeId>) -> Self {
        Self::new_from_quorum_with_leader_allocator::<RoundRobinLA>(seq_no, members)
    }

    fn new_from_quorum_with_leader_allocator<L>(seq_no: SeqNo, members: Vec<NodeId>) -> Self
    where
        L: LeaderAllocator,
    {
        let f = crate::get_f_for_n(members.len());
        let leader = L::allocate_leader_from(&members, seq_no);

        Self::new(seq_no, members, leader, f)
    }

    pub fn with_new_seq(&self, seq: SeqNo) -> Self {
        Self::new_from_quorum(seq, self.members.clone())
    }
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
