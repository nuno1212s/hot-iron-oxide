use atlas_metrics::{MetricLevel, MetricRegistry};
use atlas_metrics::metrics::MetricKind;


pub(crate) const SIGNATURE_PROPOSAL_LATENCY: &str = "SIGNATURE_PROPOSAL_LATENCY";
pub(crate) const SIGNATURE_PROPOSAL_LATENCY_ID: usize = 106;

pub(crate) const SIGNATURE_VOTE_LATENCY: &str = "SIGNATURE_VOTE_LATENCY";
pub(crate) const SIGNATURE_VOTE_LATENCY_ID: usize = 107;

pub(crate) const END_TO_END_LATENCY: &str = "END_TO_END_LATENCY";
pub(crate) const END_TO_END_LATENCY_ID: usize = 100;

#[must_use]
pub fn metrics() -> Vec<MetricRegistry> {
    let mut registered = vec![
        (
            END_TO_END_LATENCY_ID,
            END_TO_END_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
            1,
        )
            .into(),
        (
            SIGNATURE_PROPOSAL_LATENCY_ID,
            SIGNATURE_PROPOSAL_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
            1,
        )
            .into(),
        (
            SIGNATURE_VOTE_LATENCY_ID,
            SIGNATURE_VOTE_LATENCY.to_string(),
            MetricKind::Duration,
            MetricLevel::Info,
        )
            .into(),
    ];
    
    registered.append(&mut crate::hot_iron::metric::metrics());
    registered.append(&mut crate::chained::metrics::metrics());

    registered
}