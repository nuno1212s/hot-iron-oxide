use std::sync::{Arc, LazyLock};

pub mod chained;
pub mod config;
pub mod crypto;
mod decision_tree;
mod req_aggr;
pub mod view;
pub mod hot_iron;
pub mod metric;
mod serialize;

static MOD_NAME: LazyLock<Arc<str>> = LazyLock::new(|| Arc::from("HOT-IRON"));

pub(crate) fn get_n_for_f(f: usize) -> usize {
    3 * f + 1
}

pub(crate) fn get_quorum_for_n(n: usize) -> usize {
    2 * get_f_for_n(n) + 1
}

pub(crate) fn get_f_for_n(n: usize) -> usize {
    (n - 1) / 3
}