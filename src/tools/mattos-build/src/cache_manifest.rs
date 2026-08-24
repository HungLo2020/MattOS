use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) const STAGE_MANIFEST_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug)]
pub(crate) struct StageSpec {
    pub(crate) id: String,
    pub(crate) source_inputs: Vec<PathBuf>,
    pub(crate) configuration_inputs: Vec<PathBuf>,
    pub(crate) tools: Vec<String>,
    pub(crate) dependencies: Vec<String>,
    pub(crate) outputs: Vec<PathBuf>,
    pub(crate) recipe: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct InventoryEntry {
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) mode: u32,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) size: u64,
    pub(crate) content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct StageInputs {
    pub(crate) source_digest: String,
    pub(crate) configuration_digest: String,
    pub(crate) tool_digest: String,
    /// Exact tool identities are retained separately from the development
    /// reuse key. This is provenance, not an eager rebuild trigger.
    #[serde(default)]
    pub(crate) build_provenance_digest: String,
    pub(crate) environment_digest: String,
    pub(crate) dependency_digests: BTreeMap<String, String>,
    pub(crate) full_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct ToolIdentity {
    pub(crate) resolved_path: String,
    pub(crate) executable_sha256: String,
    pub(crate) version: String,
    pub(crate) target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct DependencyIdentity {
    pub(crate) input_digest: String,
    pub(crate) output_digest: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
pub(crate) struct StageInputDetails {
    pub(crate) schema_version: u32,
    pub(crate) recipe: String,
    pub(crate) source: BTreeMap<String, String>,
    pub(crate) configuration: BTreeMap<String, String>,
    pub(crate) environment: BTreeMap<String, String>,
    pub(crate) tools: BTreeMap<String, ToolIdentity>,
    pub(crate) dependencies: BTreeMap<String, DependencyIdentity>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct StageManifest {
    pub(crate) schema_version: u32,
    pub(crate) stage: String,
    pub(crate) inputs: StageInputs,
    #[serde(default)]
    pub(crate) input_details: StageInputDetails,
    pub(crate) expected_outputs: Vec<InventoryEntry>,
    pub(crate) output_content_digest: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_identity_excludes_upstream_input_identity_from_consumer_key() {
        let first = DependencyIdentity {
            input_digest: "upstream-input-one".to_string(),
            output_digest: "same-output".to_string(),
        };
        let second = DependencyIdentity {
            input_digest: "upstream-input-two".to_string(),
            output_digest: "same-output".to_string(),
        };
        assert_ne!(first, second);
        assert_eq!(first.output_digest, second.output_digest);
    }
}
