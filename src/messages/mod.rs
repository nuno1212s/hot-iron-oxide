pub mod serialize;

use crate::decisions::{DecisionNode, DecisionNodeHeader, QC};
use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
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
        Self { proposal_type }
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
            VoteType::PrepareVote(dn) | VoteType::PreCommitVote(dn) | VoteType::CommitVote(dn) => {
                dn
            }
            VoteType::NewView(_) => {
                unreachable!()
            }
        }
    }
}

impl<D> Debug for HotFeOxMsg<D> {
    fn fmt(&self, writer: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(writer, "HotStuffMsg {{ {:?} ", self.curr_view)?;

        match &self.message {
            HotFeOxMsgType::Proposal(msg) => {
                write!(writer, "Proposal {{ {:?} }}", msg)
            }
            HotFeOxMsgType::Vote(msg) => {
                write!(writer, "Vote {{ {:?} }}", msg)
            }
        }
    }
}

impl Debug for VoteMessage {
    fn fmt(&self, writer: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(writer, "VoteMessage {{ {:?} }}", self.vote_type)
    }
}

impl<D> Debug for ProposalMessage<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProposalMessage {{ {:?} }}", self.proposal_type)
    }
}

impl<D> Debug for ProposalType<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalType::Prepare(dn, qc) => {
                write!(f, "Prepare {{ {:?}, {:?} }}", dn, qc)
            }
            ProposalType::PreCommit(qc) => {
                write!(f, "PreCommit {{ {:?} }}", qc)
            }
            ProposalType::Commit(qc) => {
                write!(f, "Commit {{ {:?} }}", qc)
            }
            ProposalType::Decide(qc) => {
                write!(f, "Decide {{ {:?} }}", qc)
            }
        }
    }
}
