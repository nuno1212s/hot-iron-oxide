use atlas_common::serialization_helper::SerType;
use std::marker::PhantomData;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_communication::message::{Buf, Header};
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::ordering_protocol::networking::serialize::{OrderingProtocolMessage, OrderProtocolVerificationHelper};
use crate::crypto::get_partial_signature_for_message;
use crate::decisions::DecisionNodeHeader;
use crate::messages::{HotFeOxMsg, HotFeOxMsgType, VoteType};

pub struct HotIronOxSer<RQ>(PhantomData<fn() -> RQ>);

impl <RQ> OrderingProtocolMessage<RQ> for HotIronOxSer<RQ> 
where RQ: SerType {
    type ProtocolMessage = HotFeOxMsg<RQ>;
    type ProofMetadata = DecisionNodeHeader;

    fn internally_verify_message<NI, OPVH>(network_info: &Arc<NI>, header: &Header, message: &Self::ProtocolMessage) -> atlas_common::error::Result<()>
        where NI: NetworkInformationProvider,
              OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
              Self: Sized {
        match message.message() {
            HotFeOxMsgType::Proposal(proposal_message) => {
                Ok(())
            }
            HotFeOxMsgType::Vote(vote_message) => {
                let buf = serialize_vote_message(message.sequence_number(), vote_message.vote_type());
                
                Ok(())
            }
        }
        
    }
}

#[derive(Serialize)]
struct VoteMessage<'a> {
    view_seq: SeqNo,
    vote_type: &'a VoteType
    
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