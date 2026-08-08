use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TimingRecord {
    pub(crate) stage: String,
    pub(crate) started_at_utc: String,
    pub(crate) ended_at_utc: String,
    pub(crate) wall_seconds: f64,
    pub(crate) result: String,
    pub(crate) cache_status: String,
    pub(crate) reason: String,
    pub(crate) input_digest: String,
    pub(crate) output_digest: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TimingReport {
    pub(crate) schema_version: u32,
    pub(crate) command: String,
    pub(crate) started_at_utc: String,
    pub(crate) ended_at_utc: Option<String>,
    pub(crate) result: String,
    pub(crate) stages: Vec<TimingRecord>,
    pub(crate) categories: BTreeMap<String, TimingCategory>,
    pub(crate) integrity_cache: BTreeMap<String, IntegrityCacheStats>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct TimingCategory {
    pub(crate) wall_seconds: f64,
    pub(crate) operations: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct IntegrityCacheStats {
    pub(crate) hits: u64,
    pub(crate) misses: u64,
}
