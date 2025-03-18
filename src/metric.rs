use atlas_metrics::metrics::{metric_duration, MetricKind};
use atlas_metrics::{MetricLevel, MetricRegistry};
use getset::Getters;
use std::time::{Duration, Instant};

pub(crate) const END_TO_END_LATENCY: &str = "END_TO_END_LATENCY";
pub(crate) const END_TO_END_LATENCY_ID: usize = 100;

pub(crate) const PREPARE_LATENCY: &str = "PREPARE_LATENCY";
pub(crate) const PREPARE_LATENCY_ID: usize = 101;

pub(crate) const PRE_COMMIT_LATENCY: &str = "PRE_COMMIT_LATENCY";
pub(crate) const PRE_COMMIT_LATENCY_ID: usize = 102;

pub(crate) const COMMIT_LATENCY: &str = "COMMIT_LATENCY";
pub(crate) const COMMIT_LATENCY_ID: usize = 103;

pub(crate) const DECIDED_LATENCY: &str = "DECIDED_LATENCY";
pub(crate) const DECIDED_LATENCY_ID: usize = 104;

#[must_use]
pub fn metrics() -> Vec<MetricRegistry> {
    vec![
        (
            END_TO_END_LATENCY_ID,
            END_TO_END_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
            1,
        )
            .into(),
        (
            PREPARE_LATENCY_ID,
            PREPARE_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
            1,
        )
            .into(),
        (
            PRE_COMMIT_LATENCY_ID,
            PRE_COMMIT_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
            1,
        )
            .into(),
        (
            COMMIT_LATENCY_ID,
            COMMIT_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
            1,
        )
            .into(),
        (
            DECIDED_LATENCY_ID,
            DECIDED_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
            1,
        )
            .into(),
    ]
}

pub(crate) enum ConsensusDecisionMetric {
    Leader(LeaderConsensusDecisionMetric, ReplicaConsensusDecisionMetric),
    Replica(ReplicaConsensusDecisionMetric),
}

macro_rules! update_instant {
    ($self:ident, $field:ident, $instant: expr) => {
        if $self.$field.is_none() {
            $self.$field = Some($instant);
        }
    };
}

#[derive(Default, Getters)]
pub(crate) struct LeaderConsensusDecisionMetric {
    first_new_view_received: Option<Instant>,
    prepare_sent_time: Option<Instant>,
    first_prepare_vote: Option<Instant>,
    pre_commit_sent_time: Option<Instant>,
    first_pre_commit_vote: Option<Instant>,
    commit_sent_time: Option<Instant>,
    first_commit_vote: Option<Instant>,
    decided_sent_time: Option<Instant>,
    decided_time: Option<Instant>,
}

#[derive(Default, Getters)]
pub(crate) struct ReplicaConsensusDecisionMetric {
    start_instant: Option<Instant>,
    prepare_received: Option<Instant>,
    pre_commit_received: Option<Instant>,
    commit_received: Option<Instant>,
    decided_received: Option<Instant>,
    decided_time: Option<Instant>,
}

impl ConsensusDecisionMetric {
    #[must_use]
    pub(crate) fn leader() -> Self {
        Self::Leader(LeaderConsensusDecisionMetric::default(), ReplicaConsensusDecisionMetric::default())
    }

    #[must_use]
    pub(crate) fn replica() -> Self {
        Self::Replica(ReplicaConsensusDecisionMetric::default())
    }

    pub(crate) fn as_leader(&mut self) -> &mut LeaderConsensusDecisionMetric {
        match self {
            Self::Leader(metric, _) => metric,
            _ => panic!("Expected Leader metric"),
        }
    }

    pub(crate) fn as_replica(&mut self) -> &mut ReplicaConsensusDecisionMetric {
        match self {
            Self::Replica(metric) | Self::Leader(_, metric) => metric,
        }
    }
}

impl LeaderConsensusDecisionMetric {
    pub(crate) fn register_new_view_received(&mut self) {
        update_instant!(self, first_new_view_received, Instant::now());
    }

    pub(crate) fn register_prepare_sent(&mut self) {
        metric_duration(
            PREPARE_LATENCY_ID,
            self.first_new_view_received
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, prepare_sent_time, Instant::now());
    }

    pub(crate) fn register_prepare_vote(&mut self) {
        update_instant!(self, first_prepare_vote, Instant::now());
    }

    pub(crate) fn register_pre_commit_sent(&mut self) {
        metric_duration(
            PRE_COMMIT_LATENCY_ID,
            self.first_prepare_vote
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, pre_commit_sent_time, Instant::now());
    }

    pub(crate) fn register_pre_commit_vote(&mut self) {
        update_instant!(self, first_pre_commit_vote, Instant::now());
    }

    pub(crate) fn register_commit_sent(&mut self) {
        metric_duration(
            COMMIT_LATENCY_ID,
            self.first_pre_commit_vote
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, commit_sent_time, Instant::now());
    }

    pub(crate) fn register_commit_vote(&mut self) {
        update_instant!(self, first_commit_vote, Instant::now());
    }

    pub(crate) fn register_decided_sent(&mut self) {
        metric_duration(
            DECIDED_LATENCY_ID,
            self.first_commit_vote
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, decided_sent_time, Instant::now());
    }

    pub(crate) fn register_decided(&mut self) {
        metric_duration(
            END_TO_END_LATENCY_ID,
            self.first_new_view_received
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, decided_time, Instant::now());
    }
}

impl ReplicaConsensusDecisionMetric {
    pub(crate) fn register_new_view_sent(&mut self) {
        update_instant!(self, start_instant, Instant::now());
    }

    pub(crate) fn register_prepare_received(&mut self) {
        metric_duration(
            PREPARE_LATENCY_ID,
            self.start_instant
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, prepare_received, Instant::now());
    }

    pub(crate) fn register_pre_commit_proposal(&mut self) {
        metric_duration(
            PRE_COMMIT_LATENCY_ID,
            self.prepare_received
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, pre_commit_received, Instant::now());
    }

    pub(crate) fn register_commit_proposal(&mut self) {
        metric_duration(
            COMMIT_LATENCY_ID,
            self.pre_commit_received
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, commit_received, Instant::now());
    }

    pub(crate) fn register_decided_received(&mut self) {
        metric_duration(
            COMMIT_LATENCY_ID,
            self.commit_received
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, decided_received, Instant::now());
    }

    pub(crate) fn register_decided(&mut self) {
        metric_duration(
            END_TO_END_LATENCY_ID,
            self.start_instant
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
        update_instant!(self, decided_time, Instant::now());
    }
}
