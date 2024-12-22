use std::hash::Hash;
use std::sync::Arc;
use std::marker::PhantomData;
use atlas_common::crypto::hash::Digest;
use atlas_common::crypto::threshold_crypto::CombinedSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::messages::StoredRequestMessage;
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use atlas_common::serialization_helper::SerType;
use crate::view::View;

mod decision;
mod msg_queue;
mod log;
mod hotstuff;
mod req_aggr;

/// The decision node header

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Hash)]
pub struct DecisionNodeHeader {
    view_no: SeqNo,
    previous_block: Option<Digest>,
    current_block_digest: Digest,
    contained_client_commands: usize
}

/// The decision nod
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
pub struct DecisionNode<D> {
    decision_header: DecisionNodeHeader,
    client_commands: DecNode<D>,
}


#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
pub enum DecNode<D> {
    FullNode {
        client_commands: Arc<[StoredRequestMessage<D>]>,
    },
    ShortNode
}

pub struct DecisionTree {}

pub struct DecisionHandler<D>(PhantomData<fn() -> D>);

impl<D> DecisionHandler<D> where D: SerType {
    fn safe_node(&self, node: &DecisionNode<D>, qc: &QC<D>) -> bool {
        todo!("Implement this according to our tree")
    }

    fn latest_qc(&self) -> Option<QC<D>> {
        todo!("Implement this according to our tree")
    }
}

impl<D> Default for DecisionHandler<D> {
    fn default() -> Self {
        Self (Default::default())
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
    pub fn create_leaf(previous_node: &DecisionNode<D>, digest: Digest, client_commands: Vec<StoredRequestMessage<D>>) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_header_from_previous(digest, &previous_node.decision_header, client_commands.len()),
            client_commands: Arc::<[StoredRequestMessage<D>]>::from(client_commands).into(),
        }
    }

    pub fn create_root_leaf(view: &View, digest: Digest, client_commands: Vec<StoredRequestMessage<D>>) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_root_node(view, digest, client_commands.len()),
            client_commands: Arc::<[StoredRequestMessage<D>]>::from(client_commands).into(),
        }
    }

    pub fn create_short_node(previous_node: &DecisionNode<D>, digest: Digest, contained_commands: usize) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_header_from_previous(digest, &previous_node.decision_header, contained_commands),
            client_commands: DecNode::ShortNode,
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

impl<D> PartialEq for DecisionNode<D> {
    fn eq(&self, other: &Self) -> bool {
        self.decision_header == other.decision_header
    }
}

impl<D> Eq for DecisionNode<D> {}

impl<D> Hash for DecisionNode<D> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.decision_header.hash(state);
    }
}

impl<D> Clone for DecisionNode<D> {
    fn clone(&self) -> Self {
        Self {
            decision_header: self.decision_header.clone(),
            client_commands: self.client_commands.clone(),
        }
    }
}

impl<D> Clone for DecNode<D> {
    fn clone(&self) -> Self {
        match self {
            DecNode::FullNode { client_commands } => DecNode::FullNode {
                client_commands: client_commands.clone(),
            },
            DecNode::ShortNode => DecNode::ShortNode,
        }
    }
}

impl<D> From<Arc<[StoredRequestMessage<D>]>> for DecNode<D> {
    fn from(value: Arc<[StoredRequestMessage<D>]>) -> Self {
        DecNode::FullNode { client_commands: value }
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone)]
pub enum QCType {
    PrepareVote,
    PreCommitVote,
    CommitVote,
}