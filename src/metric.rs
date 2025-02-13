use atlas_metrics::metrics::MetricKind;
use atlas_metrics::{MetricLevel, MetricRegistry};

pub(crate) const END_TO_END_LATENCY : &str = "END_TO_END_LATENCY";
pub(crate) const END_TO_END_LATENCY_ID: usize = 100;


pub fn metrics() -> Vec<MetricRegistry> {
    
    vec![
        (END_TO_END_LATENCY_ID, END_TO_END_LATENCY.to_string(), MetricKind::Duration, MetricLevel::Debug, 1).into()
    ]
    
}