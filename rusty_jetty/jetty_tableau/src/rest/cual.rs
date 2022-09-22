use std::sync::Once;

use anyhow::{bail, Result};

use jetty_core::{
    connectors::AssetType,
    cual::{Cual, Cualable},
};

static mut CUAL_PREFIX: String = String::new();
static INIT_CUAL_PREFIX: Once = Once::new();

macro_rules! cual {
    ($db:expr) => {
        Cual::new(format!("{}://{}", "snowflake", $db))
    };
    ($db:expr, $schema:expr) => {
        Cual::new(format!("{}://{}/{}", "snowflake", $db, $schema))
    };
    ($db:expr, $schema:expr, $table:expr) => {
        Cual::new(format!("{}://{}/{}/{}", "snowflake", $db, $schema, $table))
    };
}

pub(crate) enum TableauAssetType {
    Project,
    Datasource,
    Flow,
    Workbook,
}

impl TableauAssetType {
    /// Used for cual construction, the str representation of
    /// the asset type helps identify the asset within Tableau.
    fn as_str(&self) -> &'static str {
        match self {
            TableauAssetType::Project => "project",
            TableauAssetType::Datasource => "datasource",
            TableauAssetType::Flow => "flow",
            TableauAssetType::Workbook => "workbook",
        }
    }
}

pub(crate) fn get_tableau_cual(asset_type: TableauAssetType, id: &str) -> Result<Cual> {
    Ok(Cual::new(format!(
        "{}{}/{}",
        get_cual_prefix()?,
        asset_type.as_str(),
        id
    )))
}

// Accessing a `static mut` is unsafe much of the time, but if we do so
// in a synchronized fashion (e.g., write once or read all) then we're
// good to go!
//
// This function will only set the string once, and will
// otherwise always effectively be a no-op.
pub(crate) fn set_cual_prefix(server_name: &str, site_name: &str) {
    unsafe {
        INIT_CUAL_PREFIX.call_once(|| {
            CUAL_PREFIX = format!("tableau://{}/{}", &server_name, &site_name);
        });
    }
}

fn get_cual_prefix<'a>() -> Result<&'a str> {
    if INIT_CUAL_PREFIX.is_completed() {
        // CUAL_PREFIX is set by a Once and is safe to use after initialization.
        unsafe { Ok(&CUAL_PREFIX) }
    } else {
        bail!("cual prefix was not yet set")
    }
}
