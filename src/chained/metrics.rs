use crate::metric::END_TO_END_LATENCY_ID;
use atlas_metrics::metrics::metric_duration;
use atlas_metrics::MetricRegistry;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) const VOTE_SEND_LATENCY: &str = "VOTE_SEND_LATENCY";
pub(crate) const VOTE_SEND_LATENCY_ID: usize = 150;

pub(crate) const VOTE_RECEIVED_LATENCY: &str = "VOTE_RECEIVED_LATENCY";
pub(crate) const VOTE_RECEIVED_LATENCY_ID: usize = 151;

pub(crate) const PROPOSAL_SEND_LATENCY: &str = "PROPOSAL_SEND_LATENCY";
pub(crate) const PROPOSAL_SEND_LATENCY_ID: usize = 152;

pub(crate) struct ConsensusMetric {
    replica_consensus_decision_metric: ReplicaConsensusDecisionMetric,
    metric_type: ConsensusMetricType,
}
pub(crate) enum ConsensusMetricType {
    ReplicaConsensus,
    NextLeader(LeaderDecisionMetric),
    Leader(LeaderDecisionMetric),
}

impl ConsensusMetric {
    pub fn replica_consensus_decision() -> Self {
        Self {
            replica_consensus_decision_metric: ReplicaConsensusDecisionMetric::default(),
            metric_type: ConsensusMetricType::ReplicaConsensus,
        }
    }

    pub fn next_leader_decision() -> Self {
        Self {
            replica_consensus_decision_metric: ReplicaConsensusDecisionMetric::default(),
            metric_type: ConsensusMetricType::NextLeader(LeaderDecisionMetric::default()),
        }
    }

    pub fn leader_decision() -> Self {
        Self {
            replica_consensus_decision_metric: ReplicaConsensusDecisionMetric::default(),
            metric_type: ConsensusMetricType::Leader(LeaderDecisionMetric::default()),
        }
    }

    pub(crate) fn as_replica_consensus_decision(&self) -> &ReplicaConsensusDecisionMetric {
        &self.replica_consensus_decision_metric
    }

    pub(crate) fn as_next_leader_decision(&self) -> &LeaderDecisionMetric {
        match &self.metric_type {
            ConsensusMetricType::NextLeader(next_leader) => next_leader,
            _ => unreachable!("Not a NextLeaderDecision metric"),
        }
    }

    pub(crate) fn as_leader_decision(&self) -> &LeaderDecisionMetric {
        match &self.metric_type {
            ConsensusMetricType::Leader(leader) => leader,
            _ => unreachable!("Not a LeaderDecision metric"),
        }
    }
}

#[derive(Default, Clone)]
pub(crate) struct ReplicaConsensusDecisionMetric(Arc<Mutex<ReplicaConsensusDecisionMetricInner>>);

impl ReplicaConsensusDecisionMetric {
    pub fn register_proposal_received(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.register_proposal_received();
    }

    pub fn register_vote_sent(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.register_vote_sent();
    }

    pub fn register_finalized_decision(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.register_finalized_decision();
    }
}

#[derive(Default)]
pub(crate) struct ReplicaConsensusDecisionMetricInner {
    received_proposal: Option<Instant>,
    vote_sent: Option<Instant>,
    finalized_decision: Option<Instant>,
}

impl ReplicaConsensusDecisionMetricInner {
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
pub(crate) struct LeaderDecisionMetric(Arc<Mutex<LeaderDecisionMetricInner>>);

impl LeaderDecisionMetric {
    pub fn register_vote_received(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.register_vote_received();
    }

    pub fn register_last_vote_received(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.register_last_vote_received();
    }

    pub fn register_proposal_sent(&self) {
        let mut inner = self.0.lock().unwrap();
        inner.register_proposal_sent();
    }
}

impl Clone for LeaderDecisionMetric {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[derive(Default)]
pub(crate) struct LeaderDecisionMetricInner {
    first_vote_received: Option<Instant>,
    last_vote_received: Option<Instant>,
}

impl LeaderDecisionMetricInner {
    fn register_vote_received(&mut self) {
        if self.first_vote_received.is_some() {
            return;
        }

        self.first_vote_received = Some(Instant::now());
    }

    fn register_last_vote_received(&mut self) {
        if self.last_vote_received.is_some() {
            return;
        }

        if let Some(start) = self.first_vote_received {
            metric_duration(VOTE_RECEIVED_LATENCY_ID, start.elapsed());
        }

        self.last_vote_received = Some(Instant::now());
    }

    fn register_proposal_sent(&mut self) {
        if let Some(start) = self.last_vote_received {
            metric_duration(PROPOSAL_SEND_LATENCY_ID, start.elapsed());
        }
    }
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
        (
            VOTE_RECEIVED_LATENCY_ID,
            VOTE_RECEIVED_LATENCY.to_string(),
            atlas_metrics::metrics::MetricKind::Duration,
            atlas_metrics::MetricLevel::Info,
        )
            .into(),
        (
            PROPOSAL_SEND_LATENCY_ID,
            PROPOSAL_SEND_LATENCY.to_string(),
            atlas_metrics::metrics::MetricKind::Duration,
            atlas_metrics::MetricLevel::Info,
        )
            .into(),
    ]
}
