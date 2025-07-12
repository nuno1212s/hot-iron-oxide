use atlas_common::node_id::NodeId;
use atlas_common::ordering::SeqNo;

pub trait LeaderAllocator {
    fn allocate_leader_from(leader_pool: &[NodeId], seq_no: SeqNo) -> NodeId;
}

pub struct RoundRobinLA;

impl LeaderAllocator for RoundRobinLA {
    fn allocate_leader_from(leader_pool: &[NodeId], seq_no: SeqNo) -> NodeId {
        let index = (seq_no.into_u32() as usize) % leader_pool.len();

        leader_pool[index]
    }
}
