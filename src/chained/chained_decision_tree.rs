use atlas_common::collections::HashMap;
use crate::decision_tree::{DecisionNode, DecisionNodeHeader};

pub struct PendingDecisionNodes<D> {
    map: HashMap<DecisionNodeHeader, DecisionNode<D>>
}

impl<D> PendingDecisionNodes<D> {
    
    pub(in super) fn new() -> Self {
        Self {
            map: HashMap::default()
        }
    }
    
    pub(in super) fn insert(&mut self, node: DecisionNode<D>) {
        self.map.insert(*node.decision_header(), node);
    }
    
    pub(in super) fn get(&self, header: &DecisionNodeHeader) -> Option<&DecisionNode<D>> {
        self.map.get(header)
    }
    
    pub(in super) fn remove(&mut self, header: &DecisionNodeHeader) -> Option<DecisionNode<D>> {
        self.map.remove(header)
    }
    
}

impl<D> Default for PendingDecisionNodes<D> {
    fn default() -> Self {
        Self::new()
    }
}