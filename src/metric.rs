use crate::messages::{ProposalType, ProposalTypes, VoteType, VoteTypes};
use atlas_metrics::metrics::{metric_duration, MetricKind};
use atlas_metrics::{MetricLevel, MetricRegistry};
use enum_map::EnumMap;
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
    Leader(
        LeaderConsensusDecisionMetric,
        ReplicaConsensusDecisionMetric,
    ),
    Replica(ReplicaConsensusDecisionMetric),
}

macro_rules! update_instant {
    ($self:ident, $field:ident, $instant: expr) => {
        if $self.$field.is_none() {
            $self.$field = Some($instant);
        }
    };
}

#[derive(Getters)]
pub(crate) struct LeaderConsensusDecisionMetric {
    received_votes: EnumMap<VoteTypes, Option<Instant>>,
    sent_proposals: EnumMap<ProposalTypes, Option<Instant>>,
}

#[derive(Getters)]
pub(crate) struct ReplicaConsensusDecisionMetric {
    received_proposals: EnumMap<ProposalTypes, Option<Instant>>,
    sent_votes: EnumMap<VoteTypes, Option<Instant>>,
    finalized_proposal: Option<Instant>,
}

impl ConsensusDecisionMetric {
    #[must_use]
    pub(crate) fn leader() -> Self {
        Self::Leader(
            LeaderConsensusDecisionMetric::default(),
            ReplicaConsensusDecisionMetric::default(),
        )
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
    pub(crate) fn register_vote_received(&mut self, vote_type: VoteTypes) {
        if self.received_votes[vote_type].is_none() {
            self.received_votes[vote_type] = Some(Instant::now());
        }
    }

    pub(crate) fn register_proposal_sent(&mut self, proposal_type: ProposalTypes) {
        self.sent_proposals[proposal_type] = Some(Instant::now());

        match proposal_type {
            ProposalTypes::Prepare => {
                metric_duration(
                    PREPARE_LATENCY_ID,
                    self.received_votes[VoteTypes::NewView]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
            ProposalTypes::PreCommit => {
                metric_duration(
                    PRE_COMMIT_LATENCY_ID,
                    self.received_votes[VoteTypes::PrepareVote]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
            ProposalTypes::Commit => {
                metric_duration(
                    COMMIT_LATENCY_ID,
                    self.received_votes[VoteTypes::PreCommitVote]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
            ProposalTypes::Decide => {
                metric_duration(
                    DECIDED_LATENCY_ID,
                    self.received_votes[VoteTypes::CommitVote]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
        }
    }
}

impl ReplicaConsensusDecisionMetric {
    pub(crate) fn register_vote_sent(&mut self, vote_type: VoteTypes) {
        self.sent_votes[vote_type] = Some(Instant::now());
    }

    pub(crate) fn register_proposal_received(&mut self, proposal_type: ProposalTypes) {
        self.received_proposals[proposal_type] = Some(Instant::now());

        match proposal_type {
            ProposalTypes::Prepare => {
                metric_duration(
                    PREPARE_LATENCY_ID,
                    self.sent_votes[VoteTypes::NewView]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
            ProposalTypes::PreCommit => {
                metric_duration(
                    PRE_COMMIT_LATENCY_ID,
                    self.sent_votes[VoteTypes::PreCommitVote]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
            ProposalTypes::Commit => {
                metric_duration(
                    PRE_COMMIT_LATENCY_ID,
                    self.sent_votes[VoteTypes::PreCommitVote]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
            ProposalTypes::Decide => {
                metric_duration(
                    COMMIT_LATENCY_ID,
                    self.sent_votes[VoteTypes::CommitVote]
                        .as_ref()
                        .map_or_else(Duration::default, Instant::elapsed),
                );
            }
        }
    }
    
    pub(crate) fn register_decision_finalized(&mut self) {
        self.finalized_proposal = Some(Instant::now());
        
        metric_duration(
            DECIDED_LATENCY_ID,
            self.received_proposals[ProposalTypes::Commit]
                .as_ref()
                .map_or_else(Duration::default, Instant::elapsed),
        );
    }
}

impl Default for ReplicaConsensusDecisionMetric {
    fn default() -> Self {
        Self {
            received_proposals: EnumMap::default(),
            sent_votes: EnumMap::default(),
            finalized_proposal: None,
        }
    }
}

impl Default for LeaderConsensusDecisionMetric {
    fn default() -> Self {
        Self {
            received_votes: EnumMap::default(),
            sent_proposals: EnumMap::default(),
        }
    }
}