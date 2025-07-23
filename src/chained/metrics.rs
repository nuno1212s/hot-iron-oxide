use std::time::Instant;
use atlas_metrics::MetricRegistry;
use atlas_metrics::metrics::metric_duration;
use crate::metric::END_TO_END_LATENCY_ID;

pub(crate) const VOTE_SEND_LATENCY: &str = "VOTE_SEND_LATENCY";
pub(crate) const VOTE_SEND_LATENCY_ID: usize = 150;

pub enum ConsensusMetric {
    ReplicaConsensusDecision(ReplicaConsensusDecisionMetric),
    NextLeaderDecision(ReplicaConsensusDecisionMetric, NextLeaderDecisionMetric),
    LeaderDecision(ReplicaConsensusDecisionMetric)
}

impl ConsensusMetric {
    pub fn replica_consensus_decision() -> Self {
        ConsensusMetric::ReplicaConsensusDecision(ReplicaConsensusDecisionMetric::default())
    }

    pub fn next_leader_decision() -> Self {
        ConsensusMetric::NextLeaderDecision(
            ReplicaConsensusDecisionMetric::default(),
            NextLeaderDecisionMetric::default(),
        )
    }

    pub fn leader_decision() -> Self {
        ConsensusMetric::LeaderDecision(ReplicaConsensusDecisionMetric::default())
    }
}

#[derive(Default)]
struct ReplicaConsensusDecisionMetric {
    received_proposal: Option<Instant>,
    vote_sent: Option<Instant>,
    finalized_decision: Option<Instant>,
}

impl ReplicaConsensusDecisionMetric {
    
    fn register_proposal_received(&mut self) {
        if self.received_proposal.is_some() {
            return;
        }
        
        self.received_proposal = Some(Instant::now());
    }
    
    fn register_vote_sent(&mut self) {
        if self.vote_sent.is_some() {
            return;
        }
        
        if let Some(start) = self.received_proposal {
            metric_duration(VOTE_SEND_LATENCY_ID, start.elapsed());
        }
        
        self.vote_sent = Some(Instant::now());
    }
    
    fn register_finalized_decision(&mut self) {
        if self.finalized_decision.is_some() {
            return;
        }
        
        if let Some(start) = self.received_proposal {
            metric_duration(END_TO_END_LATENCY_ID, start.elapsed());
        }
        
        self.finalized_decision = Some(Instant::now());
    }
    
}

#[derive(Default)]
struct NextLeaderDecisionMetric {
    first_vote_received: Option<Instant>,
    last_vote_received: Option<Instant>,
}

impl NextLeaderDecisionMetric {
    
}

pub fn metrics() -> Vec<MetricRegistry> {
    vec![
        (
            VOTE_SEND_LATENCY_ID,
            VOTE_SEND_LATENCY.to_string(),
            atlas_metrics::metrics::MetricKind::Duration,
            atlas_metrics::MetricLevel::Info,
        )
            .into(),
        (
            END_TO_END_LATENCY_ID,
            crate::metric::END_TO_END_LATENCY.to_string(),
            atlas_metrics::metrics::MetricKind::Duration,
            atlas_metrics::MetricLevel::Info,
        )
            .into(),
    ]
}