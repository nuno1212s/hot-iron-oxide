use std::sync::Arc;
use crate::chained::messages::serialize::IronChainSer;
use crate::chained::IronChain;
use crate::crypto::CryptoInformationProvider;
use atlas_common::serialization_helper::SerMsg;
use atlas_communication::message::StoredMessage;
use atlas_communication::reconfiguration::NetworkInformationProvider;
use atlas_core::messages::SessionBased;
use atlas_core::ordering_protocol::loggable::{DecomposedProof, LoggableOrderProtocol, OrderProtocolLogHelper, PProof};
use atlas_core::ordering_protocol::networking::OrderProtocolSendNode;
use atlas_core::ordering_protocol::{DecisionAD, DecisionMetadata, ProtocolConsensusDecision, ProtocolMessage, ShareableConsensusMessage};
use atlas_core::ordering_protocol::loggable::message::PersistentOrderProtocolTypes;
use atlas_core::ordering_protocol::networking::serialize::{OrderProtocolVerificationHelper, OrderingProtocolMessage};
use crate::chained::messages::IronChainMessageType;

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

impl<RQ, NT, CR> OrderProtocolLogHelper<RQ, IronChainSer<RQ>, IronChainSer<RQ>> for IronChain<RQ, NT, CR>
where
    RQ: SerMsg + SessionBased,
    CR: CryptoInformationProvider,
    NT: OrderProtocolSendNode<RQ, IronChainSer<RQ>> + 'static,
{
    fn message_types() -> Vec<&'static str> {
        vec![
            VOTE_GENERIC,
            PROPOSAL_GENERIC,
            NEW_VIEW_GENERIC
        ]
    }

    fn get_type_for_message(msg: &ProtocolMessage<RQ, IronChainSer<RQ>>) -> atlas_common::error::Result<&'static str> {
        match msg.message() {
            IronChainMessageType::Proposal(_) => Ok(PROPOSAL_GENERIC),
            IronChainMessageType::Vote(_) => Ok(VOTE_GENERIC),
            IronChainMessageType::NewView(_) => Ok(NEW_VIEW_GENERIC)
        }
    }

    fn init_proof_from(metadata: DecisionMetadata<RQ, IronChainSer<RQ>>, additional_data: Vec<DecisionAD<RQ, IronChainSer<RQ>>>, messages: Vec<StoredMessage<ProtocolMessage<RQ, IronChainSer<RQ>>>>) -> atlas_common::error::Result<PProof<RQ, IronChainSer<RQ>, IronChainSer<RQ>>> {
        todo!()
    }

    fn init_proof_from_scm(metadata: DecisionMetadata<RQ, IronChainSer<RQ>>, additional_data: Vec<DecisionAD<RQ, IronChainSer<RQ>>>, messages: Vec<ShareableConsensusMessage<RQ, IronChainSer<RQ>>>) -> atlas_common::error::Result<PProof<RQ, IronChainSer<RQ>, IronChainSer<RQ>>> {
        todo!()
    }

    fn decompose_proof(proof: &PProof<RQ, IronChainSer<RQ>, IronChainSer<RQ>>) -> DecomposedProof<RQ, IronChainSer<RQ>> {
        todo!()
    }

    fn get_requests_in_proof(proof: &PProof<RQ, IronChainSer<RQ>, IronChainSer<RQ>>) -> atlas_common::error::Result<ProtocolConsensusDecision<RQ>> {
        todo!()
    }
}

impl<RQ> PersistentOrderProtocolTypes<RQ, Self> for IronChainSer<RQ>
where
    RQ: SerMsg,
{
    type Proof = ();

    fn verify_proof<NI, OPVH>(network_info: &Arc<NI>, proof: Self::Proof) -> atlas_common::error::Result<Self::Proof>
    where
        NI: NetworkInformationProvider,
        Self: OrderingProtocolMessage<RQ>,
        OPVH: OrderProtocolVerificationHelper<RQ, Self, NI>,
        Self: Sized
    {
        todo!()
    }
}