use crate::view::View;
use atlas_common::crypto::hash::Digest;
use atlas_common::crypto::threshold_crypto::CombinedSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_common::serialization_helper::SerMsg;
use atlas_core::messages::StoredRequestMessage;
use getset::{CopyGetters, Getters};
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::Arc;
use atlas_communication::message::StoredMessage;

pub(crate) mod decision;
pub(crate) mod hotstuff;
mod log;
mod msg_queue;
pub(crate) mod req_aggr;

/// The decision node header

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Hash, Copy)]
pub struct DecisionNodeHeader {
    view_no: SeqNo,
    previous_block: Option<Digest>,
    current_block_digest: Digest,
    contained_client_commands: usize,
}

/// The decision nod
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Getters, CopyGetters)]
pub struct DecisionNode<D> {
    #[getset(get_copy = "pub")]
    decision_header: DecisionNodeHeader,
    #[getset(get = "pub")]
    client_commands: Vec<StoredMessage<D>>,
}

pub struct DecisionTree {}

pub struct DecisionHandler<D>(PhantomData<fn() -> D>);

impl<D> DecisionHandler<D>
where
    D: SerMsg,
{
    fn safe_node(&self, node: &DecisionNode<D>, qc: &QC) -> bool {
        true
    }

    fn latest_qc(&self) -> Option<QC> {
        None
    }
}

impl<D> Default for DecisionHandler<D> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl Orderable for DecisionNodeHeader {
    fn sequence_number(&self) -> SeqNo {
        self.view_no
    }
}

impl DecisionNodeHeader {
    fn initialize_header_from_previous(
        digest: Digest,
        previous: &DecisionNodeHeader,
        contained_commands: usize,
    ) -> Self {
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
    pub fn create_leaf(
        previous_node: &DecisionNodeHeader,
        digest: Digest,
        client_commands: Vec<StoredMessage<D>>,
    ) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_header_from_previous(
                digest,
                previous_node,
                client_commands.len(),
            ),
            client_commands,
        }
    }

    pub fn create_root_leaf(
        view: &View,
        digest: Digest,
        client_commands: Vec<StoredMessage<D>>,
    ) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_root_node(
                view,
                digest,
                client_commands.len(),
            ),
            client_commands,
        }
    }

    fn has_previous(&self) -> bool {
        self.decision_header.previous_block.is_some()
    }

    fn extends_from(&self, prev_node: &DecisionNodeHeader) -> bool {
        let previous_node = self.decision_header.previous_block;

        if let Some(digest) = previous_node {
            digest == prev_node.current_block_digest
        } else {
            true
        }
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Getters, CopyGetters, Hash, PartialEq, Eq)]
/// A quorum certificate
pub struct QC {
    #[get_copy = "pub"]
    qc_type: QCType,
    #[get_copy = "pub"]
    view_seq: SeqNo,
    #[get_copy = "pub"]
    decision_node: DecisionNodeHeader,
    // The combined signature of the quorum which voted for this QC
    #[get = "pub"]
    signature: CombinedSignature,
}

impl QC {
    pub fn new(
        qc_type: QCType,
        view_seq: SeqNo,
        decision_node: DecisionNodeHeader,
        signature: CombinedSignature,
    ) -> Self {
        QC {
            qc_type,
            view_seq,
            decision_node,
            signature,
        }
    }
}

impl Orderable for QC {
    fn sequence_number(&self) -> SeqNo {
        self.view_seq
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

impl<D> Clone for DecisionNode<D>
where
    D: Clone,
{
    fn clone(&self) -> Self {
        Self {
            decision_header: self.decision_header.clone(),
            client_commands: self.client_commands.clone(),
        }
    }
}

impl<D> From<DecisionNode<D>> for (DecisionNodeHeader, Vec<StoredMessage<D>>) {
    fn from(value: DecisionNode<D>) -> Self {
        (value.decision_header, value.client_commands)
    }
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, PartialEq, Eq, Hash, Copy)]
pub enum QCType {
    PrepareVote,
    PreCommitVote,
    CommitVote,
}
