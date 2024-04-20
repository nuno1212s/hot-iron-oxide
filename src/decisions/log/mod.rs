use thiserror::Error;
use atlas_common::crypto::threshold_crypto::PartialSignature;

use atlas_common::Err;
use atlas_common::error::*;
use atlas_common::ordering::Orderable;
use crate::decisions::DecisionNode;

use crate::messages::QC;
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

impl<D> DecisionLog<D> {
    pub fn handle_new_view_prepare_QC_received(&mut self, view: &View, message: QC<D>) {
        match &mut self.prepare_log {
            PrepareLog::Leader { received_prepare_certificate, .. } => {
                received_prepare_certificate.push(message);
            }
            PrepareLog::Replica { .. } => {}
        }
    }

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