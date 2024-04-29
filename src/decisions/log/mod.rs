use thiserror::Error;

use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::Err;
use atlas_common::ordering::{Orderable, SeqNo};

use crate::crypto::{combine_partial_signatures, CryptoInformationProvider};
use crate::decisions::{DecisionNode, QCType};
use crate::messages::{QC, VoteMessage, VoteType};
use crate::view::View;

pub enum PrepareLog<D> {
    Leader {
        received_prepare_certificate: Vec<QC<D>>,
        highest_prepare_certificate: Option<QC<D>>,
    },
    Replica,
}

pub enum PreCommitLog<D> {
    Leader {
        decision_node: Option<DecisionNode<D>>,
        received_precommit_votes: Vec<PartialSignature>,
    },
    Replica,
}

pub enum CommitLog<D> {
    Leader {
        decision_node: Option<DecisionNode<D>>,
        received_commit_votes: Vec<PartialSignature>,
    },
    Replica,
}

/// Decision log
pub struct DecisionLog<D> {
    prepare_log: PrepareLog<D>,
    pre_commit_log: PreCommitLog<D>,
    commit_log: CommitLog<D>,
}

pub struct VoteStore<D> {
    vote_type: QCType,
    decision_node: Option<DecisionNode<D>>,
    received_commit_votes: Vec<PartialSignature>,
}

pub struct NewViewStore<D> {
    prepare_qcs: Vec<QC<D>>
}

pub struct LeaderDecisionLog<D> {
    current_proposal: Option<DecisionNode<D>>,
    high_qc: VoteStore<D>,
    prepare_qc: VoteStore<D>,
    pre_commit_qc: VoteStore<D>,
    commit_qc: VoteStore<D>,
}

pub struct ReplicaDecisionLog<D> {
    prepare_qc: Option<QC<D>>,
    locked_qc: Option<QC<D>>,
}

impl<D> VoteStore<D> {
    pub(super) fn accept_vote(&mut self, voted_node: DecisionNode<D>, vote_signature: PartialSignature) {
        if let None = &self.decision_node {
            self.decision_node = Some(voted_node);
        }

        self.received_commit_votes.push(vote_signature);
    }

    pub(super) fn generate_qc<CR>(&mut self, crypto_info: CR, view: SeqNo) -> Result<QC<D>, VoteStoreError>
        where CR: CryptoInformationProvider {
        match combine_partial_signatures(crypto_info, &self.received_commit_votes) {
            Ok(signature) => {
                if let Some(node) = self.decision_node.take() {
                    Ok(QC::new(self.vote_type.clone(), view, node, signature))
                } else {
                    Err(VoteStoreError::NoDecisionNode)
                }
            }
            Err(err) => Err(VoteStoreError::FailedToCreateCombinedSignature)
        }
    }
}

impl<D> LeaderDecisionLog<D> {
    
    pub(super) fn accept_vote(&mut self, vote: VoteMessage<D>) {
        let (vote, signature) = vote.into_inner();

        match vote {
            VoteType::NewView(_) => {}
            VoteType::PrepareVote(_) => {}
            VoteType::PreCommitVote(_) => {}
            VoteType::CommitVote(_) => {}
        }
        
    }
    
}

impl<D> DecisionLog<D> {
    #[allow(non_snake_case)]
    pub fn handle_new_view_prepare_QC_received(&mut self, view: &View, message: QC<D>) {
        match &mut self.prepare_log {
            PrepareLog::Leader { received_prepare_certificate, .. } => {
                received_prepare_certificate.push(message);
            }
            PrepareLog::Replica { .. } => {}
        }
    }

    #[allow(non_snake_case)]
    pub fn populate_highest_prepare_QC(&mut self, view: &View) -> Result<Option<&QC<D>>> {
        match &mut self.prepare_log {
            PrepareLog::Leader { received_prepare_certificate, highest_prepare_certificate } => {
                if received_prepare_certificate.is_empty() {
                    return Err!(DecisionError::PrepareCertificateEmpty);
                } else {
                    received_prepare_certificate.sort_by(|c1, c2| c1.sequence_number().cmp(&c2.sequence_number()));

                    Ok(Some(received_prepare_certificate.last().unwrap()))
                }
            }
            PrepareLog::Replica { .. } => {
                Ok(None)
            }
        }
    }

    pub fn queue_prepare_vote(&mut self, view: &View, node: &DecisionNode<D>, prepare_certificate: PartialSignature) {
        match &mut self.pre_commit_log {
            PreCommitLog::Leader { received_precommit_votes, decision_node } => {
                if let Some(current_node) = decision_node {
                    if current_node != *node {
                        return;
                    }
                } else {
                    *decision_node = Some(node.clone());
                }

                received_precommit_votes.push(prepare_certificate);
            }
            PreCommitLog::Replica => {}
        }
    }

    pub fn make_prepare_certificate(&mut self, view: &View) -> Result<QC<D>> {
        match &self.pre_commit_log {
            PreCommitLog::Leader {
                decision_node,
                received_precommit_votes
            } => {}
            PreCommitLog::Replica => {}
        }

        todo!()
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