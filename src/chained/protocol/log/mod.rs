use crate::chained::messages::{NewViewMessage, VoteDetails, VoteMessage};
use crate::chained::ChainedQC;
use crate::crypto::{
    combine_partial_signatures, CryptoInformationProvider, CryptoProvider,
};
use crate::view::View;
use atlas_common::collections::HashMap;
use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use getset::Getters;
use std::collections::hash_map::Entry;
use std::error::Error;
use thiserror::Error;

pub enum DecisionLog {
    /// The decision log for the leader
    Leader(NewViewStore),
    NextLeader(VoteStore),
    Replica,
}

impl DecisionLog {
    pub fn as_mut_leader(&mut self) -> &mut NewViewStore {
        if let DecisionLog::Leader(log) = self {
            log
        } else {
            unreachable!("Not a leader log")
        }
    }

    pub fn as_mut_next_leader(&mut self) -> &mut VoteStore {
        if let DecisionLog::NextLeader(log) = self {
            log
        } else {
            unreachable!("Not a next leader log")
        }
    }
}

#[derive(Default)]
pub struct NewViewStore {
    new_view: HashMap<Option<ChainedQC>, HashMap<NodeId, PartialSignature>>,
}

impl NewViewStore {
    pub(in super::super) fn accept_new_view(
        &mut self,
        voter: NodeId,
        vote_message: NewViewMessage,
    ) -> bool {
        let (qc, signature) = vote_message.into_parts();

        let entry = self.new_view.entry(qc).or_default().entry(voter);

        match entry {
            Entry::Occupied(_) => false,
            Entry::Vacant(vacant) => {
                vacant.insert(signature);
                true
            }
        }
    }

    pub(in super::super) fn get_high_qc(&self) -> Result<NewViewQCResult, NewViewQCError> {
        self.new_view
            .iter()
            .max_by_key(|(qc, votes)| qc.as_ref().map(Orderable::sequence_number))
            .and_then(|(qc, votes)| {
                Some(NewViewQCResult {
                    high_qc: qc.clone(),
                    signatures: votes.clone(),
                })
            })
            .ok_or(NewViewQCError::NoVotesReceived)
    }
}

#[derive(Getters)]
pub struct NewViewQCResult {
    high_qc: Option<ChainedQC>,
    #[get = "pub"]
    signatures: HashMap<NodeId, PartialSignature>,
}

impl NewViewQCResult {
    pub fn high_qc(&self) -> Option<&ChainedQC> {
        self.high_qc.as_ref()
    }
}

#[derive(Default)]
pub struct VoteStore {
    votes: HashMap<VoteDetails, HashMap<NodeId, PartialSignature>>,
    proposed: bool,
}

impl VoteStore {
    pub(in super::super) fn accept_vote(
        &mut self,
        voter: NodeId,
        vote_message: VoteMessage,
    ) -> Option<usize> {
        let VoteMessage {
            vote_details,
            signature,
        } = vote_message;

        let votes_for_details = self.votes.entry(vote_details).or_default();

        if let Entry::Vacant(e) = votes_for_details.entry(voter) {
            e.insert(signature);

            Some(votes_for_details.len())
        } else {
            None
        }
    }

    pub(in super::super) fn get_high_justify_qc(&self) -> Option<&VoteDetails> {
        self.votes.keys().max_by_key(|node| {
            node.justify()
                .map_or(SeqNo::ZERO, Orderable::sequence_number)
        })
    }

    pub(in super::super) fn is_proposed(&self) -> bool {
        self.proposed
    }

    pub(in super::super) fn set_proposed(&mut self, proposed: bool) {
        self.proposed = proposed;
    }

    pub(in super::super) fn get_quorum_qc<CR, CP>(
        &mut self,
        view: &View,
        crypto_info: &CR,
    ) -> Result<ChainedQC, ChainedQCGenerateError<CP::CombinationError>>
    where
        CP: CryptoProvider,
        CR: CryptoInformationProvider,
    {
        let (node, votes) = self
            .votes
            .iter()
            .max_by_key(|(_, votes)| votes.len())
            .ok_or(ChainedQCGenerateError::NotEnoughVotes)?;

        if votes.len() < view.quorum() {
            return Err(ChainedQCGenerateError::NotEnoughVotes);
        }

        let votes = votes
            .iter()
            .map(|(node, sig)| (*node, sig.clone()))
            .collect::<Vec<_>>();

        let combined_signature = combine_partial_signatures::<CR, CP>(crypto_info, &votes)
            .map_err(ChainedQCGenerateError::FailedToCombinePartialSignatures)?;

        Ok(ChainedQC::new(
            view.sequence_number(),
            *node.node(),
            combined_signature,
        ))
    }
}

#[derive(Error, Debug, Clone)]
pub enum NewViewGenerateError<CS: Error> {
    #[error("Failed to generate high qc")]
    FailedToGenerateHighQC,
    #[error("Failed to combine partial signatures {0:?}")]
    FailedToCombinePartialSignatures(#[from] CS),
    #[error("Failed to collect the highest vote")]
    NotEnoughVotes,
    #[error("Failed to generate QC from votes {0:?}")]
    FailedToGenerateQC(#[from] ChainedQCGenerateError<CS>),
    #[error("Failed to generate new view message")]
    FailedToGenerateNewViewQC(NewViewQCError),
}

#[derive(Error, Debug, Clone)]
pub enum ChainedQCGenerateError<CS: Error> {
    #[error("Failed to combine partial signatures {0:?}")]
    FailedToCombinePartialSignatures(#[from] CS),
    #[error("Failed to collect the highest vote")]
    NotEnoughVotes,
}

#[derive(Error, Debug, Clone)]
pub enum NewViewQCError {
    #[error("There are no available votes for a new view")]
    NoVotesReceived,
}
