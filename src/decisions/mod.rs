use std::sync::Arc;
use atlas_common::crypto::hash::Digest;
use atlas_common::crypto::threshold_crypto::CombinedSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::messages::StoredRequestMessage;
use crate::messages::QC;
use crate::view::View;

mod decision;
mod msg_queue;
mod log;
mod hotstuff;

/// The decision node header
#[derive(PartialEq, Eq, Clone)]
pub struct DecisionNodeHeader {
    view_no: SeqNo,
    previous_block: Option<Digest>,
    current_block_digest: Digest,
    contained_client_commands: usize
}

/// The decision node
#[derive(PartialEq, Eq, Clone)]
pub struct DecisionNode<D> {
    decision_header: DecisionNodeHeader,
    client_commands: Arc<[StoredRequestMessage<D>]>,
}

pub enum DecNode<D> {
    FullNode {
        decision_header: DecisionNodeHeader,
        client_commands: Arc<[StoredRequestMessage<D>]>,
    },
    ShortNode {
        decision_header: DecisionNodeHeader,
    },
}

pub struct DecisionTree {}

pub struct DecisionHandler<D> {}

impl<D> DecisionHandler<D> {
    fn safe_node(&self, node: &DecisionNode<D>, qc: &QC<D>) -> bool {
        todo!("Implement this according to our tree")
    }

    fn latest_qc(&self) -> Option<QC<D>> {
        todo!("Implement this according to our tree")
    }
}

impl Orderable for DecisionNodeHeader {
    fn sequence_number(&self) -> SeqNo {
        self.view_no
    }
}

impl DecisionNodeHeader {
    fn initialize_header_from_previous(digest: Digest, previous: &DecisionNodeHeader, contained_commands: usize) -> Self {
        Self {
            view_no: previous.sequence_number().next(),
            previous_block: Some(previous.current_block_digest.clone()),
            current_block_digest: digest,
            contained_client_commands: contained_commands,
        }
    }

    fn initialize_root_node(view: &View, digest: Digest, contained_commands: usize) -> Self {
        Self {
            view_no: view.sequence_number(),
            previous_block: None,
            current_block_digest: digest,
            contained_client_commands: contained_commands,
        }
    }
}

impl<D> Orderable for DecisionNode<D> {
    fn sequence_number(&self) -> SeqNo {
        self.decision_header.sequence_number()
    }
}

impl<D> DecisionNode<D> {
    fn create_leaf(previous_node: &DecisionNode<D>, digest: Digest, client_commands: Vec<StoredRequestMessage<D>>) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_header_from_previous(digest, &previous_node.decision_header, client_commands.len()),
            client_commands: Arc::from(client_commands),
        }
    }

    fn create_root_leaf(view: &View, digest: Digest, client_commands: Vec<StoredRequestMessage<D>>) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_root_node(view, digest, client_commands.len()),
            client_commands: Arc::from(client_commands),
        }
    }

    fn has_previous(&self) -> bool {
        self.decision_header.previous_block.is_some()
    }

    fn extends_from(&self, prev_node: &DecisionNode<D>) -> bool {
        let previous_node = self.decision_header.previous_block;

        if let Some(digest) = previous_node {
            if digest == prev_node.decision_header.current_block_digest {
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
/// A quorum certificate
pub struct QC<D> {
    qc_type: QCType,
    view_seq: SeqNo,
    decision_node: DecisionNode<D>,
    // The combined signature of the quorum which voted for this QC
    signature: CombinedSignature,
}

impl<D> QC<D> {
    pub fn new(qc_type: QCType,
               view_seq: SeqNo,
               decision_node: DecisionNode<D>,
               signature: CombinedSignature) -> Self {
        QC {
            qc_type,
            view_seq,
            decision_node,
            signature,
        }
    }

    pub fn qc_type(&self) -> &QCType {
        &self.qc_type
    }
    pub fn view_seq(&self) -> SeqNo {
        self.view_seq
    }
    pub fn decision_node(&self) -> &DecisionNode<D> {
        &self.decision_node
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum QCType {
    PrepareVote,
    PreCommitVote,
    CommitVote,
}