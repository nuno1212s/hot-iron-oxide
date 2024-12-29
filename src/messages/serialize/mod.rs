use atlas_common::serialization_helper::SerType;
use std::marker::PhantomData;
use std::sync::Arc;
use atlas_common::ordering::SeqNo;
use atlas_communication::message::{Buf, Header};
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::{OrderingProtocolMessage, OrderProtocolVerificationHelper};
use crate::decisions::DecisionNodeHeader;
use crate::messages::{HotFeOxMsg, VoteType};

pub struct HotIronOxSer<RQ>(PhantomData<fn() -> RQ>);

impl <RQ> OrderingProtocolMessage<RQ> for HotIronOxSer<RQ> 
where RQ: SerType {
    type ProtocolMessage = HotFeOxMsg<RQ>;
    type ProofMetadata = DecisionNodeHeader;

    fn internally_verify_message<NI, OPVH>(network_info: &Arc<NI>, header: &Header, message: &Self::ProtocolMessage) -> atlas_common::error::Result<()>
        where NI: NetworkInformationProvider,
              OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
              Self: Sized {
        todo!()
    }
}

pub fn serialize_vote_message<RQ>(view_seq: SeqNo, vote_type: VoteType) -> Buf {
    
    todo!()
    
}