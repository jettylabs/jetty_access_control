use serde::{Deserialize, Serialize};

/// Snowflake Database entry.
#[derive(Clone, Default, Deserialize, Serialize, Debug)]
pub struct Database {
    /// The Database name in Snowflake.
    pub name: String,
}
