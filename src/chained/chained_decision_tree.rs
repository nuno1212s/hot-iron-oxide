use std::fmt::Debug;
use crate::chained::ChainedQC;
use crate::decision_tree::{DecisionNode, DecisionNodeHeader, TQuorumCertificate};
use crate::view::View;
use atlas_common::collections::HashMap;
use atlas_common::crypto::hash::Digest;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_communication::message::StoredMessage;
use getset::Getters;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Clone, Hash, Eq, PartialEq, Getters)]
pub struct ChainedDecisionNode<D> {
    node: DecisionNode<D>,
    #[get = "pub"]
    justify: Option<ChainedQC>,
}

impl<D> ChainedDecisionNode<D> {
    pub fn create_root(
        view: &View,
        digest: Digest,
        client_commands: Vec<StoredMessage<D>>,
    ) -> Self {
        let node = DecisionNode::create_root(view, digest, client_commands);

        Self {
            node,
            justify: None,
        }
    }

    pub fn create_leaf(
        view_seq: SeqNo,
        previous_node: &DecisionNodeHeader,
        digest: Digest,
        client_commands: Vec<StoredMessage<D>>,
        justify: ChainedQC,
    ) -> Self {
        let node = if previous_node.current_block_digest()
            == justify.decision_node().current_block_digest()
            && justify.decision_node().sequence_number() == view_seq.prev()
        {
            DecisionNode::create_leaf(previous_node, digest, client_commands)
        } else {
            DecisionNode::create_blank_branch_node(view_seq, digest, client_commands)
        };

        Self {
            node,
            justify: Some(justify),
        }
    }

    pub fn into_decision_node(self) -> DecisionNode<D> {
        self.node
    }
}

impl<D> Deref for ChainedDecisionNode<D> {
    type Target = DecisionNode<D>;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl<D> Debug for ChainedDecisionNode<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainedDecisionNode")
            .field("node", &self.node)
            .field("justify", &self.justify)
            .finish()
    }
}

pub struct PendingDecisionNodes<D> {
    map: HashMap<Digest, ChainedDecisionNode<D>>,
}

impl<D> PendingDecisionNodes<D> {
    pub(super) fn new() -> Self {
        Self {
            map: HashMap::default(),
        }
    }

    pub(super) fn insert(&mut self, node: ChainedDecisionNode<D>) {
        self.map
            .insert(node.decision_header().current_block_digest(), node);
    }

    pub(super) fn get(&self, header: &Digest) -> Option<&ChainedDecisionNode<D>> {
        self.map.get(header)
    }

    pub(super) fn remove(&mut self, header: &Digest) -> Option<ChainedDecisionNode<D>> {
        self.map.remove(header)
    }
}

impl<D> Default for PendingDecisionNodes<D> {
    fn default() -> Self {
        Self::new()
    }
}
