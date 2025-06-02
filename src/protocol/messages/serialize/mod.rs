use crate::decision_tree::DecisionNodeHeader;
use crate::protocol::messages::{HotFeOxMsg, VoteType};
use crate::protocol::QC;
use atlas_common::ordering::SeqNo;
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
struct VoteMessage<'a, VT> {
    view_seq: SeqNo,
    vote_type: &'a VT,
}

pub fn serialize_vote_message<VT>(view_seq: SeqNo, vote_type: &VT) -> Buf
where
    VT: SerMsg,
{
    let vote_msg = VoteMessage {
        view_seq,
        vote_type,
    };

    let bytes = bincode::serde::encode_to_vec(&vote_msg, bincode::config::standard())
        .expect("Failed to serialize");

    Buf::from(bytes)
}
