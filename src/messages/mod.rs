pub mod serialize;

use std::fmt::Debug;
#[cfg(feature = "serialize_serde")]
use serde::{Serialize, Deserialize};
use getset::Getters;
use atlas_common::crypto::threshold_crypto::{PartialSignature, CombinedSignature};
use atlas_common::ordering::{Orderable, SeqNo};
use crate::decisions::{DecisionNode, QC};


#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum VoteType<D> {
    NewView(Option<QC<D>>),
    PrepareVote(DecisionNode<D>),
    PreCommitVote(DecisionNode<D>),
    CommitVote(DecisionNode<D>),
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Getters)]
pub struct VoteMessage<D> {
    #[get = "pub"]
    vote_type: VoteType<D>,
    #[get = "pub"]
    signature: PartialSignature,
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum ProposalType<D> {
    Prepare(DecisionNode<D>, QC<D>),
    PreCommit(QC<D>),
    Commit(QC<D>),
    Decide(QC<D>),
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
    Vote(VoteMessage<D>),
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

impl<D> VoteMessage<D> {
    pub fn new(vote_type: VoteType<D>, sig: PartialSignature) -> Self {
        Self {
            vote_type,
            signature: sig,
        }
    }
    
    pub(crate) fn into_inner(self) -> (VoteType<D>, PartialSignature) {
        (self.vote_type, self.signature)
    }
}

impl<D> From<VoteMessage<D>> for DecisionNode<D> {
    fn from(value: VoteMessage<D>) -> Self {
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
