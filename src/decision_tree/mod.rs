use crate::view::View;
use atlas_common::crypto::hash::{Context, Digest};
use atlas_common::crypto::threshold_crypto::CombinedSignature;
use atlas_common::ordering::{Orderable, SeqNo};
use atlas_communication::message::StoredMessage;
use getset::{CopyGetters, Getters};
#[cfg(feature = "serialize_serde")]
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use tracing::debug;

/// The Trait which specifies all of the required methods for a quorum certificate
pub trait TQuorumCertificate: Orderable + Clone {
    fn decision_node(&self) -> &DecisionNodeHeader;

    fn signature(&self) -> &CombinedSignature;
}

#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Clone, Hash, Copy, Debug, Getters, CopyGetters)]
pub struct DecisionNodeHeader {
    #[get = "pub"]
    view_no: SeqNo,
    #[get = "pub"]
    previous_block: Option<Digest>,
    #[get_copy = "pub"]
    current_block_digest: Digest,
    #[get_copy = "pub"]
    contained_client_commands: usize,
}

impl Orderable for DecisionNodeHeader {
    fn sequence_number(&self) -> SeqNo {
        self.view_no
    }
}

impl DecisionNodeHeader {
    fn initialize_header_from_previous(
        client_command_digest: Digest,
        previous: &DecisionNodeHeader,
        contained_commands: usize,
    ) -> Self {
        let block_digest = Self::calculate_node_header_digest(previous.sequence_number().next(), Some(previous.current_block_digest()), client_command_digest, contained_commands);
        
        Self {
            view_no: previous.sequence_number().next(),
            previous_block: Some(previous.current_block_digest),
            current_block_digest: block_digest,
            contained_client_commands: contained_commands,
        }
    }

    fn initialize_root_node(view: &View, client_command_digest: Digest, contained_commands: usize) -> Self {
        let block_digest = Self::calculate_node_header_digest(
            view.sequence_number(),
            None,
            client_command_digest,
            contained_commands,
        );
        
        Self {
            view_no: view.sequence_number(),
            previous_block: None,
            current_block_digest: block_digest,
            contained_client_commands: contained_commands,
        }
    }

    fn new(
        view_no: SeqNo,
        previous_block: Option<Digest>,
        current_block_digest: Digest,
        contained_client_commands: usize,
    ) -> Self {
        let block_digest = Self::calculate_node_header_digest(
            view_no,
            previous_block,
            current_block_digest,
            contained_client_commands,
        );
        
        Self {
            view_no,
            previous_block,
            current_block_digest,
            contained_client_commands,
        }
    }

    fn calculate_node_header_digest(
        view_seq: SeqNo,
        previous_block: Option<Digest>,
        client_rq_digest: Digest,
        contained_client_commands: usize,
    ) -> Digest {
        let blank = Digest::blank();
        let blank_digest = blank.as_ref();
        
        let mut context = Context::new();
        context.update(&view_seq.into_u32().to_le_bytes()[..]);
        context.update(previous_block.as_ref().map_or_else(|| blank_digest, |d| d.as_ref()));
        context
            .update(client_rq_digest.as_ref());
        context
            .update(&contained_client_commands.to_le_bytes()[..]);
        
        context.finish()
    }
}

/// The decision nod
#[cfg_attr(feature = "serialize_serde", derive(Serialize, Deserialize))]
#[derive(Getters)]
pub struct DecisionNode<D> {
    #[getset(get = "pub")]
    decision_header: DecisionNodeHeader,
    #[getset(get = "pub")]
    client_commands: Vec<StoredMessage<D>>,
}

impl<D> Orderable for DecisionNode<D> {
    fn sequence_number(&self) -> SeqNo {
        self.decision_header.sequence_number()
    }
}

impl<D> DecisionNode<D> {
    #[must_use]
    pub fn create_leaf(
        previous_node: &DecisionNodeHeader,
        client_command_digest: Digest,
        client_commands: Vec<StoredMessage<D>>,
    ) -> Self {
        Self {
            decision_header: DecisionNodeHeader::initialize_header_from_previous(
                client_command_digest,
                previous_node,
                client_commands.len(),
            ),
            client_commands,
        }
    }

    #[must_use]
    pub fn create_blank_branch_node(
        seq_no: SeqNo,
        digest: Digest,
        client_commands: Vec<StoredMessage<D>>,
    ) -> Self {
        Self {
            decision_header: DecisionNodeHeader::new(
                seq_no,
                Some(Digest::blank()),
                digest,
                client_commands.len(),
            ),
            client_commands,
        }
    }

    #[must_use]
    pub fn create_root(
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

    pub(crate) fn into_commands(self) -> Vec<StoredMessage<D>> {
        self.client_commands
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
            decision_header: self.decision_header,
            client_commands: self.client_commands.clone(),
        }
    }
}

impl<D> Debug for DecisionNode<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "DecisionNode {{ {:?} }}", self.decision_header)
    }
}

impl<D> From<DecisionNode<D>> for (DecisionNodeHeader, Vec<StoredMessage<D>>) {
    fn from(value: DecisionNode<D>) -> Self {
        (value.decision_header, value.client_commands)
    }
}

pub struct DecisionHandler<QC> {
    latest_qc: Option<QC>,
    latest_prepare_qc: Option<QC>,
    latest_locked_qc: Option<QC>,
}

impl<QC> DecisionHandler<QC>
where
    QC: TQuorumCertificate,
{
    pub fn safe_node<D>(&self, node: &DecisionNode<D>, qc: &QC) -> bool 
    where QC: Debug {
        
        debug!("Checking if node is safe: {:?}. Latest QC: {:?}", node, self.latest_prepare_qc_ref());
        match (node.decision_header.previous_block, self.latest_prepare_qc_ref().cloned()) {
            (Some(prev), Some(latest_qc)) => {
                prev == latest_qc.decision_node().current_block_digest
                    && qc.sequence_number() > latest_qc.sequence_number()
            }
            (None, None) => true,
            
            _ => true,
        }
    }

    pub(crate) fn latest_qc_ref(&self) -> Option<&QC> {
        self.latest_qc.as_ref()
    }

    pub(crate) fn latest_prepare_qc_ref(&self) -> Option<&QC> {
        self.latest_prepare_qc.as_ref()
    }

    pub(crate) fn latest_locked_qc_ref(&self) -> Option<&QC> {
        self.latest_locked_qc.as_ref()
    }

    pub(crate) fn latest_qc(&self) -> Option<QC> {
        self.latest_qc.clone()
    }

    fn install_latest_qc(&mut self, qc: QC) {
        self.latest_qc = Some(qc);
    }

    pub(crate) fn install_latest_prepare_qc(&mut self, qc: QC) {
        self.latest_prepare_qc = Some(qc);
    }

    pub(crate) fn install_latest_locked_qc(&mut self, qc: QC) {
        self.latest_locked_qc = Some(qc);
    }
}

impl<QC> Default for DecisionHandler<QC> {
    fn default() -> Self {
        Self {
            latest_qc: Option::default(),
            latest_prepare_qc: Option::default(),
            latest_locked_qc: Option::default(),
        }
    }
}
