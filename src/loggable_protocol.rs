use crate::crypto::CryptoInformationProvider;
use crate::decisions::proof::Proof;
use crate::messages::serialize::HotIronOxSer;
use crate::messages::{HotFeOxMsgType, ProposalType, VoteType};
use crate::HotIron;
use crate::SerMsg;
use atlas_common::ordering::Orderable;
use atlas_communication::message::StoredMessage;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::messages::SessionBased;
use atlas_core::ordering_protocol::loggable::message::PersistentOrderProtocolTypes;
use atlas_core::ordering_protocol::loggable::{
    DecomposedProof, LoggableOrderProtocol, OrderProtocolLogHelper, PProof,
};
use atlas_core::ordering_protocol::networking::serialize::{
    OrderProtocolVerificationHelper, OrderingProtocolMessage,
};
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{
    BatchedDecision, DecisionAD, DecisionMetadata, ProtocolConsensusDecision, ProtocolMessage,
    ShareableConsensusMessage, ShareableMessage,
};
use std::sync::Arc;

pub const VOTE_NEW_VIEW: &str = "VOTE-NEW-VIEW";
pub const PROPOSAL_PREPARE: &str = "PROPOSAL-PREPARE";
pub const VOTE_PREPARE: &str = "VOTE-PREPARE";
pub const PROPOSAL_PRE_COMMIT: &str = "PROPOSAL-PRE-COMMIT";
pub const VOTE_PRE_COMMIT: &str = "VOTE-PRE-COMMIT";
pub const PROPOSAL_COMMIT: &str = "PROPOSAL-COMMIT";
pub const VOTE_COMMIT: &str = "VOTE-COMMIT";
pub const PROPOSAL_DECIDE: &str = "PROPOSAL-DECIDE";

impl<RQ, NT, CR> LoggableOrderProtocol<RQ> for HotIron<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: CryptoInformationProvider + Send + Sync + 'static,
{
    type PersistableTypes = HotIronOxSer<RQ>;
}

impl<RQ, NT, CR> OrderProtocolLogHelper<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>
    for HotIron<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    NT: OrderProtocolSendNode<RQ, HotIronOxSer<RQ>> + 'static,
    CR: Send + Sync + 'static,
{
    fn message_types() -> Vec<&'static str> {
        vec![
            VOTE_NEW_VIEW,
            PROPOSAL_PREPARE,
            VOTE_PREPARE,
            PROPOSAL_PRE_COMMIT,
            VOTE_PRE_COMMIT,
            PROPOSAL_COMMIT,
            VOTE_COMMIT,
            PROPOSAL_DECIDE,
        ]
    }

    fn get_type_for_message(
        msg: &ProtocolMessage<RQ, HotIronOxSer<RQ>>,
    ) -> atlas_common::error::Result<&'static str> {
        match msg.message() {
            HotFeOxMsgType::Proposal(msg) => match msg.proposal_type() {
                ProposalType::Prepare(_, _) => Ok(PROPOSAL_PREPARE),
                ProposalType::PreCommit(_) => Ok(PROPOSAL_PRE_COMMIT),
                ProposalType::Commit(_) => Ok(PROPOSAL_COMMIT),
                ProposalType::Decide(_) => Ok(PROPOSAL_DECIDE),
            },
            HotFeOxMsgType::Vote(msg) => match msg.vote_type() {
                VoteType::NewView(_) => Ok(VOTE_NEW_VIEW),
                VoteType::PrepareVote(_) => Ok(VOTE_PREPARE),
                VoteType::PreCommitVote(_) => Ok(VOTE_PRE_COMMIT),
                VoteType::CommitVote(_) => Ok(VOTE_COMMIT),
            },
        }
    }

    fn init_proof_from(
        metadata: DecisionMetadata<RQ, HotIronOxSer<RQ>>,
        additional_data: Vec<DecisionAD<RQ, HotIronOxSer<RQ>>>,
        messages: Vec<StoredMessage<ProtocolMessage<RQ, HotIronOxSer<RQ>>>>,
    ) -> atlas_common::error::Result<PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>> {

        Ok(Proof::new_from_storage(metadata, additional_data, messages)?)
    }

    fn init_proof_from_scm(
        metadata: DecisionMetadata<RQ, HotIronOxSer<RQ>>,
        additional_data: Vec<DecisionAD<RQ, HotIronOxSer<RQ>>>,
        messages: Vec<ShareableConsensusMessage<RQ, HotIronOxSer<RQ>>>,
    ) -> atlas_common::error::Result<PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>> {
        let cloned_messages = messages.iter()
            .map(|msg| (**msg).clone()).collect::<Vec<_>>();
        
        Ok(Proof::new_from_storage(metadata, additional_data, cloned_messages)?)
    }

    fn decompose_proof(
        proof: &PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>,
    ) -> DecomposedProof<RQ, HotIronOxSer<RQ>> {
        (
            proof.decision_node().decision_header(),
            proof.qcs().values().collect(),
            proof.messages().iter().collect(),
        )
    }

    fn get_requests_in_proof(
        proof: &PProof<RQ, HotIronOxSer<RQ>, HotIronOxSer<RQ>>,
    ) -> atlas_common::error::Result<ProtocolConsensusDecision<RQ>> {
        let commands = proof.decision_node().client_commands().clone();

        let decision = BatchedDecision::new_with_batch(proof.sequence_number(), commands);

        let digest = proof
            .decision_node()
            .decision_header()
            .current_block_digest();

        Ok(ProtocolConsensusDecision::new(
            proof.sequence_number(),
            decision,
            vec![],
            digest,
        ))
    }
}

impl<RQ> PersistentOrderProtocolTypes<RQ, Self> for HotIronOxSer<RQ>
where
    RQ: SerMsg,
{
    type Proof = Proof<RQ>;

    fn verify_proof<NI, OPVH>(
        _network_info: &Arc<NI>,
        proof: Self::Proof,
    ) -> atlas_common::error::Result<Self::Proof>
    where
        NI: NetworkInformationProvider,
        Self: OrderingProtocolMessage<RQ> + Sized,
        OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
    {
        Ok(proof)
    }
}
