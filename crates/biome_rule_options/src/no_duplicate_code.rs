use biome_deserialize_macros::{Deserializable, Merge};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Deserializable, Merge, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct NoDuplicateCodeOptions {
    /// The minimum number of consecutive identical tokens before a code block is considered duplicate.
    #[serde(default = "default_threshold")]
    pub threshold: usize,
}

fn default_threshold() -> usize {
    100
}
