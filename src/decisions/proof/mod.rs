use crate::decisions::{DecisionNode, DecisionNodeHeader, QCType, QC};
use crate::messages::{HotFeOxMsg, HotFeOxMsgType, ProposalType};
use atlas_common::collections::HashMap;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_communication::message::StoredMessage;
use atlas_core::ordering_protocol::networking::serialize::OrderProtocolProof;
use getset::Getters;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::info;

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Getters)]
pub struct Proof<RQ> {
    #[get = "pub"]
    decision_node: DecisionNode<RQ>,
    #[get = "pub"]
    qcs: HashMap<QCType, QC>,
    #[get = "pub"]
    messages: Vec<StoredMessage<HotFeOxMsg<RQ>>>,
}

impl<D> Proof<D>
where
    D: Clone,
{
    
    /// # [Errors]
    /// Returns an error if the prepare proposal is not found within
    /// the stored messages.
    pub fn new_from_storage(
        decision_header: DecisionNodeHeader,
        qcs: Vec<QC>,
        messages: Vec<StoredMessage<HotFeOxMsg<D>>>,
    ) -> Result<Self, ProofFromStorageError> {
        let qcs = qcs
            .into_iter()
            .map(|qc| (qc.qc_type(), qc))
            .collect::<HashMap<_, _>>();

        let decision_node = messages
            .iter()
            .filter(|msg| matches!(msg.message().message(), HotFeOxMsgType::Proposal(_)))
            .filter(|msg| {
                let HotFeOxMsgType::Proposal(proposal_message) = msg.message().message() else {
                    unreachable!()
                };

                matches!(
                    proposal_message.proposal_type(),
                    ProposalType::Prepare(_, _)
                )
            })
            .map(|msg| {
                let HotFeOxMsgType::Proposal(proposal_message) = msg.message().message() else {
                    unreachable!()
                };

                let ProposalType::Prepare(node, _) = proposal_message.proposal_type() else {
                    unreachable!()
                };

                node.clone()
            })
            .next()
            .ok_or(ProofFromStorageError::FailedToObtainPrepareProposal)?;

        if *decision_node.decision_header() != decision_header {
            return Err(ProofFromStorageError::MissMatchingDecisionHeader);
        }
        
        Ok(Self {
            decision_node,
            qcs,
            messages,
        })
    }

    #[must_use] pub fn new(
        decision_node: DecisionNode<D>,
        qcs: HashMap<QCType, QC>,
        messages: Vec<StoredMessage<HotFeOxMsg<D>>>,
    ) -> Self {
        Self {
            decision_node,
            qcs,
            messages,
        }
    }
}

impl<RQ> Orderable for Proof<RQ> {
    fn sequence_number(&self) -> SeqNo {
        self.decision_node.sequence_number()
    }
}

impl<RQ> OrderProtocolProof for Proof<RQ> {
    fn contained_messages(&self) -> usize {
        self.messages.len()
    }
}

impl<RQ> Clone for Proof<RQ>
where
    RQ: Clone,
{
    fn clone(&self) -> Self {
        Self {
            decision_node: self.decision_node.clone(),
            qcs: self.qcs.clone(),
            messages: self.messages.clone(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ProofFromStorageError {
    #[error("Failed to obtain prepare proposal")]
    FailedToObtainPrepareProposal,
    #[error("Failed to match the decision header of the proof with the prepare proposal")]
    MissMatchingDecisionHeader,
}
