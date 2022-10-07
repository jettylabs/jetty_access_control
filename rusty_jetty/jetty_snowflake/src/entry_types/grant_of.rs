use serde::Deserialize;
use structmap::FromMap;
use structmap_derive::FromMap;

/// Snowflake entry for a grant to a role.
#[derive(FromMap, Default, Deserialize, Debug)]
pub struct GrantOf {
    /// The role name in Snowflake.
    pub role: String,
    pub granted_to: String,
    pub grantee_name: String,
}
