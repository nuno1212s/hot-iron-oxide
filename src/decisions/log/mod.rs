use std::error::Error;
use crate::crypto::{combine_partial_signatures, CryptoInformationProvider, CryptoProvider, CryptoSignatureCombiner};
use crate::decisions::{DecisionNode, DecisionNodeHeader, QCType, QC};
use crate::messages::{ProposalMessage, ProposalType, VoteMessage, VoteType};
use crate::view::View;
use atlas_common::collections::HashMap;
use atlas_common::crypto::threshold_crypto::{CombineSignatureError, PartialSignature};
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::ordering_protocol::networking::serialize::NetworkView;
use getset::{Getters, Setters};
use thiserror::Error;

/// The log of votes for a given decision instance
pub enum MsgDecisionLog {
    Leader(LeaderDecisionLog),
    Replica(ReplicaDecisionLog),
}

#[derive(Setters, Getters)]
pub struct DecisionLog<D> {
    #[getset(get = "pub", set = "pub")]
    current_proposal: Option<DecisionNode<D>>,
    #[getset(get = "pub", set = "pub")]
    prepare_qc: Option<QC>,
    #[getset(get = "pub", set = "pub")]
    pre_commit_qc: Option<QC>,
    #[getset(get = "pub", set = "pub")]
    commit_qc: Option<QC>,
}

pub struct VoteStore {
    vote_type: QCType,
    decision_nodes: HashMap<DecisionNodeHeader, HashMap<NodeId, PartialSignature>>,
}

pub struct NewViewStore {
    new_view: HashMap<Option<QC>, HashMap<NodeId, PartialSignature>>,
}

pub struct LeaderDecisionLog {
    high_qc: NewViewStore,
    prepare_qc: VoteStore,
    pre_commit_qc: VoteStore,
    commit_qc: VoteStore,
}

#[derive(Getters)]
pub struct ReplicaDecisionLog {
    #[get = "pub(super)"]
    prepare_qc: Option<QC>,
    #[get = "pub(super)"]
    locked_qc: Option<QC>,
}

impl MsgDecisionLog {
    pub fn as_replica(&self) -> Option<&ReplicaDecisionLog> {
        match self {
            MsgDecisionLog::Replica(replica) => Some(replica),
            _ => None,
        }
    }

    pub fn as_leader(&self) -> Option<&LeaderDecisionLog> {
        match self {
            MsgDecisionLog::Leader(leader) => Some(leader),
            _ => None,
        }
    }

    pub fn as_mut_replica(&mut self) -> Option<&mut ReplicaDecisionLog> {
        match self {
            MsgDecisionLog::Replica(replica) => Some(replica),
            _ => None,
        }
    }

    pub fn as_mut_leader(&mut self) -> Option<&mut LeaderDecisionLog> {
        match self {
            MsgDecisionLog::Leader(leader) => Some(leader),
            _ => None,
        }
    }
}

impl<D> Default for DecisionLog<D> {
    fn default() -> Self {
        Self {
            current_proposal: Option::default(),
            prepare_qc: Option::default(),
            pre_commit_qc: Option::default(),
            commit_qc: Option::default(),
        }
    }
}

impl VoteStore {
    fn new(vote_type: QCType) -> Self {
        Self {
            vote_type,
            decision_nodes: HashMap::default(),
        }
    }

    pub(super) fn accept_vote(
        &mut self,
        voter: NodeId,
        voted_node: DecisionNodeHeader,
        vote_signature: PartialSignature,
    ) {
        self.decision_nodes
            .entry(voted_node)
            .or_insert_with(HashMap::default)
            .insert(voter, vote_signature);
    }

    pub(super) fn generate_qc<CR, CP>(
        &mut self,
        crypto_info: &CR,
        view: &View,
    ) -> Result<QC, VoteStoreError>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        let decision_node = self
            .decision_nodes
            .iter()
            .max_by_key(|(_, votes)| votes.len());

        if let Some((node, votes)) = decision_node {
            let votes = votes
                .iter()
                .map(|(node, sig)| (*node, sig.clone()))
                .collect::<Vec<_>>();

            match combine_partial_signatures::<CR, CP>(crypto_info, &votes) {
                Ok(signature) => Ok(QC::new(
                    self.vote_type.clone(),
                    view.sequence_number(),
                    node.clone(),
                    signature,
                )),
                Err(err) => Err(VoteStoreError::FailedToCreateCombinedSignature),
            }
        } else {
            Err(VoteStoreError::NoDecisionNode)
        }
    }
}

impl LeaderDecisionLog {
    pub(in super::super) fn new_view_store(&mut self) -> &mut NewViewStore {
        &mut self.high_qc
    }

    pub(in super::super) fn accept_vote(
        &mut self,
        sender: NodeId,
        vote: VoteMessage,
    ) -> Result<(), VoteAcceptError> {
        let (vote, signature) = vote.into_inner();

        match vote {
            VoteType::NewView(_) => Err(VoteAcceptError::NewViewVoteNotAcceptable),
            VoteType::PrepareVote(vote) => Ok(self.prepare_qc.accept_vote(sender, vote, signature)),
            VoteType::PreCommitVote(vote) => {
                Ok(self.pre_commit_qc.accept_vote(sender, vote, signature))
            }
            VoteType::CommitVote(commit_vote) => {
                Ok(self.commit_qc.accept_vote(sender, commit_vote, signature))
            }
        }
    }

    pub(in super::super) fn generate_qc<CR, CP>(
        &mut self,
        crypto_info: &CR,
        view: &View,
        qc_type: QCType,
    ) -> Result<QC, VoteStoreError>
    where
        CR: CryptoInformationProvider,
        CP: CryptoProvider,
    {
        match qc_type {
            QCType::PrepareVote => self.prepare_qc.generate_qc::<CR, CP>(crypto_info, view),
            QCType::PreCommitVote => self.pre_commit_qc.generate_qc::<CR, CP>(crypto_info, view),
            QCType::CommitVote => self.commit_qc.generate_qc::<CR, CP>(crypto_info, view),
        }
    }
}

impl Default for LeaderDecisionLog {
    fn default() -> Self {
        Self {
            high_qc: NewViewStore::default(),
            prepare_qc: VoteStore::new(QCType::PrepareVote),
            pre_commit_qc: VoteStore::new(QCType::PreCommitVote),
            commit_qc: VoteStore::new(QCType::CommitVote),
        }
    }
}

impl ReplicaDecisionLog {
    pub(in super::super) fn accept_proposal<D>(
        &mut self,
        proposal: ProposalMessage<D>,
    ) -> Result<(), ProposalAcceptError> {
        match proposal.into() {
            ProposalType::Prepare(_, _) => Err(ProposalAcceptError::PrepareProposalNotAcceptable),
            ProposalType::PreCommit(qc) => {
                self.prepare_qc = Some(qc);

                Ok(())
            }
            ProposalType::Commit(locked_qc) => {
                self.locked_qc = Some(locked_qc);

                Ok(())
            }
            ProposalType::Decide(_) => Ok(()),
        }
    }
}

impl Default for ReplicaDecisionLog {
    fn default() -> Self {
        Self {
            prepare_qc: Option::default(),
            locked_qc: Option::default(),
        }
    }
}

impl NewViewStore {
    pub(in super::super) fn accept_new_view(
        &mut self,
        voter: NodeId,
        vote_message: VoteMessage,
    ) -> Result<(), NewViewAcceptError> {
        let (qc, sig) = match vote_message.into_inner() {
            (VoteType::NewView(qc), sig) => (qc.clone(), sig),
            _ => return Err(NewViewAcceptError::WrongMessageType),
        };

        self.new_view
            .entry(qc)
            .or_insert_with(HashMap::default)
            .insert(voter, sig);

        Ok(())
    }

    pub(in super::super) fn get_high_qc(&self) -> Option<&QC> {
        self.new_view.keys().max_by_key(|qc| qc.as_ref().map(|qc| qc.sequence_number()))
            .map(Option::as_ref).flatten()
    }
    
    pub(in super::super) fn create_new_qc<CR, CP>(&self, crypto_info: &CR, decision_node_header: &DecisionNodeHeader) -> Result<QC, NewViewGenerateError<CP::CombinationError>>
    where
        CR: CryptoInformationProvider,
        CP: CryptoSignatureCombiner,
    {
        let (qc, votes) = self
            .new_view
            .iter()
            .max_by_key(|(qc, _)| qc.as_ref().map(|f| f.sequence_number()))
            .unwrap();

        let votes = votes
            .iter()
            .map(|(node, sig)| (*node, sig.clone()))
            .collect::<Vec<_>>();

        let combined_signature = combine_partial_signatures::<_, CP>(crypto_info, &votes).map_err(|err| NewViewGenerateError::FailedToCombinePartialSignatures(err))?;

        if let Some(qc) = qc {
            Ok(QC::new(
                QCType::PrepareVote,
                qc.view_seq(),
                decision_node_header.clone(),
                combined_signature
            ))
        } else {
            Ok(QC::new(
                QCType::PrepareVote,
                SeqNo::ONE,
                decision_node_header.clone(),
                combined_signature
            ))
        }
    }
}

impl Default for NewViewStore {
    fn default() -> Self {
        Self {
            new_view: HashMap::default(),
        }
    }
}

#[derive(Error, Debug)]
pub enum DecisionError {
    #[error("The received prepare certificate is empty")]
    PrepareCertificateEmpty(),
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
    PrepareProposalNotAcceptable,
}

#[derive(Error, Debug)]
pub enum NewViewAcceptError {
    #[error("Wrong message passed")]
    WrongMessageType,
}

#[derive(Error, Debug)]
pub enum NewViewGenerateError<CS: Error> {
    #[error("Failed to generate high qc")]
    FailedToGenerateHighQC,
    #[error("Failed to combine partial signatures")]
    FailedToCombinePartialSignatures(#[from] CS),
}