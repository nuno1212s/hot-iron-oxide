use atlas_common::crypto::hash::Digest;
use atlas_common::crypto::signature::Signature;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_core::messages::StoredRequestMessage;
use crate::view::View;

mod decision;
mod msg_queue;
mod log;

pub struct DecisionNodeHeader {
    view_no: SeqNo,
    previous_block: Option<Digest>,
    current_block_digest: Digest,
}

pub struct DecisionNode<D> {
    decision_header: DecisionNodeHeader,
    client_commands: Vec<StoredRequestMessage<D>>,
}

pub struct DecisionTree {}

impl Orderable for DecisionNodeHeader {
    fn sequence_number(&self) -> SeqNo {
        self.view_no
    }
}

impl DecisionNodeHeader {
    fn initialize_header_from_previous(digest: Digest, previous: &DecisionNodeHeader) -> Self {
        Self {
            view_no: previous.sequence_number().next(),
            previous_block: Some(previous.current_block_digest.clone()),
            current_block_digest: digest,
        }
    }

    fn initialize_root_node(view: &View, digest: Digest) -> Self {
        Self {
            view_no: view.sequence_number(),
            previous_block: None,
            current_block_digest: digest,
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
            decision_header: DecisionNodeHeader::initialize_header_from_previous(digest, &previous_node.decision_header),
            client_commands,
        }
    }
    
    fn create_root_leaf(view: &View, digest: Digest, client_commands: Vec<StoredRequestMessage<D>>) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_root_node(view, digest),
            client_commands,
        }
    }
    
}