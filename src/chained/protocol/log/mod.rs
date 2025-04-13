use std::error::Error;
use anyhow::Chain;
use thiserror::Error;
use atlas_common::collections::HashMap;
use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use crate::chained::messages::{ChainedQC, IronChainMessageType, VoteMessage};
use crate::crypto::{combine_partial_signatures, CryptoInformationProvider, CryptoProvider, CryptoSignatureCombiner};
use crate::protocol::DecisionNodeHeader;
use crate::view::View;

pub enum DecisionLog {
    /// The decision log for the leader
    Leader(NewViewStore),
    Replica,
}

#[derive(Default)]
pub struct NewViewStore {
    new_view: HashMap<Option<ChainedQC>, HashMap<NodeId, PartialSignature>>,
}

impl DecisionLog {
    
    pub fn as_mut_leader(&mut self) -> &mut NewViewStore {
        if let DecisionLog::Leader(log) = self {
            log
        } else {
            unreachable!("Not a leader log")
        }
    }
    
}


impl NewViewStore {
    pub(in super::super) fn accept_new_view(
        &mut self,
        voter: NodeId,
        vote_message: VoteMessage,
    ) -> bool {
        
        let VoteMessage {
            vote: qc,
            signature: sig,
        } = vote_message;

        let previous = self.new_view.entry(qc).or_default().insert(voter, sig);

        previous.is_none()
    }

    pub(in super::super) fn get_high_qc(&self) -> Option<&ChainedQC> {
        self.new_view
            .keys()
            .max_by_key(|qc| qc.as_ref().map(|qc| qc.sequence_number()))
            .and_then(Option::as_ref)
    }

    pub(in super::super) fn create_new_qc<CR, CP>(
        &self,
        crypto_info: &CR,
        decision_node_header: &DecisionNodeHeader,
    ) -> Result<ChainedQC, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoSignatureCombiner,
    {
        let (qc, votes) = self
            .new_view
            .iter()
            .max_by_key(|(qc, _)| qc.as_ref().map(Orderable::sequence_number))
            .ok_or(NewViewGenerateError::NotEnoughVotes)?;

        let votes = votes
            .iter()
            .map(|(node, sig)| (*node, sig.clone()))
            .collect::<Vec<_>>();

        let combined_signature = combine_partial_signatures::<_, CP>(crypto_info, &votes)
            .map_err(NewViewGenerateError::FailedToCombinePartialSignatures)?;

        if let Some(qc) = qc {
            Ok(ChainedQC::new(
                qc.sequence_number().next(),
                *decision_node_header,
                combined_signature,
            ))
        } else {
            Ok(ChainedQC::new(
                SeqNo::ZERO,
                *decision_node_header,
                combined_signature,
            ))
        }
    }
}

#[derive(Error, Debug)]
pub enum NewViewGenerateError<CS: Error> {
    #[error("Failed to generate high qc")]
    FailedToGenerateHighQC,
    #[error("Failed to combine partial signatures {0:?}")]
    FailedToCombinePartialSignatures(#[from] CS),
    #[error("Failed to collect the highest vote")]
    NotEnoughVotes,
}
