use crate::crypto::{
    combine_partial_signatures, CryptoInformationProvider, CryptoProvider, CryptoSignatureCombiner,
};
use crate::decisions::{DecisionNode, DecisionNodeHeader, QCType, QC};
use crate::messages::{ProposalMessage, ProposalType, VoteMessage, VoteType};
use crate::view::View;
use atlas_common::collections::HashMap;
use atlas_common::crypto::threshold_crypto::PartialSignature;
use atlas_common::node_id::NodeId;
use atlas_common::ordering::{Orderable, SeqNo};
use getset::{Getters, MutGetters, Setters};
use std::error::Error;
use thiserror::Error;

/// The log of votes for a given decision instance
pub enum MsgDecisionLog {
    Leader(MsgLeaderDecisionLog),
    Replica(MsgReplicaDecisionLog),
}

#[derive(Default)]
pub struct LeaderDecisionLog {
    prepare_qc: Option<QC>,
    pre_commit_qc: Option<QC>,
    commit_qc: Option<QC>,
}

#[derive(Default)]
pub struct ReplicaDecisionLog {
    prepare_qc: Option<QC>,
    locked_qc: Option<QC>,
}

pub enum DecisionLogType {
    Leader(LeaderDecisionLog),
    Replica(ReplicaDecisionLog),
}

#[derive(Setters, Getters, MutGetters)]
pub struct DecisionLog<D> {
    #[getset(get = "pub(super)", set = "pub(super)")]
    current_proposal: Option<DecisionNode<D>>,
    decision_log_type: DecisionLogType,
}

pub struct VoteStore {
    vote_type: QCType,
    decision_nodes: HashMap<DecisionNodeHeader, HashMap<NodeId, PartialSignature>>,
}

#[derive(Default)]
pub struct NewViewStore {
    new_view: HashMap<Option<QC>, HashMap<NodeId, PartialSignature>>,
}

pub struct MsgLeaderDecisionLog {
    high_qc: NewViewStore,
    prepare_qc: VoteStore,
    pre_commit_qc: VoteStore,
    commit_qc: VoteStore,
}

#[derive(Default, Getters)]
pub struct MsgReplicaDecisionLog {
    #[get = "pub(super)"]
    prepare_qc: Option<QC>,
    #[get = "pub(super)"]
    locked_qc: Option<QC>,
}

impl MsgDecisionLog {
    pub fn as_replica(&self) -> Option<&MsgReplicaDecisionLog> {
        match self {
            MsgDecisionLog::Replica(replica) => Some(replica),
            _ => None,
        }
    }

    pub fn as_leader(&self) -> Option<&MsgLeaderDecisionLog> {
        match self {
            MsgDecisionLog::Leader(leader) => Some(leader),
            _ => None,
        }
    }

    pub fn as_mut_replica(&mut self) -> Option<&mut MsgReplicaDecisionLog> {
        match self {
            MsgDecisionLog::Replica(replica) => Some(replica),
            _ => None,
        }
    }

    pub fn as_mut_leader(&mut self) -> Option<&mut MsgLeaderDecisionLog> {
        match self {
            MsgDecisionLog::Leader(leader) => Some(leader),
            _ => None,
        }
    }
}

impl ReplicaDecisionLog {
    pub fn set_prepare_qc(&mut self, qc: QC) {
        self.prepare_qc = Some(qc);
    }

    pub fn set_locked_qc(&mut self, qc: QC) {
        self.locked_qc = Some(qc);
    }
}

impl LeaderDecisionLog {
    pub fn set_prepare_qc(&mut self, qc: QC) {
        self.prepare_qc = Some(qc);
    }

    pub fn set_pre_commit_qc(&mut self, qc: QC) {
        self.pre_commit_qc = Some(qc);
    }

    pub fn set_commit_qc(&mut self, qc: QC) {
        self.commit_qc = Some(qc);
    }
}

impl<D> DecisionLog<D> {
    pub fn new(decision_log_type: DecisionLogType) -> Self {
        Self {
            current_proposal: Default::default(),
            decision_log_type,
        }
    }

    pub fn into_current_proposal(self) -> Option<DecisionNode<D>> {
        self.current_proposal
    }

    pub fn as_replica(&self) -> &ReplicaDecisionLog {
        match &self.decision_log_type {
            DecisionLogType::Replica(replica) => replica,
            _ => unreachable!(),
        }
    }

    pub fn as_mut_replica(&mut self) -> &mut ReplicaDecisionLog {
        match &mut self.decision_log_type {
            DecisionLogType::Replica(replica) => replica,
            _ => unreachable!(),
        }
    }

    pub fn as_leader(&self) -> &LeaderDecisionLog {
        match &self.decision_log_type {
            DecisionLogType::Leader(leader) => leader,
            _ => unreachable!(),
        }
    }

    pub fn as_mut_leader(&mut self) -> &mut LeaderDecisionLog {
        match &mut self.decision_log_type {
            DecisionLogType::Leader(leader) => leader,
            _ => unreachable!(),
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

    /// Accept a vote received through the protocol
    /// # [Returns]
    /// True if the vote was accepted, false if the vote was already present
    pub(super) fn accept_vote(
        &mut self,
        voter: NodeId,
        voted_node: DecisionNodeHeader,
        vote_signature: PartialSignature,
    ) -> bool {
        let previous = self
            .decision_nodes
            .entry(voted_node)
            .or_default()
            .insert(voter, vote_signature);

        previous.is_none()
    }

    pub(super) fn generate_qc<CR, CP>(
        &mut self,
        crypto_info: &CR,
        view: &View,
    ) -> Result<QC, VoteStoreError<CP::CombinationError>>
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
                    self.vote_type,
                    view.sequence_number(),
                    *node,
                    signature,
                )),
                Err(err) => Err(err.into()),
            }
        } else {
            Err(VoteStoreError::NoDecisionNode)
        }
    }
}

impl MsgLeaderDecisionLog {
    pub(in super::super) fn new_view_store(&mut self) -> &mut NewViewStore {
        &mut self.high_qc
    }

    pub(in super::super) fn accept_vote(
        &mut self,
        sender: NodeId,
        vote: VoteMessage,
    ) -> Result<bool, VoteAcceptError> {
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
    ) -> Result<QC, VoteStoreError<CP::CombinationError>>
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

impl Default for MsgLeaderDecisionLog {
    fn default() -> Self {
        Self {
            high_qc: NewViewStore::default(),
            prepare_qc: VoteStore::new(QCType::PrepareVote),
            pre_commit_qc: VoteStore::new(QCType::PreCommitVote),
            commit_qc: VoteStore::new(QCType::CommitVote),
        }
    }
}

impl MsgReplicaDecisionLog {
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

        self.new_view.entry(qc).or_default().insert(voter, sig);

        Ok(())
    }

    pub(in super::super) fn get_high_qc(&self) -> Option<&QC> {
        self.new_view
            .keys()
            .max_by_key(|qc| qc.as_ref().map(|qc| qc.sequence_number()))
            .and_then(Option::as_ref)
    }

    pub(in super::super) fn create_new_qc<CR, CP>(
        &self,
        crypto_info: &CR,
        decision_node_header: &DecisionNodeHeader,
    ) -> Result<QC, NewViewGenerateError<CP::CombinationError>>
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
            Ok(QC::new(
                QCType::PrepareVote,
                qc.view_seq().next(),
                *decision_node_header,
                combined_signature,
            ))
        } else {
            Ok(QC::new(
                QCType::PrepareVote,
                SeqNo::ZERO,
                *decision_node_header,
                combined_signature,
            ))
        }
    }
}

#[derive(Error, Debug)]
pub enum DecisionError {
    #[error("The received prepare certificate is empty")]
    PrepareCertificateEmpty(),
}

#[derive(Error, Debug)]
pub enum VoteStoreError<CS: Error> {
    #[error("There is no decision node present")]
    NoDecisionNode,
    #[error("Failed to create combined signature {0:?}")]
    FailedToCreateCombinedSignature(#[from] CS),
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
    #[error("Failed to combine partial signatures {0:?}")]
    FailedToCombinePartialSignatures(#[from] CS),
    #[error("Failed to collect the highest vote")]
    NotEnoughVotes,
}
