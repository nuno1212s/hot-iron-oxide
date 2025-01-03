pub mod serialize;

use std::fmt::Debug;
#[cfg(feature = "serialize_serde")]
use serde::{Serialize, Deserialize};
use getset::Getters;
use atlas_common::crypto::threshold_crypto::{PartialSignature, CombinedSignature};
use atlas_common::ordering::{Orderable, SeqNo};
use crate::decisions::{DecisionNode, DecisionNodeHeader, QC};


#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum VoteType {
    NewView(Option<QC>),
    PrepareVote(DecisionNodeHeader),
    PreCommitVote(DecisionNodeHeader),
    CommitVote(DecisionNodeHeader),
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Getters)]
pub struct VoteMessage {
    #[get = "pub"]
    vote_type: VoteType,
    #[get = "pub"]
    signature: PartialSignature,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum ProposalType<D> {
    Prepare(DecisionNode<D>, QC),
    PreCommit(QC),
    Commit(QC),
    Decide(QC),
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Getters)]
pub struct ProposalMessage<D> {
    #[get = "pub"]
    proposal_type: ProposalType<D>,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum HotFeOxMsgType<D> {
    Proposal(ProposalMessage<D>),
    Vote(VoteMessage),
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Getters)]
pub struct HotFeOxMsg<D> {
    curr_view: SeqNo,
    #[get = "pub"]
    message: HotFeOxMsgType<D>,
}

impl<D> Orderable for HotFeOxMsg<D> {
    fn sequence_number(&self) -> SeqNo {
        self.curr_view
    }
}

impl<D> HotFeOxMsg<D> {
    pub fn new(view: SeqNo, message: HotFeOxMsgType<D>) -> Self {
        Self {
            curr_view: view,
            message,
        }
    }
}

impl<D> From<HotFeOxMsg<D>> for HotFeOxMsgType<D> {
    fn from(value: HotFeOxMsg<D>) -> Self {
        value.message
    }
}

impl<D> ProposalMessage<D> {
    pub fn new(proposal_type: ProposalType<D>) -> Self {
        Self {
            proposal_type,
        }
    }
}

impl<D> From<ProposalMessage<D>> for ProposalType<D> {
    fn from(value: ProposalMessage<D>) -> Self {
        value.proposal_type
    }
}

impl VoteMessage {
    pub fn new(vote_type: VoteType, sig: PartialSignature) -> Self {
        Self {
            vote_type,
            signature: sig,
        }
    }
    
    pub(crate) fn into_inner(self) -> (VoteType, PartialSignature) {
        (self.vote_type, self.signature)
    }
}

impl From<VoteMessage> for DecisionNodeHeader {
    fn from(value: VoteMessage) -> Self {
        match value.vote_type {
            VoteType::PrepareVote(dn) |
            VoteType::PreCommitVote(dn) |
            VoteType::CommitVote(dn) => {
                dn
            }
            VoteType::NewView(_) => {
                unreachable!()
            }
        }
    }
}

impl<D> Debug for HotFeOxMsg<D> {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> { todo!() }
}
