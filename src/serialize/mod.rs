use atlas_common::ordering::SeqNo;
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::Buf;
use serde::Serialize;

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
