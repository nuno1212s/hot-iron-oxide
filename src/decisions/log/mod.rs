use getset::Getters;
use thiserror::Error;
use atlas_common::collections::HashMap;
use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::ordering::SeqNo;

use crate::crypto::{combine_partial_signatures, CryptoInformationProvider};
use crate::decisions::{DecisionNode, QCType, QC};
use crate::messages::{ProposalMessage, ProposalType, VoteMessage, VoteType};

/// The log of votes for a given decision instance
pub enum DecisionLog<D> {
    Leader(LeaderDecisionLog<D>),
    Replica(ReplicaDecisionLog<D>),
}


pub struct VoteStore<D> {
    vote_type: QCType,
    decision_nodes: HashMap<DecisionNode<D>, Vec<PartialSignature>>,
}

pub struct NewViewStore<D> {
    prepare_qcs: Vec<QC<D>>,
}

pub struct LeaderDecisionLog<D> {
    current_proposal: Option<DecisionNode<D>>,
    high_qc: NewViewStore<D>,
    prepare_qc: VoteStore<D>,
    pre_commit_qc: VoteStore<D>,
    commit_qc: VoteStore<D>,
}

#[derive(Getters)]
pub struct ReplicaDecisionLog<D> {
    #[get = "pub(super)"]
    prepare_qc: Option<QC<D>>,
    #[get = "pub(super)"]
    locked_qc: Option<QC<D>>,
}

impl<D> DecisionLog<D> {
    pub fn as_replica(&self) -> Option<&ReplicaDecisionLog<D>> {
        match self {
            DecisionLog::Replica(replica) => Some(replica),
            _ => None
        }
    }

    pub fn as_leader(&self) -> Option<&LeaderDecisionLog<D>> {
        match self {
            DecisionLog::Leader(leader) => Some(leader),
            _ => None
        }
    }
    
    pub fn as_mut_replica(&mut self) -> Option<&mut ReplicaDecisionLog<D>> {
        match self {
            DecisionLog::Replica(replica) => Some(replica),
            _ => None
        }
    }
    
    pub fn as_mut_leader(&mut self) -> Option<&mut LeaderDecisionLog<D>> {
        match self {
            DecisionLog::Leader(leader) => Some(leader),
            _ => None
        }
    }
}

impl<D> VoteStore<D> {
    pub(super) fn accept_vote(&mut self, voted_node: DecisionNode<D>, vote_signature: PartialSignature) {
        
        self.decision_nodes.entry(voted_node)
            .or_insert_with(Vec::new)
            .push(vote_signature);
    }

    pub(super) fn generate_qc<CR>(&mut self, crypto_info: &CR, view: SeqNo) -> Result<QC<D>, VoteStoreError>
        where CR: CryptoInformationProvider {
        let decision_node = self.decision_nodes.iter()
            .max_by_key(|(_, votes)| votes.len());
        
        if let Some((node, votes)) = decision_node {
            
            match combine_partial_signatures(crypto_info, votes) {
                Ok(signature) => {
                    Ok(QC::new(self.vote_type.clone(), view, node.clone(), signature))
                }
                Err(err) => Err(VoteStoreError::FailedToCreateCombinedSignature)
            }
        } else {
            Err(VoteStoreError::NoDecisionNode)
        }
        
    }
}

impl<D> LeaderDecisionLog<D> {

    pub(in super::super) fn new_view_store(&mut self) -> &mut NewViewStore<D> {
        &mut self.high_qc
    }
    
    pub(in super::super) fn accept_vote(&mut self, vote: VoteMessage<D>) -> Result<(), VoteAcceptError> {
        let (vote, signature) = vote.into_inner();

        match vote {
            VoteType::NewView(_) => {
                Err(VoteAcceptError::NewViewVoteNotAcceptable)
            }
            VoteType::PrepareVote(vote) => {
                Ok(self.prepare_qc.accept_vote(vote, signature))
            }
            VoteType::PreCommitVote(vote) => {
                Ok(self.pre_commit_qc.accept_vote(vote, signature))
            }
            VoteType::CommitVote(commit_vote) => {
                Ok(self.commit_qc.accept_vote(commit_vote, signature))
            }
        }
    }

    pub(in super::super) fn generate_qc<CR>(&mut self, crypto_info: &CR, view: SeqNo, qc_type: QCType) -> Result<QC<D>, VoteStoreError>
        where CR: CryptoInformationProvider {
        match qc_type {
            QCType::PrepareVote => {
                self.prepare_qc.generate_qc(crypto_info, view)
            }
            QCType::PreCommitVote => {
                self.pre_commit_qc.generate_qc(crypto_info, view)
            }
            QCType::CommitVote => {
                self.commit_qc.generate_qc(crypto_info, view)
            }
        }
    }
}

impl<D> ReplicaDecisionLog<D> {
    pub(in super::super) fn accept_proposal(&mut self, proposal: ProposalMessage<D>) -> Result<(), ProposalAcceptError> {
        match proposal.into() {
            ProposalType::Prepare(_, _) => {
                Err(ProposalAcceptError::PrepareProposalNotAcceptable)
            }
            ProposalType::PreCommit(qc) => {
                self.prepare_qc = Some(qc);

                Ok(())
            }
            ProposalType::Commit(locked_qc) => {
                self.locked_qc = Some(locked_qc);

                Ok(())
            }
            ProposalType::Decide(decided_qc) => {
                Ok(())
            }
        }
    }
}

impl<D> NewViewStore<D> {
    
    pub(in super::super) fn accept_new_view(&mut self, new_view_qc: QC<D>) -> Result<(), NewViewAcceptError> {
        
        self.prepare_qcs.push(new_view_qc);
        
        Ok(())
    }
    
    pub(in super::super) fn get_high_qc(&self) -> Option<&QC<D>> {
        self.prepare_qcs.iter()
            .max_by_key(|f| f.view_seq())
    }
}

#[derive(Error, Debug)]
pub enum DecisionError {
    #[error("The received prepare certificate is empty")]
    PrepareCertificateEmpty()
}

#[derive(Error, Debug)]
pub enum VoteStoreError {
    #[error("There is no decision node present")]
    NoDecisionNode,
    #[error("Failed to create combined signature")]
    FailedToCreateCombinedSignature,
}

#[derive(Error, Debug)]
pub enum VoteAcceptError {
    #[error("Cannot accept new view vote in the log.")]
    NewViewVoteNotAcceptable,
}

#[derive(Error, Debug)]
pub enum ProposalAcceptError {
    #[error("Cannot accept prepare proposal")]
    PrepareProposalNotAcceptable
}

#[derive(Error, Debug)]
pub enum NewViewAcceptError {
    
}