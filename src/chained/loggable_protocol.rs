use crate::chained::chained_decision_tree::ChainedDecisionNode;
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::messages::{IronChainMessage, IronChainMessageType};
use crate::chained::proof::{ChainedProof, ProofQCType};
use crate::chained::{ChainedQC, IronChain};
use crate::crypto::CryptoInformationProvider;
use crate::decision_tree::{DecisionNodeHeader, TQuorumCertificate};
use atlas_common::collections::HashMap;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::StoredMessage;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::messages::{ClientRqInfo, SessionBased};
use atlas_core::ordering_protocol::loggable::message::PersistentOrderProtocolTypes;
use atlas_core::ordering_protocol::loggable::{
    DecomposedProof, LoggableOrderProtocol, OrderProtocolLogHelper, PProof,
};
use atlas_core::ordering_protocol::networking::serialize::{
    OrderProtocolVerificationHelper, OrderingProtocolMessage,
};
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{
    BatchedDecision, ProtocolConsensusDecision, ProtocolMessage, ShareableConsensusMessage,
};
use either::Either;
use std::sync::Arc;
use strum::IntoEnumIterator;
use thiserror::Error;

const VOTE_GENERIC: &str = "VOTE-GENERIC";
const PROPOSAL_GENERIC: &str = "PROPOSAL-GENERIC";
const NEW_VIEW_GENERIC: &str = "NEW-VIEW-GENERIC";

impl<RQ, NT, CR> LoggableOrderProtocol<RQ> for IronChain<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    CR: CryptoInformationProvider,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
{
    type PersistableTypes = IronChainSer<RQ>;
}

impl<RQ, NT, CR> OrderProtocolLogHelper<RQ, IronChainSer<RQ>, IronChainSer<RQ>>
    for IronChain<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    CR: CryptoInformationProvider,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
{
    fn message_types() -> Vec<&'static str> {
        vec![VOTE_GENERIC, PROPOSAL_GENERIC, NEW_VIEW_GENERIC]
    }

    fn get_type_for_message(
        msg: &ProtocolMessage<RQ, IronChainSer<RQ>>,
    ) -> atlas_common::error::Result<&'static str> {
        match msg.message() {
            IronChainMessageType::Proposal(_) => Ok(PROPOSAL_GENERIC),
            IronChainMessageType::Vote(_) => Ok(VOTE_GENERIC),
            IronChainMessageType::NewView(_) => Ok(NEW_VIEW_GENERIC),
        }
    }

    fn init_proof_from(
        decision_node_header: DecisionNodeHeader,
        qcs: Vec<ChainedQC>,
        messages: Vec<StoredMessage<ProtocolMessage<RQ, IronChainSer<RQ>>>>,
    ) -> atlas_common::error::Result<ChainedProof<RQ>> {
        let (decision_node, proposal_message) =
            Self::get_decision_header(&decision_node_header, messages)?;

        let indexed_qcs = Self::get_qcs_index_from_vec(qcs, &decision_node_header)?;

        Ok(ChainedProof::new(
            decision_node,
            proposal_message,
            indexed_qcs,
        ))
    }

    fn init_proof_from_scm(
        decision_node_header: DecisionNodeHeader,
        qcs: Vec<ChainedQC>,
        messages: Vec<ShareableConsensusMessage<RQ, IronChainSer<RQ>>>,
    ) -> atlas_common::error::Result<PProof<RQ, IronChainSer<RQ>, IronChainSer<RQ>>> {
        let messages = messages
            .into_iter()
            .map(Arc::unwrap_or_clone)
            .collect::<Vec<_>>();

        let (decision_node, message) = Self::get_decision_header(&decision_node_header, messages)?;

        let indexed_qcs = Self::get_qcs_index_from_vec(qcs, &decision_node_header)?;

        Ok(ChainedProof::new(decision_node, message, indexed_qcs))
    }

    fn decompose_proof(
        proof: &PProof<RQ, IronChainSer<RQ>, IronChainSer<RQ>>,
    ) -> DecomposedProof<RQ, IronChainSer<RQ>> {
        (
            proof.decision_node().decision_header(),
            proof.qcs().values().collect(),
            vec![proof.proposal_message()],
        )
    }

    fn get_requests_in_proof(
        proof: &PProof<RQ, IronChainSer<RQ>, IronChainSer<RQ>>,
    ) -> atlas_common::error::Result<ProtocolConsensusDecision<RQ>> {
        let seq_no = proof.sequence_number();

        let node = proof.decision_node().clone();

        let digest = node.decision_header().current_block_digest();

        let requests = node.into_decision_node().into_commands();

        let client_rq_info = requests.iter().map(ClientRqInfo::from).collect::<Vec<_>>();

        let batched_decision = BatchedDecision::new_with_batch(seq_no, requests);

        Ok(ProtocolConsensusDecision::new(
            seq_no,
            batched_decision,
            client_rq_info,
            digest,
        ))
    }
}

impl<RQ, NT, CR> IronChain<RQ, NT, CR>
where
    CR: CryptoInformationProvider,
    NT: 'static + OrderProtocolSendNode<RQ, IronChainSer<RQ>>,
    RQ: SerMsg + SessionBased,
{
    fn get_decision_header(
        decision_node_header: &DecisionNodeHeader,
        messages: Vec<StoredMessage<IronChainMessage<RQ>>>,
    ) -> Result<(ChainedDecisionNode<RQ>, StoredMessage<IronChainMessage<RQ>>), IronChainComposeError>
    {
        let proposal_message = messages
            .into_iter()
            .find(|msg| matches!(msg.message().message(), IronChainMessageType::Proposal(_)))
            .ok_or(IronChainComposeError::NoProposalMessageFound)?;

        let (_, message) = proposal_message.clone().into_inner();

        let (seq_no, message) = message.into_parts();

        if seq_no != decision_node_header.sequence_number() {
            return Err(
                IronChainComposeError::DecisionNodeDoesNotMatchSequenceNumber {
                    expected: decision_node_header.sequence_number(),
                    found: seq_no,
                },
            );
        }

        let IronChainMessageType::Proposal(proposal) = message else {
            return Err(IronChainComposeError::NoProposalMessageFound);
        };

        let decision_node = proposal.into_parts();

        if *decision_node.decision_header() != *decision_node_header {
            return Err(IronChainComposeError::DecisionNodeDoesNotMatchHeader {
                expected: Box::new(*decision_node_header),
                found: Box::new(*decision_node.decision_header()),
            });
        }

        Ok((decision_node, proposal_message))
    }

    fn get_qcs_index_from_vec(
        qcs: Vec<ChainedQC>,
        decision: &DecisionNodeHeader,
    ) -> Result<HashMap<ProofQCType, ChainedQC>, IronChainComposeError> {
        let mut indexed_qcs = HashMap::default();

        let decision_seq = decision.sequence_number();

        qcs.into_iter().try_for_each(|qc| {
            if *qc.decision_node() != *decision {
                return Err(IronChainComposeError::QCDecisionNodeDoesNotMatchHeader);
            }

            //TODO: This does not have to be all in a row
            // As we can skip certain decisions. How to handle that?
            match qc.sequence_number().index(decision_seq) {
                Either::Right(0) => {
                    indexed_qcs.insert(ProofQCType::PrePrepare, qc);
                }
                Either::Right(1) => {
                    indexed_qcs.insert(ProofQCType::Prepare, qc);
                }
                Either::Right(2) => {
                    indexed_qcs.insert(ProofQCType::Commit, qc);
                }
                Either::Right(3) => {
                    indexed_qcs.insert(ProofQCType::Decide, qc);
                }
                Either::Right(_) => {
                    return Err(IronChainComposeError::QCNewerThanDecision {
                        found: qc.sequence_number(),
                        decision_seq,
                    });
                }
                Either::Left(_) => {
                    return Err(IronChainComposeError::QCOlderThanDecision {
                        found: qc.sequence_number(),
                        decision_seq,
                    });
                }
            }

            Ok(())
        })?;

        Ok(indexed_qcs)
    }
}

impl<RQ> PersistentOrderProtocolTypes<RQ, Self> for IronChainSer<RQ>
where
    RQ: SerMsg,
{
    type Proof = ChainedProof<RQ>;

    fn verify_proof<NI, OPVH>(
        _network_info: &Arc<NI>,
        proof: Self::Proof,
    ) -> atlas_common::error::Result<Self::Proof>
    where
        NI: NetworkInformationProvider,
        Self: OrderingProtocolMessage<RQ>,
        OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
        Self: Sized,
    {
        //TODO: Verify the proof correctness
        Ok(proof)
    }
}

#[derive(Debug, Error)]
pub enum IronChainComposeError {
    #[error("Tried composing proof from messages, but no proposal message was found.")]
    NoProposalMessageFound,
    #[error("The provided proposal message does not match the decision node header. Expected: {expected:?}, found: {found:?}"
    )]
    DecisionNodeDoesNotMatchHeader {
        expected: Box<DecisionNodeHeader>,
        found: Box<DecisionNodeHeader>,
    },
    #[error("The provided QC does not match the decision node header.")]
    QCDecisionNodeDoesNotMatchHeader,
    #[error(
        "The sequence number of the proposal message does not match the decision node header."
    )]
    DecisionNodeDoesNotMatchSequenceNumber { expected: SeqNo, found: SeqNo },
    #[error("Found QC for sequence number {found:?}, but it is older than the decision {decision_seq:?}.")]
    QCOlderThanDecision { found: SeqNo, decision_seq: SeqNo },
    #[error("Found QC for sequence number {found:?}, but it is newer than the decision {decision_seq:?}.")]
    QCNewerThanDecision { found: SeqNo, decision_seq: SeqNo },
}
