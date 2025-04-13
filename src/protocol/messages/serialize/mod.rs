use crate::protocol::{DecisionNodeHeader, QC};
use crate::protocol::messages::{HotFeOxMsg, HotFeOxMsgType, VoteType};
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::{Buf, Header};
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::{
    OrderProtocolVerificationHelper, OrderingProtocolMessage,
};
use serde::Serialize;
use std::marker::PhantomData;
use std::sync::Arc;

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

#[derive(Serialize)]
struct VoteMessage<'a> {
    view_seq: SeqNo,
    vote_type: &'a VoteType,
}

pub fn serialize_vote_message(view_seq: SeqNo, vote_type: &VoteType) -> Buf {
    let vote_msg = VoteMessage {
        view_seq,
        vote_type,
    };

    let bytes = bincode::serde::encode_to_vec(&vote_msg, bincode::config::standard())
        .expect("Failed to serialize");

    Buf::from(bytes)
}
